//! Event processor for simplified sidecar.
//!
//! Processes events asynchronously, updating:
//! - `state.md` with session context
//! - `patches/staged/` with commit patches (L2)

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(feature = "tauri")]
use std::sync::Arc;

use tokio::sync::mpsc;

#[cfg(feature = "tauri")]
use tauri::AppHandle;

use super::commits::{BoundaryReason, PatchManager};
use super::events::{CommitBoundaryDetector, EventType, SessionEvent, SidecarEvent};
use super::session::Session;
use super::synthesis::{
    create_state_synthesizer, generate_template_message, StateSynthesisInput, SynthesisBackend,
    SynthesisConfig, SynthesisInput,
};

/// Event sent to the processor
#[derive(Debug)]
pub enum ProcessorTask {
    /// Process a new event
    ProcessEvent {
        session_id: String,
        event: Box<SessionEvent>,
    },
    /// End a session
    EndSession { session_id: String },
    /// Shutdown the processor
    Shutdown,
}

/// Configuration for the processor
#[derive(Clone)]
pub struct ProcessorConfig {
    /// Directory containing sessions
    pub sessions_dir: PathBuf,
    /// Whether to generate staged patches (L2)
    pub generate_patches: bool,
    /// Synthesis configuration for commit messages
    pub synthesis: SynthesisConfig,
    /// App handle for emitting events (Tauri only)
    #[cfg(feature = "tauri")]
    pub app_handle: Option<Arc<AppHandle>>,
}

impl std::fmt::Debug for ProcessorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessorConfig")
            .field("sessions_dir", &self.sessions_dir)
            .field("generate_patches", &self.generate_patches)
            .field("synthesis", &self.synthesis)
            .finish()
    }
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            sessions_dir: super::session::default_sessions_dir(),
            generate_patches: true,
            synthesis: SynthesisConfig::default(),
            #[cfg(feature = "tauri")]
            app_handle: None,
        }
    }
}

impl ProcessorConfig {
    /// Emit a sidecar event to the frontend
    #[cfg(feature = "tauri")]
    pub fn emit_event(&self, event: SidecarEvent) {
        use tauri::Emitter;
        if let Some(handle) = &self.app_handle {
            if let Err(e) = handle.emit("sidecar-event", &event) {
                tracing::warn!("Failed to emit sidecar event from processor: {}", e);
            }
        }
    }

    /// No-op emit_event for non-tauri builds
    #[cfg(not(feature = "tauri"))]
    pub fn emit_event(&self, _event: SidecarEvent) {
        // No-op when not using tauri
    }
}

/// Tracks file changes for patch generation
#[derive(Debug, Default)]
struct FileChangeTracker {
    /// Files changed since last commit boundary
    files: Vec<PathBuf>,
}

impl FileChangeTracker {
    fn new() -> Self {
        Self { files: Vec::new() }
    }

    fn record_change(&mut self, path: PathBuf) {
        if !self.files.contains(&path) {
            self.files.push(path);
        }
    }

    fn get_files(&self) -> Vec<PathBuf> {
        self.files.clone()
    }

    fn clear(&mut self) {
        self.files.clear();
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// State for a single session's processing
struct SessionProcessorState {
    /// Commit boundary detector
    boundary_detector: CommitBoundaryDetector,
    /// File change tracker for patch generation
    file_tracker: FileChangeTracker,
    /// All files modified during session (for state.md updates)
    all_modified_files: Vec<PathBuf>,
    /// Tool calls completed during session (for progress tracking)
    completed_tools: Vec<String>,
    /// Event count for this session
    event_count: u32,
}

impl SessionProcessorState {
    fn new() -> Self {
        Self {
            boundary_detector: CommitBoundaryDetector::new(),
            file_tracker: FileChangeTracker::new(),
            all_modified_files: Vec::new(),
            completed_tools: Vec::new(),
            event_count: 0,
        }
    }

    /// Record a modified file (deduplicates)
    fn record_modified_file(&mut self, path: PathBuf) {
        if !self.all_modified_files.contains(&path) {
            self.all_modified_files.push(path);
        }
    }

    /// Record a completed tool call
    fn record_tool_call(&mut self, tool_name: &str, success: bool) {
        let status = if success { "✓" } else { "✗" };
        self.completed_tools
            .push(format!("{} {}", tool_name, status));
    }
}

/// Event processor
pub struct Processor {
    task_tx: mpsc::Sender<ProcessorTask>,
    /// Handle to the processor task, used to await completion during shutdown
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Processor {
    /// Create a new processor and spawn its background task
    pub fn spawn(config: ProcessorConfig) -> Self {
        tracing::info!(
            "[processor] Spawning processor: synthesis.enabled={}, synthesis.backend={:?}",
            config.synthesis.enabled,
            config.synthesis.backend
        );

        let (task_tx, task_rx) = mpsc::channel(256);

        let task_handle = tokio::spawn(async move {
            run_processor(config, task_rx).await;
        });

        Self {
            task_tx,
            task_handle: Some(task_handle),
        }
    }

    /// Process an event (non-blocking, queues for async processing)
    pub fn process_event(&self, session_id: String, event: SessionEvent) {
        tracing::info!(
            "[processor] Queuing event: type={}, session={}",
            event.event_type.name(),
            session_id
        );
        let task = ProcessorTask::ProcessEvent {
            session_id,
            event: Box::new(event),
        };
        if let Err(e) = self.task_tx.try_send(task) {
            tracing::warn!("[processor] Failed to queue event for processing: {}", e);
        }
    }

    /// Signal session end
    pub fn end_session(&self, session_id: String) {
        let task = ProcessorTask::EndSession { session_id };
        if let Err(e) = self.task_tx.try_send(task) {
            tracing::warn!("Failed to queue session end: {}", e);
        }
    }

    /// Shutdown the processor and wait for it to complete all pending work
    ///
    /// This sends a shutdown signal and then waits for the processor task to finish,
    /// ensuring that any pending operations (like patch generation) complete.
    pub async fn shutdown(mut self) {
        // Send shutdown signal
        let _ = self.task_tx.send(ProcessorTask::Shutdown).await;

        // Wait for the processor task to actually finish
        if let Some(handle) = self.task_handle.take() {
            match handle.await {
                Ok(()) => tracing::debug!("Processor task completed successfully"),
                Err(e) => tracing::warn!("Processor task panicked: {}", e),
            }
        }
    }
}

/// Main processor loop
async fn run_processor(config: ProcessorConfig, mut task_rx: mpsc::Receiver<ProcessorTask>) {
    tracing::info!("Sidecar processor started");

    let mut session_states: HashMap<String, SessionProcessorState> = HashMap::new();

    while let Some(task) = task_rx.recv().await {
        match task {
            ProcessorTask::ProcessEvent { session_id, event } => {
                let session_state = session_states
                    .entry(session_id.clone())
                    .or_insert_with(SessionProcessorState::new);

                if let Err(e) = handle_event(&config, &session_id, &event, session_state).await {
                    tracing::error!("Failed to process event for {}: {}", session_id, e);
                }
            }
            ProcessorTask::EndSession { session_id } => {
                tracing::info!(
                    "[processor] EndSession task received for session: {}",
                    session_id
                );

                // Generate final patch if there are pending changes
                if let Some(session_state) = session_states.get_mut(&session_id) {
                    let file_count = session_state.file_tracker.get_files().len();
                    tracing::info!(
                        "[processor] Session {} ending: generate_patches={}, file_tracker has {} file(s)",
                        session_id,
                        config.generate_patches,
                        file_count
                    );

                    if config.generate_patches && !session_state.file_tracker.is_empty() {
                        tracing::info!(
                            "[processor] Generating patch for session {} with {} file(s)",
                            session_id,
                            file_count
                        );
                        if let Err(e) = generate_patch(
                            &config,
                            &session_id,
                            session_state,
                            BoundaryReason::SessionEnd,
                        )
                        .await
                        {
                            tracing::error!(
                                "Failed to generate final patch for {}: {}",
                                session_id,
                                e
                            );
                        }
                    } else if !config.generate_patches {
                        tracing::debug!(
                            "[processor] Patch generation disabled for session {}",
                            session_id
                        );
                    } else {
                        tracing::debug!(
                            "[processor] No files tracked for session {}, skipping patch generation",
                            session_id
                        );
                    }
                } else {
                    tracing::warn!(
                        "[processor] No session state found for session {}, cannot generate patch",
                        session_id
                    );
                }

                if let Err(e) = handle_end_session(&config, &session_id).await {
                    tracing::error!("Failed to end session {}: {}", session_id, e);
                }

                session_states.remove(&session_id);
            }
            ProcessorTask::Shutdown => {
                tracing::info!("Sidecar processor shutting down");
                break;
            }
        }
    }
}

/// Handle a single event
async fn handle_event(
    config: &ProcessorConfig,
    session_id: &str,
    event: &SessionEvent,
    session_state: &mut SessionProcessorState,
) -> Result<()> {
    session_state.event_count += 1;

    // Track file changes for L2 patch generation and state updates
    if config.generate_patches {
        track_file_changes(event, session_state);

        // Check for commit boundary
        if let Some(boundary_info) = session_state.boundary_detector.check_boundary(event) {
            let reason = parse_boundary_reason(&boundary_info.reason);
            if !session_state.file_tracker.is_empty() {
                generate_patch(config, session_id, session_state, reason).await?;
            }
        }
    }

    // Update state.md and log.md for significant events
    update_session_files(config, session_id, event, session_state).await?;

    tracing::debug!(
        "Processed event for session {}: {:?}",
        session_id,
        event.event_type.name()
    );
    Ok(())
}

/// Update session state.md and log.md based on event
async fn update_session_files(
    config: &ProcessorConfig,
    session_id: &str,
    event: &SessionEvent,
    session_state: &mut SessionProcessorState,
) -> Result<()> {
    tracing::info!(
        "[processor] update_session_files called: event_type={}, session_id={}",
        event.event_type.name(),
        session_id
    );

    // Load session
    let mut session = match Session::load(&config.sessions_dir, session_id).await {
        Ok(s) => {
            tracing::debug!("[processor] Session {} loaded successfully", session_id);
            s
        }
        Err(e) => {
            tracing::warn!("[processor] Could not load session {} for update: {}", session_id, e);
            return Ok(());
        }
    };

    // Track modified files and update state.md
    match &event.event_type {
        EventType::FileEdit {
            path, operation, ..
        } => {
            session_state.record_modified_file(path.clone());

            // Append to log for file operations
            let log_entry = format!(
                "**File {}**: `{}`",
                format_operation(operation),
                path.display()
            );
            if let Err(e) = session.append_log(&log_entry).await {
                tracing::warn!("Failed to append to log: {}", e);
            }

            // Note: State synthesis happens on AiResponse, not per-file-edit
        }
        EventType::ToolCall {
            tool_name,
            args_summary,
            success,
            ..
        } => {
            // Log tool calls with args and results
            let status = if *success { "✓" } else { "✗" };

            // Build detailed log entry
            let mut log_entry = format!("**Tool**: {} {}\n", tool_name, status);

            // Add args if present
            if !args_summary.is_empty() {
                log_entry.push_str(&format!("- **Args**: `{}`\n", args_summary));
            }

            // Add tool output/result if present
            if let Some(output) = &event.tool_output {
                // Truncate output for log readability
                let truncated = if output.len() > 500 {
                    format!("{}...", &output[..500])
                } else {
                    output.clone()
                };
                log_entry.push_str(&format!("- **Result**:\n```\n{}\n```\n", truncated));
            }

            if let Err(e) = session.append_log(&log_entry).await {
                tracing::warn!("Failed to append to log: {}", e);
            }

            // Track tool call for progress (state will be synthesized on AiResponse)
            session_state.record_tool_call(tool_name, *success);

            // Track files from tool call
            for path in &event.files_modified {
                session_state.record_modified_file(path.clone());
            }

            // Note: State synthesis happens on AiResponse, not per-tool-call
            // This avoids intermediate template updates and reduces LLM calls
        }
        EventType::UserPrompt { intent, .. } => {
            // Log user prompts
            let truncated = if intent.len() > 100 {
                format!("{}...", &intent[..100])
            } else {
                intent.clone()
            };
            let log_entry = format!("**User**: {}", truncated);
            if let Err(e) = session.append_log(&log_entry).await {
                tracing::warn!("Failed to append to log: {}", e);
            }

            // Update state.md with new user request via LLM synthesis
            tracing::info!(
                "[processor] UserPrompt received, synthesis.enabled={}, backend={:?}",
                config.synthesis.enabled,
                config.synthesis.backend
            );
            if config.synthesis.enabled {
                tracing::info!("[processor] Calling synthesize_state_update for user_prompt");
                if let Err(e) = synthesize_state_update(
                    config,
                    &mut session,
                    session_state,
                    "user_prompt",
                    intent,
                )
                .await
                {
                    tracing::error!(
                        "[sidecar] LLM state synthesis failed for user prompt: {}",
                        e
                    );
                }
            } else {
                tracing::warn!("[processor] Synthesis disabled, skipping state update for user_prompt");
            }
        }
        EventType::AiResponse { content, .. } => {
            // Log agent responses (truncated)
            let truncated = if content.len() > 100 {
                format!("{}...", &content[..100])
            } else {
                content.clone()
            };
            let log_entry = format!("**Agent**: {}", truncated);
            if let Err(e) = session.append_log(&log_entry).await {
                tracing::warn!("Failed to append to log: {}", e);
            }

            // Trigger LLM-based state synthesis on AI responses (completed turns)
            tracing::info!(
                "[processor] AiResponse received, synthesis.enabled={}, backend={:?}",
                config.synthesis.enabled,
                config.synthesis.backend
            );
            if config.synthesis.enabled {
                tracing::info!("[processor] Calling synthesize_state_update for ai_response");
                if let Err(e) = synthesize_state_update(
                    config,
                    &mut session,
                    session_state,
                    "ai_response",
                    &truncated,
                )
                .await
                {
                    tracing::error!(
                        "[sidecar] LLM state synthesis failed for AI response: {}",
                        e
                    );
                }
            } else {
                tracing::warn!("[processor] Synthesis disabled, skipping state update for ai_response");
            }
        }
        _ => {
            // Other events don't need state/log updates
        }
    }

    Ok(())
}

/// Format file operation for display
fn format_operation(op: &super::events::FileOperation) -> &'static str {
    match op {
        super::events::FileOperation::Create => "created",
        super::events::FileOperation::Modify => "modified",
        super::events::FileOperation::Delete => "deleted",
        super::events::FileOperation::Rename { .. } => "renamed",
    }
}

/// Synthesize an updated state.md using LLM
async fn synthesize_state_update(
    config: &ProcessorConfig,
    session: &mut Session,
    _session_state: &SessionProcessorState,
    event_type: &str,
    event_details: &str,
) -> Result<()> {
    tracing::info!(
        "[sidecar] Synthesizing state update via LLM (event_type={}, backend={:?})",
        event_type,
        config.synthesis.backend
    );

    // Read current state
    let current_state = session.read_state().await.unwrap_or_default();

    // Get files from git diff (more reliable than tracking tool calls)
    let git_changes = get_git_changes(&session.meta().cwd).await;
    let files: Vec<String> = git_changes
        .iter()
        .map(|gc| {
            if gc.diff.is_empty() {
                gc.path.clone()
            } else {
                // Include truncated diff info for context
                let diff_preview = if gc.diff.len() > 200 {
                    format!("{} (+{} lines)", gc.path, gc.diff.lines().count())
                } else {
                    format!("{}\n{}", gc.path, gc.diff)
                };
                diff_preview
            }
        })
        .collect();

    if !files.is_empty() {
        tracing::info!(
            "[sidecar] Git detected {} modified files: {:?}",
            files.len(),
            git_changes.iter().map(|g| &g.path).collect::<Vec<_>>()
        );
    }

    // Create synthesis input
    let input = StateSynthesisInput::new(
        current_state,
        event_type.to_string(),
        event_details.to_string(),
        files,
    );

    // Create synthesizer and generate updated state
    let synthesizer = create_state_synthesizer(&config.synthesis)?;
    let result = synthesizer.synthesize_state(&input).await?;

    // Write updated state
    session.update_state(&result.state_body).await?;

    tracing::info!(
        "[sidecar] State synthesized successfully using {} backend",
        result.backend
    );

    Ok(())
}

/// Represents a git change with file path and optional diff
#[derive(Debug)]
struct GitChange {
    path: String,
    diff: String,
}

/// Get modified files and their diffs using git
async fn get_git_changes(cwd: &std::path::Path) -> Vec<GitChange> {
    // First check if this is a git repo
    let is_git = tokio::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_git {
        tracing::debug!("[sidecar] Not a git repository, skipping git diff");
        return vec![];
    }

    // Get list of modified files (staged + unstaged + untracked)
    let output = match tokio::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("[sidecar] Failed to run git status: {}", e);
            return vec![];
        }
    };

    if !output.status.success() {
        return vec![];
    }

    let status_output = String::from_utf8_lossy(&output.stdout);
    let mut changes = Vec::new();

    for line in status_output.lines() {
        if line.len() < 4 {
            continue;
        }

        let status = &line[0..2];
        let path = line[3..].trim().to_string();

        // Skip binary files and build artifacts
        if is_binary_or_artifact(&path) {
            tracing::debug!("[sidecar] Skipping binary/artifact: {}", path);
            continue;
        }

        // Skip deleted files for diff
        if status.contains('D') {
            changes.push(GitChange {
                path: path.clone(),
                diff: "(deleted)".to_string(),
            });
            continue;
        }

        // Check if git considers this file binary
        if is_git_binary(cwd, &path).await {
            tracing::debug!("[sidecar] Skipping git-detected binary: {}", path);
            continue;
        }

        // Get diff for this file
        let diff = get_file_diff(cwd, &path).await;
        changes.push(GitChange { path, diff });
    }

    changes
}

/// Check if a file is likely a binary or build artifact based on path/extension
fn is_binary_or_artifact(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Common binary extensions
    let binary_extensions = [
        ".exe", ".dll", ".so", ".dylib", ".a", ".o", ".obj",
        ".pyc", ".pyo", ".class", ".jar", ".war",
        ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar",
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".svg",
        ".pdf", ".doc", ".docx", ".xls", ".xlsx",
        ".wasm", ".node",
    ];

    // Common build artifact directories
    let artifact_dirs = [
        "node_modules/", "target/", "build/", "dist/", "out/",
        ".git/", "__pycache__/", ".pytest_cache/", ".mypy_cache/",
        "vendor/", "bin/", "obj/",
    ];

    // Check extensions
    for ext in &binary_extensions {
        if path_lower.ends_with(ext) {
            return true;
        }
    }

    // Check artifact directories
    for dir in &artifact_dirs {
        if path_lower.contains(dir) {
            return true;
        }
    }

    // Files without extension in root are often binaries (Go, C, etc.)
    // But only if they don't look like common config files
    let filename = path.rsplit('/').next().unwrap_or(path);
    if !filename.contains('.') {
        // Allow common extensionless config files
        let allowed_extensionless = [
            "Makefile", "Dockerfile", "Jenkinsfile", "Vagrantfile",
            "README", "LICENSE", "CHANGELOG", "AUTHORS", "CONTRIBUTORS",
            "Gemfile", "Rakefile", "Procfile", "Brewfile",
            ".gitignore", ".gitattributes", ".dockerignore", ".editorconfig",
        ];
        if !allowed_extensionless.iter().any(|&f| filename == f || filename.starts_with('.')) {
            return true;
        }
    }

    false
}

/// Check if git considers a file binary using diff --numstat
async fn is_git_binary(cwd: &std::path::Path, file_path: &str) -> bool {
    let output = tokio::process::Command::new("git")
        .args(["diff", "--numstat", "HEAD", "--", file_path])
        .current_dir(cwd)
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            // Binary files show as "-\t-\t<filename>" in numstat
            stdout.starts_with("-\t-\t")
        }
        _ => false,
    }
}

/// Get the diff for a specific file
async fn get_file_diff(cwd: &std::path::Path, file_path: &str) -> String {
    // Try staged diff first, then unstaged
    let output = tokio::process::Command::new("git")
        .args(["diff", "HEAD", "--", file_path])
        .current_dir(cwd)
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let diff = String::from_utf8_lossy(&o.stdout).to_string();
            if diff.is_empty() {
                // File might be untracked, just note it as new
                "(new file)".to_string()
            } else {
                // Truncate very long diffs
                if diff.len() > 2000 {
                    format!(
                        "{}...\n(truncated, {} more lines)",
                        &diff[..2000],
                        diff.lines().count().saturating_sub(50)
                    )
                } else {
                    diff
                }
            }
        }
        _ => "(unable to get diff)".to_string(),
    }
}

/// Track file changes from an event
fn track_file_changes(event: &SessionEvent, session_state: &mut SessionProcessorState) {
    match &event.event_type {
        EventType::FileEdit { path, .. } => {
            tracing::debug!(
                "[processor] FileEdit event for path: {:?}, tracking change",
                path
            );
            session_state.file_tracker.record_change(path.clone());
        }
        EventType::ToolCall { tool_name, .. } => {
            if is_write_tool(tool_name) {
                if event.files_modified.is_empty() {
                    tracing::debug!(
                        "[processor] ToolCall {} is write tool but files_modified is empty",
                        tool_name
                    );
                } else {
                    tracing::debug!(
                        "[processor] ToolCall {} tracking {} file(s): {:?}",
                        tool_name,
                        event.files_modified.len(),
                        event.files_modified
                    );
                }
                for path in &event.files_modified {
                    session_state.file_tracker.record_change(path.clone());
                }
            }
        }
        _ => {}
    }
    tracing::debug!(
        "[processor] File tracker now has {} file(s)",
        session_state.file_tracker.get_files().len()
    );
}

/// Check if a tool is a write tool
fn is_write_tool(tool_name: &str) -> bool {
    matches!(
        tool_name.to_lowercase().as_str(),
        "write" | "write_file" | "edit" | "edit_file" | "create_file" | "delete_file"
    )
}

/// Parse boundary reason from string
fn parse_boundary_reason(reason: &str) -> BoundaryReason {
    let lower = reason.to_lowercase();
    if lower.contains("completion") {
        BoundaryReason::CompletionSignal
    } else if lower.contains("approv") {
        BoundaryReason::UserApproval
    } else if lower.contains("session") || lower.contains("end") {
        BoundaryReason::SessionEnd
    } else if lower.contains("pause") {
        BoundaryReason::ActivityPause
    } else {
        BoundaryReason::CompletionSignal
    }
}

/// Generate a staged patch from tracked file changes
async fn generate_patch(
    config: &ProcessorConfig,
    session_id: &str,
    session_state: &mut SessionProcessorState,
    reason: BoundaryReason,
) -> Result<()> {
    tracing::info!(
        "[processor] generate_patch called for session {} with reason {:?}",
        session_id,
        reason
    );

    let session = Session::load(&config.sessions_dir, session_id)
        .await
        .context("Failed to load session")?;

    let git_root = session
        .meta()
        .git_root
        .clone()
        .unwrap_or_else(|| session.meta().cwd.clone());

    tracing::debug!(
        "[processor] Using git_root: {:?} for patch generation",
        git_root
    );

    let manager = PatchManager::new(session.dir().to_path_buf());

    let files = session_state.file_tracker.get_files();
    if files.is_empty() {
        tracing::debug!("[processor] No files in file_tracker, skipping patch creation");
        return Ok(());
    }

    tracing::info!(
        "[processor] Creating patch with {} file(s): {:?}",
        files.len(),
        files
    );

    // Generate commit message using synthesis
    let message = generate_commit_message(config, &session, &files, &git_root).await;

    tracing::debug!("[processor] Generated commit message: {}", message);

    // Create patch
    let patch = manager
        .create_patch_from_changes(&git_root, &files, &message, reason)
        .await?;

    tracing::info!(
        "[processor] Patch {} created successfully for session {}",
        patch.meta.id,
        session_id
    );

    // Emit patch created event
    config.emit_event(SidecarEvent::PatchCreated {
        session_id: session_id.to_string(),
        patch_id: patch.meta.id,
        subject: patch.subject.clone(),
    });

    // Clear tracked changes
    session_state.file_tracker.clear();
    session_state.boundary_detector.clear();

    Ok(())
}

/// Generate a commit message using the configured synthesis backend
async fn generate_commit_message(
    config: &ProcessorConfig,
    session: &Session,
    files: &[PathBuf],
    git_root: &PathBuf,
) -> String {
    // If synthesis is disabled or set to template, use fast template-based generation
    if !config.synthesis.enabled || config.synthesis.backend == SynthesisBackend::Template {
        // Get diff for template-based analysis
        let diff = get_diff_for_files(git_root, files)
            .await
            .unwrap_or_default();
        return generate_template_message(files, &diff);
    }

    // Try LLM synthesis
    match generate_llm_commit_message(config, session, files, git_root).await {
        Ok(message) => message,
        Err(e) => {
            tracing::warn!("LLM synthesis failed, falling back to template: {}", e);
            // Fallback to template on error
            let diff = get_diff_for_files(git_root, files)
                .await
                .unwrap_or_default();
            generate_template_message(files, &diff)
        }
    }
}

/// Generate commit message using LLM synthesis
async fn generate_llm_commit_message(
    config: &ProcessorConfig,
    session: &Session,
    files: &[PathBuf],
    git_root: &PathBuf,
) -> Result<String> {
    use super::synthesis::create_synthesizer;

    // Get diff
    let diff = get_diff_for_files(git_root, files).await?;

    // Get session context
    let session_context = session.read_state().await.ok();

    // Create synthesizer
    let synthesizer = create_synthesizer(&config.synthesis)?;

    // Build input
    let mut input = SynthesisInput::new(diff, files.to_vec());
    if let Some(ctx) = session_context {
        input = input.with_context(ctx);
    }

    // Generate message
    let result = synthesizer.synthesize(&input).await?;
    tracing::debug!("Generated commit message using {} backend", result.backend);

    Ok(result.message)
}

/// Get git diff for specific files
async fn get_diff_for_files(git_root: &PathBuf, files: &[PathBuf]) -> Result<String> {
    use tokio::process::Command;

    let mut cmd = Command::new("git");
    cmd.arg("diff").arg("HEAD").arg("--").current_dir(git_root);

    for file in files {
        cmd.arg(file);
    }

    let output = cmd.output().await.context("Failed to run git diff")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Handle session end
async fn handle_end_session(config: &ProcessorConfig, session_id: &str) -> Result<()> {
    let mut session = Session::load(&config.sessions_dir, session_id).await?;
    session.complete().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_processor_lifecycle() {
        let temp = TempDir::new().unwrap();
        let config = ProcessorConfig {
            sessions_dir: temp.path().to_path_buf(),
            generate_patches: true,
            synthesis: SynthesisConfig::default(),
            #[cfg(feature = "tauri")]
            app_handle: None,
        };

        let processor = Processor::spawn(config);
        processor.shutdown().await;
    }

    #[test]
    fn test_file_change_tracker_records_unique_paths() {
        let mut tracker = FileChangeTracker::new();

        tracker.record_change(PathBuf::from("src/main.rs"));
        tracker.record_change(PathBuf::from("src/lib.rs"));
        tracker.record_change(PathBuf::from("src/main.rs")); // duplicate

        assert_eq!(tracker.get_files().len(), 2);
        assert!(tracker.get_files().contains(&PathBuf::from("src/main.rs")));
        assert!(tracker.get_files().contains(&PathBuf::from("src/lib.rs")));
    }

    #[test]
    fn test_file_change_tracker_clear() {
        let mut tracker = FileChangeTracker::new();

        tracker.record_change(PathBuf::from("src/main.rs"));
        assert!(!tracker.is_empty());

        tracker.clear();
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_is_write_tool() {
        assert!(is_write_tool("write"));
        assert!(is_write_tool("Write"));
        assert!(is_write_tool("WRITE_FILE"));
        assert!(is_write_tool("edit"));
        assert!(is_write_tool("Edit_File"));
        assert!(is_write_tool("create_file"));
        assert!(is_write_tool("delete_file"));
        assert!(!is_write_tool("read_file"));
        assert!(!is_write_tool("grep"));
    }

    #[test]
    fn test_parse_boundary_reason() {
        assert!(matches!(
            parse_boundary_reason("Completion signal detected"),
            BoundaryReason::CompletionSignal
        ));
        assert!(matches!(
            parse_boundary_reason("User approved changes"),
            BoundaryReason::UserApproval
        ));
        assert!(matches!(
            parse_boundary_reason("Session ended"),
            BoundaryReason::SessionEnd
        ));
        assert!(matches!(
            parse_boundary_reason("Pause in activity detected"),
            BoundaryReason::ActivityPause
        ));
    }

    #[test]
    fn test_track_file_changes_from_file_edit_event() {
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut state = SessionProcessorState::new();

        let event = SessionEvent::file_edit(
            session_id,
            PathBuf::from("src/main.rs"),
            super::super::events::FileOperation::Modify,
            Some("Update main function".to_string()),
        );

        track_file_changes(&event, &mut state);

        assert_eq!(state.file_tracker.get_files().len(), 1);
        assert!(state
            .file_tracker
            .get_files()
            .contains(&PathBuf::from("src/main.rs")));
    }

    #[test]
    fn test_track_file_changes_from_tool_call_event() {
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut state = SessionProcessorState::new();

        // Create a tool call event with files_modified
        let mut event = SessionEvent::tool_call_with_output(
            session_id,
            "write_file".to_string(),
            Some("path=src/lib.rs".to_string()),
            None,
            true,
            None,
            None,
        );
        event.files_modified = vec![PathBuf::from("src/lib.rs")];

        track_file_changes(&event, &mut state);

        assert_eq!(state.file_tracker.get_files().len(), 1);
        assert!(state
            .file_tracker
            .get_files()
            .contains(&PathBuf::from("src/lib.rs")));
    }

    #[test]
    fn test_commit_boundary_integration_with_processor_state() {
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut state = SessionProcessorState::new();

        // Add multiple file edits
        for i in 0..3 {
            let event = SessionEvent::file_edit(
                session_id.clone(),
                PathBuf::from(format!("src/file{}.rs", i)),
                super::super::events::FileOperation::Modify,
                None,
            );
            track_file_changes(&event, &mut state);

            // Also feed to boundary detector
            let _boundary = state.boundary_detector.check_boundary(&event);
        }

        // Now add reasoning with completion signal
        let reasoning_event =
            SessionEvent::reasoning(session_id.clone(), "Implementation is complete.", None);

        // Check for boundary
        let boundary = state.boundary_detector.check_boundary(&reasoning_event);

        // Should trigger a boundary since we have >= min_events (3) and completion signal
        assert!(
            boundary.is_some(),
            "Expected commit boundary to be detected after completion signal"
        );

        let boundary_info = boundary.unwrap();
        assert_eq!(
            boundary_info.files_in_scope.len(),
            3,
            "Boundary should include all 3 modified files"
        );
    }

    #[test]
    fn test_no_boundary_without_enough_files() {
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut state = SessionProcessorState::new();

        // Add only 2 file edits (less than min_events = 3)
        for i in 0..2 {
            let event = SessionEvent::file_edit(
                session_id.clone(),
                PathBuf::from(format!("src/file{}.rs", i)),
                super::super::events::FileOperation::Modify,
                None,
            );
            let _ = state.boundary_detector.check_boundary(&event);
        }

        // Add reasoning with completion signal
        let reasoning_event =
            SessionEvent::reasoning(session_id.clone(), "Implementation is complete.", None);

        // Check for boundary
        let boundary = state.boundary_detector.check_boundary(&reasoning_event);

        // Should NOT trigger a boundary since we have < min_events
        assert!(
            boundary.is_none(),
            "Should not detect boundary with fewer than min_events"
        );
    }

}

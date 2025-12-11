//! Sidecar state management for simplified session tracking.
//!
//! This is the main entry point for the sidecar system. It manages:
//! - Session lifecycle (create, end, get current)
//! - Event capture and forwarding to the processor
//! - Status reporting

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[cfg(feature = "tauri")]
use std::sync::Arc;
use std::sync::RwLock;

#[cfg(feature = "tauri")]
use tauri::AppHandle;

use super::config::SidecarConfig;
use super::events::{SessionEvent, SidecarEvent};
use super::processor::{Processor, ProcessorConfig};
use super::session::{ensure_sessions_dir, Session, SessionMeta};

/// Status of the sidecar system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarStatus {
    /// Whether a session is currently active
    pub active_session: bool,
    /// Current session ID if any
    pub session_id: Option<String>,
    /// Whether the sidecar is enabled
    pub enabled: bool,
    /// Sessions directory path
    pub sessions_dir: PathBuf,
    /// Workspace path (cwd of current session)
    pub workspace_path: Option<PathBuf>,
}

/// Internal state for active session tracking
#[derive(Default)]
struct InternalState {
    /// Current session ID
    current_session_id: Option<String>,
    /// Current workspace path
    workspace_path: Option<PathBuf>,
    /// Whether initialized
    initialized: bool,
}

/// Main sidecar state manager
pub struct SidecarState {
    /// Configuration
    config: RwLock<SidecarConfig>,
    /// Internal state
    state: RwLock<InternalState>,
    /// Event processor
    processor: RwLock<Option<Processor>>,
    /// Tauri app handle for emitting events
    #[cfg(feature = "tauri")]
    app_handle: RwLock<Option<AppHandle>>,
}

impl SidecarState {
    /// Create a new SidecarState with default configuration
    pub fn new() -> Self {
        Self {
            config: RwLock::new(SidecarConfig::default()),
            state: RwLock::new(InternalState::default()),
            processor: RwLock::new(None),
            #[cfg(feature = "tauri")]
            app_handle: RwLock::new(None),
        }
    }

    /// Create a new SidecarState with custom configuration
    pub fn with_config(config: SidecarConfig) -> Self {
        Self {
            config: RwLock::new(config),
            state: RwLock::new(InternalState::default()),
            processor: RwLock::new(None),
            #[cfg(feature = "tauri")]
            app_handle: RwLock::new(None),
        }
    }

    /// Set the Tauri app handle for event emission
    #[cfg(feature = "tauri")]
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.write().unwrap() = Some(handle);
    }

    /// Emit a sidecar event to the frontend
    #[cfg(feature = "tauri")]
    pub fn emit_event(&self, event: SidecarEvent) {
        use tauri::Emitter;
        if let Some(handle) = self.app_handle.read().unwrap().as_ref() {
            if let Err(e) = handle.emit("sidecar-event", &event) {
                tracing::warn!("Failed to emit sidecar event: {}", e);
            }
        }
    }

    /// No-op emit_event for non-tauri builds
    #[cfg(not(feature = "tauri"))]
    pub fn emit_event(&self, _event: SidecarEvent) {
        // No-op when not using tauri
    }

    /// Initialize the sidecar system
    pub async fn initialize(&self, workspace: PathBuf) -> Result<()> {
        let config = self.config.read().unwrap().clone();

        if !config.enabled {
            tracing::debug!("Sidecar is disabled, skipping initialization");
            return Ok(());
        }

        // Ensure sessions directory exists
        let sessions_dir = config.sessions_dir();
        ensure_sessions_dir(&sessions_dir).await?;

        // Create processor with synthesis config from sidecar config
        let synthesis_config = super::synthesis::SynthesisConfig {
            enabled: config.synthesis_enabled,
            backend: config.synthesis_backend,
            ..Default::default()
        };

        // Get app handle for processor to emit events
        #[cfg(feature = "tauri")]
        let app_handle_arc = self
            .app_handle
            .read()
            .unwrap()
            .as_ref()
            .map(|h| Arc::new(h.clone()));

        let processor_config = ProcessorConfig {
            sessions_dir: sessions_dir.clone(),
            generate_patches: true,
            synthesis: synthesis_config,
            #[cfg(feature = "tauri")]
            app_handle: app_handle_arc,
        };
        let processor = Processor::spawn(processor_config);

        // Update state
        {
            let mut state = self.state.write().unwrap();
            state.workspace_path = Some(workspace);
            state.initialized = true;
        }
        {
            *self.processor.write().unwrap() = Some(processor);
        }

        tracing::info!("Sidecar initialized with sessions dir: {:?}", sessions_dir);
        Ok(())
    }

    /// Get current status
    pub fn status(&self) -> SidecarStatus {
        let config = self.config.read().unwrap();
        let state = self.state.read().unwrap();

        SidecarStatus {
            active_session: state.current_session_id.is_some(),
            session_id: state.current_session_id.clone(),
            enabled: config.enabled,
            sessions_dir: config.sessions_dir(),
            workspace_path: state.workspace_path.clone(),
        }
    }

    /// Start a new session
    pub fn start_session(&self, initial_request: &str) -> Result<String> {
        let config = self.config.read().unwrap();
        if !config.enabled {
            anyhow::bail!("Sidecar is disabled");
        }

        let state = self.state.read().unwrap();
        if !state.initialized {
            anyhow::bail!("Sidecar not initialized");
        }

        // Check if session already exists
        if state.current_session_id.is_some() {
            return Ok(state.current_session_id.clone().unwrap());
        }

        let cwd = state
            .workspace_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        drop(state);

        // Generate session ID
        let session_id = uuid::Uuid::new_v4().to_string();

        // Create session directory and files synchronously via blocking task
        let sessions_dir = config.sessions_dir();
        let sid = session_id.clone();
        let req = initial_request.to_string();
        let cwd_clone = cwd.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                if let Err(e) = Session::create(&sessions_dir, sid, cwd_clone, req).await {
                    tracing::error!("Failed to create session: {}", e);
                }
            });
        });

        // Update state
        {
            let mut state = self.state.write().unwrap();
            state.current_session_id = Some(session_id.clone());
        }

        // Emit session started event
        self.emit_event(SidecarEvent::SessionStarted {
            session_id: session_id.clone(),
        });

        tracing::info!("Started new session: {}", session_id);
        Ok(session_id)
    }

    /// End the current session
    pub fn end_session(&self) -> Result<Option<SessionMeta>> {
        let session_id = {
            let mut state = self.state.write().unwrap();
            state.current_session_id.take()
        };

        let Some(session_id) = session_id else {
            return Ok(None);
        };

        // Emit session ended event
        self.emit_event(SidecarEvent::SessionEnded {
            session_id: session_id.clone(),
        });

        // Signal processor to end session
        if let Some(processor) = self.processor.read().unwrap().as_ref() {
            processor.end_session(session_id.clone());
        }

        // Load session metadata
        let config = self.config.read().unwrap();
        let sessions_dir = config.sessions_dir();

        let meta = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                match Session::load(&sessions_dir, &session_id).await {
                    Ok(session) => Some(session.meta().clone()),
                    Err(e) => {
                        tracing::error!("Failed to load session metadata: {}", e);
                        None
                    }
                }
            })
        })
        .join()
        .unwrap_or(None);

        tracing::info!("Ended session: {:?}", meta.as_ref().map(|m| &m.session_id));
        Ok(meta)
    }

    /// Get current session ID
    pub fn current_session_id(&self) -> Option<String> {
        self.state.read().unwrap().current_session_id.clone()
    }

    /// Capture an event
    pub fn capture(&self, event: SessionEvent) {
        let config = self.config.read().unwrap();
        if !config.enabled {
            return;
        }

        // Filter based on config
        if !config.capture_tool_calls
            && matches!(event.event_type, super::events::EventType::ToolCall { .. })
        {
            return;
        }
        if !config.capture_reasoning
            && matches!(
                event.event_type,
                super::events::EventType::AgentReasoning { .. }
            )
        {
            return;
        }

        // Forward to processor
        if let Some(processor) = self.processor.read().unwrap().as_ref() {
            processor.process_event(event.session_id.clone(), event);
        }
    }

    /// Get current configuration
    pub fn config(&self) -> SidecarConfig {
        self.config.read().unwrap().clone()
    }

    /// Update configuration
    pub fn set_config(&self, config: SidecarConfig) {
        *self.config.write().unwrap() = config;
    }

    /// Get injectable context (state.md body) for current session
    pub async fn get_injectable_context(&self) -> Result<Option<String>> {
        let session_id = match self.current_session_id() {
            Some(id) => id,
            None => return Ok(None),
        };

        let sessions_dir = self.config.read().unwrap().sessions_dir();
        let session = Session::load(&sessions_dir, &session_id).await?;
        let state = session.read_state().await?;
        Ok(Some(state))
    }

    /// Get session state.md content (body only)
    pub async fn get_session_state(&self, session_id: &str) -> Result<String> {
        let sessions_dir = self.config.read().unwrap().sessions_dir();
        let session = Session::load(&sessions_dir, session_id).await?;
        session.read_state().await
    }

    /// Get session metadata
    pub async fn get_session_meta(&self, session_id: &str) -> Result<SessionMeta> {
        let sessions_dir = self.config.read().unwrap().sessions_dir();
        let session = Session::load(&sessions_dir, session_id).await?;
        Ok(session.meta().clone())
    }

    /// List all sessions
    pub async fn list_sessions(&self) -> Result<Vec<SessionMeta>> {
        let sessions_dir = self.config.read().unwrap().sessions_dir();
        super::session::list_sessions(&sessions_dir).await
    }

    /// Shutdown the sidecar
    pub fn shutdown(&self) {
        let _ = self.end_session();

        if let Some(processor) = self.processor.write().unwrap().take() {
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(processor.shutdown());
            });
        }

        tracing::info!("Sidecar shutdown complete");
    }
}

impl Default for SidecarState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(temp_dir: &std::path::Path) -> SidecarConfig {
        SidecarConfig {
            enabled: true,
            sessions_dir: Some(temp_dir.to_path_buf()),
            retention_days: 0,
            max_state_size: 16 * 1024,
            write_raw_events: false,
            use_llm_for_state: false,
            capture_tool_calls: true,
            capture_reasoning: true,
            synthesis_enabled: true,
            synthesis_backend: crate::sidecar::synthesis::SynthesisBackend::Template,
            artifact_synthesis_backend:
                crate::sidecar::artifacts::ArtifactSynthesisBackend::Template,
        }
    }

    #[tokio::test]
    async fn test_sidecar_state_creation() {
        let state = SidecarState::new();
        let status = state.status();
        assert!(!status.active_session);
        assert!(status.session_id.is_none());
    }

    #[tokio::test]
    async fn test_sidecar_initialization() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let state = SidecarState::with_config(config);

        state.initialize(temp.path().to_path_buf()).await.unwrap();

        let status = state.status();
        assert!(status.enabled);
        assert!(status.workspace_path.is_some());
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let state = SidecarState::with_config(config);

        state.initialize(temp.path().to_path_buf()).await.unwrap();

        let session_id = state.start_session("Test request").unwrap();
        assert!(!session_id.is_empty());
        assert!(state.current_session_id().is_some());

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let _meta = state.end_session().unwrap();
        assert!(state.current_session_id().is_none());
    }

    #[tokio::test]
    async fn test_status() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let state = SidecarState::with_config(config);

        let status = state.status();
        assert!(status.enabled);
        assert!(!status.active_session);
    }
}

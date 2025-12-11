//! L2: Staged Commits using Git Format-Patch
//!
//! Stores commits as standard git patch files that can be applied with `git am`.
//!
//! ## File Format
//!
//! Each patch is a standard git format-patch file:
//!
//! ```patch
//! From 0000000000000000000000000000000000000000 Mon Sep 17 00:00:00 2001
//! From: Qbit Agent <agent@qbit.dev>
//! Date: Tue, 10 Dec 2025 14:30:00 +0000
//! Subject: [PATCH] feat(auth): add JWT authentication module
//!
//! Implements token generation and validation with configurable expiry.
//! ---
//!  src/auth.rs | 25 +++++++++++++++++++++++++
//!  src/lib.rs  |  1 +
//!  2 files changed, 26 insertions(+)
//!  create mode 100644 src/auth.rs
//!
//! diff --git a/src/auth.rs b/src/auth.rs
//! ...
//! --
//! 2.39.0
//! ```
//!
//! We also store a small metadata sidecar file for qbit-specific info.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;

/// Metadata for a staged patch (stored alongside the .patch file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchMeta {
    /// Unique patch ID (sequence number)
    pub id: u32,
    /// When this patch was created
    pub created_at: DateTime<Utc>,
    /// Why this boundary was detected
    pub boundary_reason: BoundaryReason,
    /// Git SHA after applying (only set after applied)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_sha: Option<String>,
}

/// Reason for commit boundary detection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryReason {
    /// Agent signaled completion in reasoning
    CompletionSignal,
    /// User approved changes
    UserApproval,
    /// Session ended
    SessionEnd,
    /// Pause in activity
    ActivityPause,
    /// User explicitly requested commit
    UserRequest,
}

impl std::fmt::Display for BoundaryReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundaryReason::CompletionSignal => write!(f, "completion_signal"),
            BoundaryReason::UserApproval => write!(f, "user_approval"),
            BoundaryReason::SessionEnd => write!(f, "session_end"),
            BoundaryReason::ActivityPause => write!(f, "activity_pause"),
            BoundaryReason::UserRequest => write!(f, "user_request"),
        }
    }
}

/// A staged patch with its metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedPatch {
    /// Patch metadata
    pub meta: PatchMeta,
    /// Subject line (first line of commit message)
    pub subject: String,
    /// Full commit message body
    pub message: String,
    /// Files changed (parsed from diffstat)
    pub files: Vec<String>,
    /// The raw patch content (used internally, skipped in serialization)
    #[serde(skip)]
    #[allow(dead_code)]
    pub patch_content: String,
}

impl StagedPatch {
    /// Generate filename for this patch (e.g., "0001-feat-auth-add-jwt.patch")
    pub fn filename(&self) -> String {
        let slug = slugify(&self.subject);
        format!("{:04}-{}.patch", self.meta.id, slug)
    }

    /// Generate metadata filename
    pub fn meta_filename(&self) -> String {
        format!("{:04}.meta.toml", self.meta.id)
    }

    /// Parse subject from patch content
    pub fn parse_subject(patch_content: &str) -> Option<String> {
        for line in patch_content.lines() {
            if let Some(subject) = line.strip_prefix("Subject: ") {
                // Remove [PATCH] prefix if present
                let subject = subject
                    .strip_prefix("[PATCH] ")
                    .or_else(|| subject.strip_prefix("[PATCH 1/1] "))
                    .unwrap_or(subject);
                return Some(subject.to_string());
            }
        }
        None
    }

    /// Parse files changed from patch content (diffstat section)
    pub fn parse_files(patch_content: &str) -> Vec<String> {
        let mut files = Vec::new();
        let mut in_diffstat = false;

        for line in patch_content.lines() {
            // Diffstat starts after "---" line
            if line == "---" {
                in_diffstat = true;
                continue;
            }

            // Diffstat ends at empty line or diff start
            if in_diffstat {
                if line.is_empty() || line.starts_with("diff --git") {
                    break;
                }
                // Parse diffstat line: " src/auth.rs | 25 ++++"
                if let Some(file) = line.split('|').next() {
                    let file = file.trim();
                    if !file.is_empty() && !file.contains("changed") {
                        files.push(file.to_string());
                    }
                }
            }
        }

        files
    }
}

/// Manages patches for a session
pub struct PatchManager {
    /// Session directory
    session_dir: PathBuf,
}

impl PatchManager {
    /// Subdirectory names
    const PATCHES_DIR: &'static str = "patches";
    const STAGED_DIR: &'static str = "staged";
    const APPLIED_DIR: &'static str = "applied";

    /// Create a new patch manager for a session
    pub fn new(session_dir: PathBuf) -> Self {
        Self { session_dir }
    }

    /// Get the path to staged patches directory
    fn staged_dir(&self) -> PathBuf {
        self.session_dir
            .join(Self::PATCHES_DIR)
            .join(Self::STAGED_DIR)
    }

    /// Get the path to applied patches directory
    fn applied_dir(&self) -> PathBuf {
        self.session_dir
            .join(Self::PATCHES_DIR)
            .join(Self::APPLIED_DIR)
    }

    /// Ensure patch directories exist
    pub async fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.staged_dir())
            .await
            .context("Failed to create staged patches directory")?;
        fs::create_dir_all(self.applied_dir())
            .await
            .context("Failed to create applied patches directory")?;
        Ok(())
    }

    /// Get the next patch ID
    pub async fn next_id(&self) -> Result<u32> {
        let staged = self.list_staged().await.unwrap_or_default();
        let applied = self.list_applied().await.unwrap_or_default();

        let max_staged = staged.iter().map(|p| p.meta.id).max().unwrap_or(0);
        let max_applied = applied.iter().map(|p| p.meta.id).max().unwrap_or(0);

        Ok(max_staged.max(max_applied) + 1)
    }

    /// Create a patch from staged git changes
    ///
    /// This stages files and generates a patch using git format-patch style.
    #[allow(dead_code)]
    pub async fn create_patch_from_staged(
        &self,
        git_root: &Path,
        message: &str,
        boundary_reason: BoundaryReason,
    ) -> Result<StagedPatch> {
        self.ensure_dirs().await?;

        let id = self.next_id().await?;

        // Generate patch content in git format-patch style
        let patch_content = generate_format_patch(git_root, message).await?;

        // Parse patch info
        let subject = StagedPatch::parse_subject(&patch_content)
            .unwrap_or_else(|| message.lines().next().unwrap_or("changes").to_string());
        let files = StagedPatch::parse_files(&patch_content);

        // Create metadata
        let meta = PatchMeta {
            id,
            created_at: Utc::now(),
            boundary_reason,
            applied_sha: None,
        };

        let patch = StagedPatch {
            meta: meta.clone(),
            subject: subject.clone(),
            message: message.to_string(),
            files,
            patch_content: patch_content.clone(),
        };

        // Write patch file
        let patch_path = self.staged_dir().join(patch.filename());
        fs::write(&patch_path, &patch_content)
            .await
            .context("Failed to write patch file")?;

        // Write metadata file
        let meta_path = self.staged_dir().join(patch.meta_filename());
        let meta_content = toml::to_string_pretty(&meta)?;
        fs::write(&meta_path, &meta_content)
            .await
            .context("Failed to write patch metadata")?;

        tracing::info!("Created staged patch: {}", patch.filename());
        Ok(patch)
    }

    /// Create a patch from file changes (without git staging)
    ///
    /// Used when we have tracked file changes but haven't staged them in git yet.
    pub async fn create_patch_from_changes(
        &self,
        git_root: &Path,
        files: &[PathBuf],
        message: &str,
        boundary_reason: BoundaryReason,
    ) -> Result<StagedPatch> {
        self.ensure_dirs().await?;

        let id = self.next_id().await?;

        // Generate diff for specific files
        let diff_content = generate_diff_for_files(git_root, files).await?;

        // Create format-patch style content
        let patch_content = format_patch_content(message, &diff_content);

        let subject = message.lines().next().unwrap_or("changes").to_string();
        let file_strings: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();

        // Create metadata
        let meta = PatchMeta {
            id,
            created_at: Utc::now(),
            boundary_reason,
            applied_sha: None,
        };

        let patch = StagedPatch {
            meta: meta.clone(),
            subject,
            message: message.to_string(),
            files: file_strings,
            patch_content: patch_content.clone(),
        };

        // Write patch file
        let patch_path = self.staged_dir().join(patch.filename());
        fs::write(&patch_path, &patch_content)
            .await
            .context("Failed to write patch file")?;

        // Write metadata file
        let meta_path = self.staged_dir().join(patch.meta_filename());
        let meta_content = toml::to_string_pretty(&meta)?;
        fs::write(&meta_path, &meta_content)
            .await
            .context("Failed to write patch metadata")?;

        tracing::info!("Created staged patch: {}", patch.filename());
        Ok(patch)
    }

    /// List all staged patches
    pub async fn list_staged(&self) -> Result<Vec<StagedPatch>> {
        self.list_patches_in_dir(&self.staged_dir()).await
    }

    /// List all applied patches
    pub async fn list_applied(&self) -> Result<Vec<StagedPatch>> {
        self.list_patches_in_dir(&self.applied_dir()).await
    }

    /// List patches in a directory
    async fn list_patches_in_dir(&self, dir: &Path) -> Result<Vec<StagedPatch>> {
        let mut patches = Vec::new();

        if !dir.exists() {
            return Ok(patches);
        }

        let mut entries = fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "patch") {
                match self.load_patch(&path).await {
                    Ok(patch) => patches.push(patch),
                    Err(e) => {
                        tracing::warn!("Failed to load patch {:?}: {}", path, e);
                    }
                }
            }
        }

        // Sort by ID
        patches.sort_by_key(|p| p.meta.id);
        Ok(patches)
    }

    /// Load a patch from file
    async fn load_patch(&self, patch_path: &Path) -> Result<StagedPatch> {
        let patch_content = fs::read_to_string(patch_path)
            .await
            .context("Failed to read patch file")?;

        // Load metadata from sidecar file
        let meta_path = patch_path.with_extension("meta.toml");
        let meta_path = if meta_path.exists() {
            meta_path
        } else {
            // Try alternate naming: 0001.meta.toml for 0001-*.patch
            let stem = patch_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let id_part = stem.split('-').next().unwrap_or("0000");
            patch_path
                .parent()
                .unwrap_or(Path::new("."))
                .join(format!("{}.meta.toml", id_part))
        };

        let meta: PatchMeta = if meta_path.exists() {
            let meta_content = fs::read_to_string(&meta_path).await?;
            toml::from_str(&meta_content)?
        } else {
            // Create default metadata if missing
            let id = patch_path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.split('-').next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            PatchMeta {
                id,
                created_at: Utc::now(),
                boundary_reason: BoundaryReason::UserRequest,
                applied_sha: None,
            }
        };

        let subject =
            StagedPatch::parse_subject(&patch_content).unwrap_or_else(|| "unknown".to_string());
        let files = StagedPatch::parse_files(&patch_content);

        // Extract full message from patch
        let message = extract_message_from_patch(&patch_content);

        Ok(StagedPatch {
            meta,
            subject,
            message,
            files,
            patch_content,
        })
    }

    /// Get a specific staged patch by ID
    pub async fn get_staged(&self, id: u32) -> Result<Option<StagedPatch>> {
        let patches = self.list_staged().await?;
        Ok(patches.into_iter().find(|p| p.meta.id == id))
    }

    /// Discard a staged patch
    pub async fn discard_patch(&self, id: u32) -> Result<bool> {
        let patches = self.list_staged().await?;
        if let Some(patch) = patches.into_iter().find(|p| p.meta.id == id) {
            let patch_path = self.staged_dir().join(patch.filename());
            let meta_path = self.staged_dir().join(patch.meta_filename());

            fs::remove_file(&patch_path).await.ok();
            fs::remove_file(&meta_path).await.ok();

            tracing::info!("Discarded patch: {}", patch.filename());
            return Ok(true);
        }
        Ok(false)
    }

    /// Update the commit message for a staged patch
    ///
    /// This rewrites the patch file with the new message while preserving the diff.
    pub async fn update_patch_message(&self, id: u32, new_message: &str) -> Result<StagedPatch> {
        let patch = self
            .get_staged(id)
            .await?
            .context(format!("Patch {} not found in staged", id))?;

        let old_patch_path = self.staged_dir().join(patch.filename());

        // Read the old patch to extract the diff
        let old_content = fs::read_to_string(&old_patch_path)
            .await
            .context("Failed to read patch file")?;

        // Extract the diff portion (everything after the "---" separator line until "--" footer)
        let diff = extract_diff_from_patch(&old_content);

        // Create new patch content with updated message
        let new_patch_content = format_patch_content(new_message, &diff);

        // Update patch data
        let new_subject = new_message.lines().next().unwrap_or("changes").to_string();
        let updated_patch = StagedPatch {
            meta: patch.meta.clone(),
            subject: new_subject.clone(),
            message: new_message.to_string(),
            files: patch.files.clone(),
            patch_content: new_patch_content.clone(),
        };

        // Calculate new filename (might change if subject changed)
        let new_patch_path = self.staged_dir().join(updated_patch.filename());

        // Write new patch file
        fs::write(&new_patch_path, &new_patch_content)
            .await
            .context("Failed to write updated patch file")?;

        // Remove old patch file if filename changed
        if old_patch_path != new_patch_path && old_patch_path.exists() {
            fs::remove_file(&old_patch_path).await.ok();
        }

        tracing::info!("Updated patch {} message: {}", id, new_subject);
        Ok(updated_patch)
    }

    /// Get the raw diff content from a staged patch
    pub async fn get_patch_diff(&self, id: u32) -> Result<String> {
        let patch = self
            .get_staged(id)
            .await?
            .context(format!("Patch {} not found in staged", id))?;

        let patch_path = self.staged_dir().join(patch.filename());
        let content = fs::read_to_string(&patch_path)
            .await
            .context("Failed to read patch file")?;

        Ok(extract_diff_from_patch(&content))
    }

    /// Apply a staged patch using git am
    pub async fn apply_patch(&self, id: u32, git_root: &Path) -> Result<String> {
        let patch = self
            .get_staged(id)
            .await?
            .context(format!("Patch {} not found in staged", id))?;

        let patch_path = self.staged_dir().join(patch.filename());

        // Apply using git am
        let output = Command::new("git")
            .args(["am", "--3way"])
            .arg(&patch_path)
            .current_dir(git_root)
            .output()
            .await
            .context("Failed to run git am")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Abort the failed am
            let _ = Command::new("git")
                .args(["am", "--abort"])
                .current_dir(git_root)
                .output()
                .await;
            bail!("git am failed: {}", stderr);
        }

        // Get the commit SHA
        let sha_output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(git_root)
            .output()
            .await
            .context("Failed to get commit SHA")?;

        let sha = String::from_utf8_lossy(&sha_output.stdout)
            .trim()
            .to_string();

        // Move patch to applied
        self.mark_applied(id, &sha).await?;

        tracing::info!("Applied patch {} with SHA {}", id, sha);
        Ok(sha)
    }

    /// Move a patch from staged to applied
    async fn mark_applied(&self, id: u32, sha: &str) -> Result<()> {
        let patches = self.list_staged().await?;
        if let Some(mut patch) = patches.into_iter().find(|p| p.meta.id == id) {
            patch.meta.applied_sha = Some(sha.to_string());

            // Move patch file
            let staged_patch = self.staged_dir().join(patch.filename());
            let applied_patch = self.applied_dir().join(patch.filename());
            fs::rename(&staged_patch, &applied_patch).await?;

            // Update and move metadata
            let staged_meta = self.staged_dir().join(patch.meta_filename());
            let applied_meta = self.applied_dir().join(patch.meta_filename());
            let meta_content = toml::to_string_pretty(&patch.meta)?;
            fs::write(&applied_meta, &meta_content).await?;
            fs::remove_file(&staged_meta).await.ok();
        }
        Ok(())
    }

    /// Apply all staged patches in order
    pub async fn apply_all_patches(&self, git_root: &Path) -> Result<Vec<(u32, String)>> {
        let staged = self.list_staged().await?;
        let mut results = Vec::new();

        for patch in staged {
            match self.apply_patch(patch.meta.id, git_root).await {
                Ok(sha) => {
                    results.push((patch.meta.id, sha));
                }
                Err(e) => {
                    bail!(
                        "Failed to apply patch {}: {}. Applied {} patches before failure.",
                        patch.meta.id,
                        e,
                        results.len()
                    );
                }
            }
        }

        Ok(results)
    }
}

// =============================================================================
// Git Helpers
// =============================================================================

/// Generate a format-patch style patch from staged changes
#[allow(dead_code)]
async fn generate_format_patch(git_root: &Path, message: &str) -> Result<String> {
    // Get the diff of staged changes
    let diff_output = Command::new("git")
        .args(["diff", "--cached"])
        .current_dir(git_root)
        .output()
        .await
        .context("Failed to run git diff")?;

    let diff = String::from_utf8_lossy(&diff_output.stdout).to_string();

    if diff.trim().is_empty() {
        bail!("No staged changes to create patch from");
    }

    Ok(format_patch_content(message, &diff))
}

/// Generate diff for specific files (comparing to HEAD)
async fn generate_diff_for_files(git_root: &Path, files: &[PathBuf]) -> Result<String> {
    let mut args = vec!["diff", "HEAD", "--"];
    for file in files {
        if let Some(s) = file.to_str() {
            args.push(s);
        }
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(git_root)
        .output()
        .await
        .context("Failed to run git diff")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Format diff content as a git format-patch style patch
fn format_patch_content(message: &str, diff: &str) -> String {
    let now = Utc::now();
    let date = now.format("%a, %d %b %Y %H:%M:%S %z").to_string();

    // Split message into subject and body
    let mut lines = message.lines();
    let subject = lines.next().unwrap_or("changes");
    let body: String = lines.collect::<Vec<_>>().join("\n");

    // Count files changed (rough estimate from diff)
    let files_changed = diff.matches("diff --git").count();

    let mut patch = String::new();

    // Header
    patch.push_str("From 0000000000000000000000000000000000000000 Mon Sep 17 00:00:00 2001\n");
    patch.push_str("From: Qbit Agent <agent@qbit.dev>\n");
    patch.push_str(&format!("Date: {}\n", date));
    patch.push_str(&format!("Subject: [PATCH] {}\n", subject));
    patch.push('\n');

    // Body
    if !body.trim().is_empty() {
        patch.push_str(body.trim());
        patch.push('\n');
    }

    // Diffstat separator
    patch.push_str("---\n");

    // Simple diffstat
    patch.push_str(&format!(" {} file(s) changed\n", files_changed));
    patch.push('\n');

    // The actual diff
    patch.push_str(diff);

    // Footer
    patch.push_str("--\n");
    patch.push_str("2.39.0\n");

    patch
}

/// Extract commit message from patch content
fn extract_message_from_patch(patch_content: &str) -> String {
    let mut in_message = false;
    let mut message_lines = Vec::new();

    for line in patch_content.lines() {
        if line.starts_with("Subject: ") {
            let subject = line
                .strip_prefix("Subject: ")
                .unwrap_or("")
                .strip_prefix("[PATCH] ")
                .or_else(|| line.strip_prefix("Subject: [PATCH 1/1] "))
                .unwrap_or(line.strip_prefix("Subject: ").unwrap_or(""));
            message_lines.push(subject.to_string());
            in_message = true;
            continue;
        }

        if in_message {
            if line == "---" {
                break;
            }
            message_lines.push(line.to_string());
        }
    }

    message_lines.join("\n").trim().to_string()
}

/// Extract the diff portion from a patch file
///
/// The diff starts after the diffstat section (after "---" and file stats)
/// and ends before the "--" footer line.
fn extract_diff_from_patch(patch_content: &str) -> String {
    let mut in_diff = false;
    let mut diff_lines = Vec::new();
    let mut found_separator = false;

    for line in patch_content.lines() {
        // Look for the "---" separator after the message
        if line == "---" && !found_separator {
            found_separator = true;
            continue;
        }

        // After the separator, look for the diff start
        if found_separator && line.starts_with("diff --git") {
            in_diff = true;
        }

        // Stop at the footer
        if line == "--" {
            break;
        }

        if in_diff {
            diff_lines.push(line);
        }
    }

    diff_lines.join("\n")
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert a title to a URL-friendly slug
fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(50)
        .collect()
}

// =============================================================================
// LLM Prompts
// =============================================================================

/// System prompt for LLM-based commit message generation
#[allow(dead_code)]
pub const COMMIT_MESSAGE_PROMPT: &str = r#"You are generating a git commit message for the following changes.

## Guidelines
- Use conventional commit format: type(scope): description
- Types: feat, fix, refactor, docs, test, chore, perf, style, build, ci
- First line ≤ 72 characters
- Body explains what and why (not how)
- Be concise but complete

## Format
```
type(scope): short description

Optional body explaining what changed and why.
```

Return ONLY the commit message, no explanations or markdown formatting.
"#;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("feat(auth): add JWT"), "feat-auth-add-jwt");
        assert_eq!(slugify("Fix bug #123"), "fix-bug-123");
    }

    #[test]
    fn test_format_patch_content() {
        let message = "feat(auth): add authentication\n\nAdds JWT-based auth.";
        let diff = "diff --git a/src/auth.rs b/src/auth.rs\n+pub fn auth() {}";

        let patch = format_patch_content(message, diff);

        assert!(patch.contains("From: Qbit Agent"));
        assert!(patch.contains("Subject: [PATCH] feat(auth): add authentication"));
        assert!(patch.contains("Adds JWT-based auth."));
        assert!(patch.contains("diff --git"));
    }

    #[test]
    fn test_parse_subject() {
        let patch = "From: Test\nSubject: [PATCH] feat: add feature\n\nbody";
        assert_eq!(
            StagedPatch::parse_subject(patch),
            Some("feat: add feature".to_string())
        );
    }

    #[tokio::test]
    async fn test_patch_manager_lifecycle() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path().to_path_buf());

        manager.ensure_dirs().await.unwrap();
        assert!(temp.path().join("patches/staged").exists());
        assert!(temp.path().join("patches/applied").exists());

        // Test next_id
        let id = manager.next_id().await.unwrap();
        assert_eq!(id, 1);
    }
}

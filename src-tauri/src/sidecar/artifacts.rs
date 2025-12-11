//! L3: Project Artifacts
//!
//! Auto-maintained project documentation (README.md, CLAUDE.md) based on session activity.
//! Proposes updates that users review and apply.
//!
//! ## File Format
//!
//! Each artifact file includes a metadata header as an HTML comment:
//!
//! ```markdown
//! <!--
//! Target: /path/to/README.md
//! Created: 2025-12-10 14:30
//! Reason: Added authentication feature
//! Based on patches: 0001, 0002
//! -->
//!
//! # Project Title
//! ...content...
//! ```
//!
//! ## Directory Structure
//!
//! ```text
//! artifacts/
//!   pending/     # Proposed documentation updates awaiting review
//!   applied/     # Previously applied artifacts (archived)
//! ```
//!
//! ## LLM Synthesis (Phase 6)
//!
//! Artifact synthesis can use LLM backends to generate better documentation:
//! - `Template` - Rule-based generation (default, no API calls)
//! - `VertexAnthropic` - Anthropic Claude via Vertex AI
//! - `OpenAi` - OpenAI API (or compatible)
//! - `Grok` - Grok API (xAI)

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::settings::schema::{
    SynthesisGrokSettings, SynthesisOpenAiSettings, SynthesisVertexSettings,
};

// =============================================================================
// LLM Prompt Templates for Artifact Synthesis
// =============================================================================

/// System prompt for README.md generation
pub const README_SYSTEM_PROMPT: &str = r#"You are a technical writer updating a README.md file based on recent code changes.

## Your Task
Analyze the provided patches and update the README to accurately reflect the current state of the project.

## Guidelines
- Update relevant sections only; preserve existing structure and content that is still accurate
- Focus on user-facing changes: new features, changed APIs, updated usage instructions
- Be concise and clear; avoid unnecessary verbosity
- Maintain the existing writing style and tone
- Do not add boilerplate or placeholder sections
- If the changes are purely internal/refactoring with no user impact, make minimal or no changes

## Output Format
Return ONLY the updated README.md content, no explanations or markdown code blocks.
The output should be ready to save directly as README.md."#;

/// User prompt template for README.md generation
pub const README_USER_PROMPT: &str = r#"Update this README.md based on recent changes.

## Current README.md
```markdown
{existing_content}
```

## Recent Changes (patch summaries)
{patches_summary}

## Session Context
{session_context}

Generate the updated README.md content."#;

/// System prompt for CLAUDE.md generation
pub const CLAUDE_MD_SYSTEM_PROMPT: &str = r#"You are updating a CLAUDE.md file (AI assistant instructions) based on recent code changes.

## About CLAUDE.md
CLAUDE.md provides context and conventions for AI assistants working on this codebase. It typically includes:
- Project overview and architecture
- Commands and workflows
- Code conventions and patterns
- Important files and their purposes

## Your Task
Update CLAUDE.md to reflect new conventions, patterns, or architecture discovered in the patches.

## Guidelines
- Add new commands or workflows if introduced
- Update architecture sections if structure changed
- Add new conventions discovered from the code changes
- Preserve existing accurate content
- Keep instructions actionable and specific
- Do not remove existing content unless it's clearly outdated

## Output Format
Return ONLY the updated CLAUDE.md content, no explanations or markdown code blocks.
The output should be ready to save directly as CLAUDE.md."#;

/// User prompt template for CLAUDE.md generation
pub const CLAUDE_MD_USER_PROMPT: &str = r#"Update this CLAUDE.md based on recent changes.

## Current CLAUDE.md
```markdown
{existing_content}
```

## Recent Changes (patch summaries)
{patches_summary}

## Session Context
{session_context}

Generate the updated CLAUDE.md content."#;

// =============================================================================
// Artifact Synthesis Backend
// =============================================================================

/// Backend for artifact synthesis (similar to commit message synthesis)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSynthesisBackend {
    /// Rule-based template generation (no API calls)
    #[default]
    Template,
    /// Anthropic Claude via Vertex AI
    VertexAnthropic,
    /// OpenAI API (or compatible)
    OpenAi,
    /// Grok API (xAI)
    Grok,
}

impl std::str::FromStr for ArtifactSynthesisBackend {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "template" => Ok(ArtifactSynthesisBackend::Template),
            "vertex_anthropic" | "vertex" => Ok(ArtifactSynthesisBackend::VertexAnthropic),
            "openai" => Ok(ArtifactSynthesisBackend::OpenAi),
            "grok" => Ok(ArtifactSynthesisBackend::Grok),
            _ => bail!("Unknown artifact synthesis backend: {}", s),
        }
    }
}

impl std::fmt::Display for ArtifactSynthesisBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactSynthesisBackend::Template => write!(f, "template"),
            ArtifactSynthesisBackend::VertexAnthropic => write!(f, "vertex_anthropic"),
            ArtifactSynthesisBackend::OpenAi => write!(f, "openai"),
            ArtifactSynthesisBackend::Grok => write!(f, "grok"),
        }
    }
}

/// Configuration for artifact synthesis
#[derive(Debug, Clone)]
pub struct ArtifactSynthesisConfig {
    /// Which backend to use
    pub backend: ArtifactSynthesisBackend,
    /// Vertex AI settings (when backend = vertex_anthropic)
    pub vertex: SynthesisVertexSettings,
    /// OpenAI settings (when backend = openai)
    pub openai: SynthesisOpenAiSettings,
    /// Grok settings (when backend = grok)
    pub grok: SynthesisGrokSettings,
}

impl Default for ArtifactSynthesisConfig {
    fn default() -> Self {
        Self {
            backend: ArtifactSynthesisBackend::Template,
            vertex: SynthesisVertexSettings::default(),
            openai: SynthesisOpenAiSettings::default(),
            grok: SynthesisGrokSettings::default(),
        }
    }
}

impl ArtifactSynthesisConfig {
    /// Create config from sidecar settings (reuses synthesis settings)
    pub fn from_sidecar_settings(settings: &crate::settings::schema::SidecarSettings) -> Self {
        // Artifact synthesis reuses the same backend config as commit message synthesis
        let backend = settings
            .synthesis_backend
            .parse()
            .unwrap_or(ArtifactSynthesisBackend::Template);

        Self {
            backend,
            vertex: settings.synthesis_vertex.clone(),
            openai: settings.synthesis_openai.clone(),
            grok: settings.synthesis_grok.clone(),
        }
    }

    /// Check if using LLM backend (not template)
    pub fn uses_llm(&self) -> bool {
        self.backend != ArtifactSynthesisBackend::Template
    }
}

// =============================================================================
// LLM Artifact Synthesizers
// =============================================================================

/// Input for artifact synthesis
#[derive(Debug, Clone)]
pub struct ArtifactSynthesisInput {
    /// Existing content of the target file
    pub existing_content: String,
    /// Summary of patches (commit subjects)
    pub patches_summary: Vec<String>,
    /// Session context (goals, progress)
    pub session_context: String,
}

impl ArtifactSynthesisInput {
    /// Create new synthesis input
    pub fn new(
        existing_content: String,
        patches_summary: Vec<String>,
        session_context: String,
    ) -> Self {
        Self {
            existing_content,
            patches_summary,
            session_context,
        }
    }

    /// Build the user prompt for README.md
    pub fn build_readme_prompt(&self) -> String {
        let patches = if self.patches_summary.is_empty() {
            "No patches available.".to_string()
        } else {
            self.patches_summary
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {}", i + 1, s))
                .collect::<Vec<_>>()
                .join("\n")
        };

        README_USER_PROMPT
            .replace("{existing_content}", &self.existing_content)
            .replace("{patches_summary}", &patches)
            .replace("{session_context}", &self.session_context)
    }

    /// Build the user prompt for CLAUDE.md
    pub fn build_claude_md_prompt(&self) -> String {
        let patches = if self.patches_summary.is_empty() {
            "No patches available.".to_string()
        } else {
            self.patches_summary
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {}", i + 1, s))
                .collect::<Vec<_>>()
                .join("\n")
        };

        CLAUDE_MD_USER_PROMPT
            .replace("{existing_content}", &self.existing_content)
            .replace("{patches_summary}", &patches)
            .replace("{session_context}", &self.session_context)
    }
}

/// Result of artifact synthesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSynthesisResult {
    /// The generated content
    pub content: String,
    /// Which backend was used
    pub backend: String,
}

/// Synthesize README.md content using the configured backend
pub async fn synthesize_readme(
    config: &ArtifactSynthesisConfig,
    input: &ArtifactSynthesisInput,
) -> Result<ArtifactSynthesisResult> {
    match config.backend {
        ArtifactSynthesisBackend::Template => {
            // Use rule-based generation
            let content = generate_readme_update(
                &input.existing_content,
                &input.session_context,
                &input.patches_summary,
            );
            Ok(ArtifactSynthesisResult {
                content,
                backend: "template".to_string(),
            })
        }
        ArtifactSynthesisBackend::OpenAi => {
            synthesize_with_openai(
                &config.openai,
                README_SYSTEM_PROMPT,
                &input.build_readme_prompt(),
            )
            .await
        }
        ArtifactSynthesisBackend::Grok => {
            synthesize_with_grok(
                &config.grok,
                README_SYSTEM_PROMPT,
                &input.build_readme_prompt(),
            )
            .await
        }
        ArtifactSynthesisBackend::VertexAnthropic => {
            synthesize_with_vertex(
                &config.vertex,
                README_SYSTEM_PROMPT,
                &input.build_readme_prompt(),
            )
            .await
        }
    }
}

/// Synthesize CLAUDE.md content using the configured backend
pub async fn synthesize_claude_md(
    config: &ArtifactSynthesisConfig,
    input: &ArtifactSynthesisInput,
) -> Result<ArtifactSynthesisResult> {
    match config.backend {
        ArtifactSynthesisBackend::Template => {
            // Use rule-based generation
            let content = generate_claude_md_update(
                &input.existing_content,
                &input.session_context,
                &input.patches_summary,
            );
            Ok(ArtifactSynthesisResult {
                content,
                backend: "template".to_string(),
            })
        }
        ArtifactSynthesisBackend::OpenAi => {
            synthesize_with_openai(
                &config.openai,
                CLAUDE_MD_SYSTEM_PROMPT,
                &input.build_claude_md_prompt(),
            )
            .await
        }
        ArtifactSynthesisBackend::Grok => {
            synthesize_with_grok(
                &config.grok,
                CLAUDE_MD_SYSTEM_PROMPT,
                &input.build_claude_md_prompt(),
            )
            .await
        }
        ArtifactSynthesisBackend::VertexAnthropic => {
            synthesize_with_vertex(
                &config.vertex,
                CLAUDE_MD_SYSTEM_PROMPT,
                &input.build_claude_md_prompt(),
            )
            .await
        }
    }
}

/// Synthesize using OpenAI API
async fn synthesize_with_openai(
    config: &SynthesisOpenAiSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<ArtifactSynthesisResult> {
    let api_key = config
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .context("OpenAI API key not configured")?;

    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com/v1");

    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": config.model,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": user_prompt
            }
        ],
        "max_tokens": 4000,
        "temperature": 0.3
    });

    let response = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send request to OpenAI")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("OpenAI API error ({}): {}", status, body);
    }

    let response_body: serde_json::Value = response.json().await?;
    let content = response_body["choices"][0]["message"]["content"]
        .as_str()
        .context("Invalid response format from OpenAI")?
        .trim()
        .to_string();

    Ok(ArtifactSynthesisResult {
        content,
        backend: "openai".to_string(),
    })
}

/// Synthesize using Grok API (xAI)
async fn synthesize_with_grok(
    config: &SynthesisGrokSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<ArtifactSynthesisResult> {
    let api_key = config
        .api_key
        .clone()
        .or_else(|| std::env::var("GROK_API_KEY").ok())
        .or_else(|| std::env::var("XAI_API_KEY").ok())
        .context("Grok API key not configured")?;

    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": config.model,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": user_prompt
            }
        ],
        "max_tokens": 4000,
        "temperature": 0.3
    });

    let response = client
        .post("https://api.x.ai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send request to Grok")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Grok API error ({}): {}", status, body);
    }

    let response_body: serde_json::Value = response.json().await?;
    let content = response_body["choices"][0]["message"]["content"]
        .as_str()
        .context("Invalid response format from Grok")?
        .trim()
        .to_string();

    Ok(ArtifactSynthesisResult {
        content,
        backend: "grok".to_string(),
    })
}

/// Synthesize using Vertex AI (Anthropic)
async fn synthesize_with_vertex(
    config: &SynthesisVertexSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<ArtifactSynthesisResult> {
    let project_id = config
        .project_id
        .clone()
        .or_else(|| std::env::var("VERTEX_AI_PROJECT_ID").ok())
        .context("Vertex AI project ID not configured")?;

    let location = config
        .location
        .clone()
        .or_else(|| std::env::var("VERTEX_AI_LOCATION").ok())
        .unwrap_or_else(|| "us-east5".to_string());

    // Get access token from gcloud
    let access_token = get_gcloud_access_token().await?;

    let client = reqwest::Client::new();

    let url = format!(
        "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:rawPredict",
        location, project_id, location, config.model
    );

    let request_body = serde_json::json!({
        "anthropic_version": "vertex-2023-10-16",
        "max_tokens": 4000,
        "system": system_prompt,
        "messages": [
            {
                "role": "user",
                "content": user_prompt
            }
        ]
    });

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send request to Vertex AI")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Vertex AI API error ({}): {}", status, body);
    }

    let response_body: serde_json::Value = response.json().await?;
    let content = response_body["content"][0]["text"]
        .as_str()
        .context("Invalid response format from Vertex AI")?
        .trim()
        .to_string();

    Ok(ArtifactSynthesisResult {
        content,
        backend: "vertex_anthropic".to_string(),
    })
}

/// Get access token from gcloud CLI
async fn get_gcloud_access_token() -> Result<String> {
    let output = tokio::process::Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .output()
        .await
        .context("Failed to run gcloud auth print-access-token")?;

    if !output.status.success() {
        bail!(
            "gcloud auth failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// =============================================================================
// Artifact File Structures
// =============================================================================

use tokio::fs;

/// Metadata for an artifact file (stored in HTML comment header)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactMeta {
    /// Target file path in the project (e.g., /Users/xlyk/Code/qbit/README.md)
    pub target: PathBuf,
    /// When this artifact was created
    pub created_at: DateTime<Utc>,
    /// Reason for the artifact (what changed)
    pub reason: String,
    /// Patch IDs this artifact is based on (if any)
    #[serde(default)]
    pub based_on_patches: Vec<u32>,
}

impl ArtifactMeta {
    /// Create new artifact metadata
    #[allow(dead_code)]
    pub fn new(target: PathBuf, reason: String) -> Self {
        Self {
            target,
            created_at: Utc::now(),
            reason,
            based_on_patches: Vec::new(),
        }
    }

    /// Create metadata with patch references
    pub fn with_patches(target: PathBuf, reason: String, patches: Vec<u32>) -> Self {
        Self {
            target,
            created_at: Utc::now(),
            reason,
            based_on_patches: patches,
        }
    }

    /// Format metadata as HTML comment header
    pub fn to_header(&self) -> String {
        let date_str = self.created_at.format("%Y-%m-%d %H:%M").to_string();
        let patches_str = if self.based_on_patches.is_empty() {
            String::new()
        } else {
            let patches: Vec<String> = self
                .based_on_patches
                .iter()
                .map(|id| format!("{:04}", id))
                .collect();
            format!("\nBased on patches: {}", patches.join(", "))
        };

        format!(
            "<!--\nTarget: {}\nCreated: {}\nReason: {}{}\n-->",
            self.target.display(),
            date_str,
            self.reason,
            patches_str
        )
    }

    /// Parse metadata from HTML comment header
    pub fn from_header(header: &str) -> Result<Self> {
        // Extract content between <!-- and -->
        let content = header
            .strip_prefix("<!--")
            .and_then(|s| s.strip_suffix("-->"))
            .map(|s| s.trim())
            .context("Invalid header format: missing <!-- --> delimiters")?;

        let mut target: Option<PathBuf> = None;
        let mut created_at: Option<DateTime<Utc>> = None;
        let mut reason: Option<String> = None;
        let mut based_on_patches: Vec<u32> = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(value) = line.strip_prefix("Target:") {
                target = Some(PathBuf::from(value.trim()));
            } else if let Some(value) = line.strip_prefix("Created:") {
                let date_str = value.trim();
                // Parse "YYYY-MM-DD HH:MM" format
                let naive = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M")
                    .context("Invalid date format, expected YYYY-MM-DD HH:MM")?;
                created_at = Some(DateTime::from_naive_utc_and_offset(naive, Utc));
            } else if let Some(value) = line.strip_prefix("Reason:") {
                reason = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("Based on patches:") {
                based_on_patches = value
                    .split(',')
                    .filter_map(|s| s.trim().parse::<u32>().ok())
                    .collect();
            }
        }

        Ok(Self {
            target: target.context("Missing Target field in header")?,
            created_at: created_at.context("Missing Created field in header")?,
            reason: reason.context("Missing Reason field in header")?,
            based_on_patches,
        })
    }
}

/// An artifact file with its metadata and content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactFile {
    /// Artifact metadata
    pub meta: ArtifactMeta,
    /// The artifact filename (e.g., "README.md", "CLAUDE.md")
    pub filename: String,
    /// The artifact content (without the metadata header)
    pub content: String,
}

impl ArtifactFile {
    /// Create a new artifact file
    pub fn new(filename: String, meta: ArtifactMeta, content: String) -> Self {
        Self {
            meta,
            filename,
            content,
        }
    }

    /// Format the full file content with metadata header
    pub fn to_file_content(&self) -> String {
        format!("{}\n\n{}", self.meta.to_header(), self.content)
    }

    /// Parse an artifact file from its content
    pub fn from_file_content(filename: &str, content: &str) -> Result<Self> {
        // Find the header end
        let header_end = content
            .find("-->")
            .context("Missing header end delimiter (-->)")?;

        let header = &content[..header_end + 3];
        let body = content[header_end + 3..].trim_start();

        let meta = ArtifactMeta::from_header(header)?;

        Ok(Self {
            meta,
            filename: filename.to_string(),
            content: body.to_string(),
        })
    }
}

/// Manages artifacts for a session
pub struct ArtifactManager {
    /// Session directory
    session_dir: PathBuf,
}

impl ArtifactManager {
    /// Subdirectory names
    const ARTIFACTS_DIR: &'static str = "artifacts";
    const PENDING_DIR: &'static str = "pending";
    const APPLIED_DIR: &'static str = "applied";

    /// Create a new artifact manager for a session
    pub fn new(session_dir: PathBuf) -> Self {
        Self { session_dir }
    }

    /// Get the path to pending artifacts directory
    pub fn pending_dir(&self) -> PathBuf {
        self.session_dir
            .join(Self::ARTIFACTS_DIR)
            .join(Self::PENDING_DIR)
    }

    /// Get the path to applied artifacts directory
    pub fn applied_dir(&self) -> PathBuf {
        self.session_dir
            .join(Self::ARTIFACTS_DIR)
            .join(Self::APPLIED_DIR)
    }

    /// Ensure artifact directories exist
    pub async fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.pending_dir())
            .await
            .context("Failed to create pending artifacts directory")?;
        fs::create_dir_all(self.applied_dir())
            .await
            .context("Failed to create applied artifacts directory")?;
        Ok(())
    }

    /// Create a pending artifact
    pub async fn create_artifact(&self, artifact: &ArtifactFile) -> Result<PathBuf> {
        self.ensure_dirs().await?;

        let path = self.pending_dir().join(&artifact.filename);
        let content = artifact.to_file_content();

        fs::write(&path, &content)
            .await
            .context("Failed to write artifact file")?;

        tracing::info!("Created pending artifact: {}", artifact.filename);
        Ok(path)
    }

    /// List all pending artifacts
    pub async fn list_pending(&self) -> Result<Vec<ArtifactFile>> {
        self.list_artifacts_in_dir(&self.pending_dir()).await
    }

    /// List all applied artifacts
    pub async fn list_applied(&self) -> Result<Vec<ArtifactFile>> {
        self.list_artifacts_in_dir(&self.applied_dir()).await
    }

    /// List artifacts in a directory
    async fn list_artifacts_in_dir(&self, dir: &Path) -> Result<Vec<ArtifactFile>> {
        let mut artifacts = Vec::new();

        if !dir.exists() {
            return Ok(artifacts);
        }

        let mut entries = fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                match self.load_artifact(&path).await {
                    Ok(artifact) => artifacts.push(artifact),
                    Err(e) => {
                        tracing::warn!("Failed to load artifact {:?}: {}", path, e);
                    }
                }
            }
        }

        // Sort by filename
        artifacts.sort_by(|a, b| a.filename.cmp(&b.filename));
        Ok(artifacts)
    }

    /// Load an artifact from a file
    async fn load_artifact(&self, path: &Path) -> Result<ArtifactFile> {
        let content = fs::read_to_string(path)
            .await
            .context("Failed to read artifact file")?;

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        ArtifactFile::from_file_content(&filename, &content)
    }

    /// Get a specific pending artifact by filename
    pub async fn get_pending(&self, filename: &str) -> Result<Option<ArtifactFile>> {
        let path = self.pending_dir().join(filename);
        if !path.exists() {
            return Ok(None);
        }
        self.load_artifact(&path).await.map(Some)
    }

    /// Discard a pending artifact
    pub async fn discard_artifact(&self, filename: &str) -> Result<bool> {
        let path = self.pending_dir().join(filename);
        if !path.exists() {
            return Ok(false);
        }

        fs::remove_file(&path)
            .await
            .context("Failed to remove artifact file")?;

        tracing::info!("Discarded artifact: {}", filename);
        Ok(true)
    }

    /// Apply an artifact (copy to target, move to applied)
    pub async fn apply_artifact(&self, filename: &str, git_root: &Path) -> Result<PathBuf> {
        let artifact = self
            .get_pending(filename)
            .await?
            .context(format!("Artifact {} not found in pending", filename))?;

        // Copy content (without metadata header) to target
        let target_path = &artifact.meta.target;

        // Ensure target directory exists
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create target directory")?;
        }

        // Write content to target
        fs::write(target_path, &artifact.content)
            .await
            .context("Failed to write to target file")?;

        // Git add the file
        let relative_path = target_path
            .strip_prefix(git_root)
            .unwrap_or(target_path)
            .to_string_lossy()
            .to_string();

        tokio::process::Command::new("git")
            .args(["add", &relative_path])
            .current_dir(git_root)
            .output()
            .await
            .context("Failed to git add artifact")?;

        // Move to applied directory
        let pending_path = self.pending_dir().join(filename);
        let applied_path = self.applied_dir().join(filename);

        self.ensure_dirs().await?;
        fs::rename(&pending_path, &applied_path)
            .await
            .context("Failed to move artifact to applied")?;

        tracing::info!("Applied artifact {} to {}", filename, target_path.display());
        Ok(target_path.clone())
    }

    /// Apply all pending artifacts
    pub async fn apply_all_artifacts(&self, git_root: &Path) -> Result<Vec<(String, PathBuf)>> {
        let pending = self.list_pending().await?;
        let mut results = Vec::new();

        for artifact in pending {
            match self.apply_artifact(&artifact.filename, git_root).await {
                Ok(path) => {
                    results.push((artifact.filename.clone(), path));
                }
                Err(e) => {
                    bail!(
                        "Failed to apply artifact {}: {}. Applied {} artifacts before failure.",
                        artifact.filename,
                        e,
                        results.len()
                    );
                }
            }
        }

        Ok(results)
    }

    /// Generate a diff between pending artifact and current target file
    pub async fn preview_artifact(&self, filename: &str) -> Result<String> {
        let artifact = self
            .get_pending(filename)
            .await?
            .context(format!("Artifact {} not found in pending", filename))?;

        let target_path = &artifact.meta.target;

        // Read current file content (if exists)
        let current_content = if target_path.exists() {
            fs::read_to_string(target_path).await.unwrap_or_default()
        } else {
            String::new()
        };

        // Generate a simple diff
        Ok(generate_simple_diff(&current_content, &artifact.content))
    }

    /// Regenerate artifacts based on applied patches (L2 -> L3 cascade)
    ///
    /// This method is called after patches are applied to update project documentation.
    /// Uses template-based generation by default. Call `regenerate_from_patches_with_config`
    /// to use LLM-based synthesis.
    #[allow(dead_code)]
    pub async fn regenerate_from_patches(
        &self,
        git_root: &Path,
        patch_subjects: &[String],
        session_context: &str,
    ) -> Result<Vec<PathBuf>> {
        // Use default template-based config
        let config = ArtifactSynthesisConfig::default();
        self.regenerate_from_patches_with_config(git_root, patch_subjects, session_context, &config)
            .await
    }

    /// Regenerate artifacts based on applied patches with explicit config (L2 -> L3 cascade)
    ///
    /// This method is called after patches are applied to update project documentation.
    /// - `Template` backend uses rule-based generation (fast, no API calls)
    /// - Other backends use LLM synthesis (better quality, requires API access)
    ///
    /// If LLM synthesis fails, falls back to template-based generation.
    pub async fn regenerate_from_patches_with_config(
        &self,
        git_root: &Path,
        patch_subjects: &[String],
        session_context: &str,
        config: &ArtifactSynthesisConfig,
    ) -> Result<Vec<PathBuf>> {
        self.ensure_dirs().await?;

        let mut created = Vec::new();

        // Build synthesis input
        let input = ArtifactSynthesisInput::new(
            String::new(), // Will be set per-artifact
            patch_subjects.to_vec(),
            session_context.to_string(),
        );

        // Try to update README.md if it exists
        let readme_path = git_root.join("README.md");
        if readme_path.exists() {
            let current_readme = fs::read_to_string(&readme_path).await.unwrap_or_default();

            let readme_input = ArtifactSynthesisInput::new(
                current_readme.clone(),
                input.patches_summary.clone(),
                input.session_context.clone(),
            );

            // Try LLM synthesis, fall back to template on failure
            let updated_readme = match synthesize_readme(config, &readme_input).await {
                Ok(result) => {
                    tracing::debug!("README synthesis using {} backend", result.backend);
                    result.content
                }
                Err(e) if config.uses_llm() => {
                    // Fall back to template if LLM fails
                    tracing::warn!(
                        "LLM synthesis failed for README.md, falling back to template: {}",
                        e
                    );
                    generate_readme_update(&current_readme, session_context, patch_subjects)
                }
                Err(e) => {
                    tracing::warn!("Template synthesis failed for README.md: {}", e);
                    continue_or_error(e)?
                }
            };

            // Only create artifact if there are actual changes
            if updated_readme != current_readme {
                let patch_ids: Vec<u32> = (1..=patch_subjects.len() as u32).collect();
                let meta = ArtifactMeta::with_patches(
                    readme_path.clone(),
                    format!(
                        "Updated based on {} applied patches ({})",
                        patch_subjects.len(),
                        config.backend
                    ),
                    patch_ids,
                );

                let artifact = ArtifactFile::new("README.md".to_string(), meta, updated_readme);
                let path = self.create_artifact(&artifact).await?;
                created.push(path);
            }
        }

        // Try to update CLAUDE.md if it exists
        let claude_md_path = git_root.join("CLAUDE.md");
        if claude_md_path.exists() {
            let current_claude_md = fs::read_to_string(&claude_md_path)
                .await
                .unwrap_or_default();

            let claude_input = ArtifactSynthesisInput::new(
                current_claude_md.clone(),
                input.patches_summary.clone(),
                input.session_context.clone(),
            );

            // Try LLM synthesis, fall back to template on failure
            let updated_claude_md = match synthesize_claude_md(config, &claude_input).await {
                Ok(result) => {
                    tracing::debug!("CLAUDE.md synthesis using {} backend", result.backend);
                    result.content
                }
                Err(e) if config.uses_llm() => {
                    // Fall back to template if LLM fails
                    tracing::warn!(
                        "LLM synthesis failed for CLAUDE.md, falling back to template: {}",
                        e
                    );
                    generate_claude_md_update(&current_claude_md, session_context, patch_subjects)
                }
                Err(e) => {
                    tracing::warn!("Template synthesis failed for CLAUDE.md: {}", e);
                    continue_or_error(e)?
                }
            };

            // Only create artifact if there are actual changes
            if updated_claude_md != current_claude_md {
                let patch_ids: Vec<u32> = (1..=patch_subjects.len() as u32).collect();
                let meta = ArtifactMeta::with_patches(
                    claude_md_path.clone(),
                    format!(
                        "Updated conventions from {} patches ({})",
                        patch_subjects.len(),
                        config.backend
                    ),
                    patch_ids,
                );

                let artifact = ArtifactFile::new("CLAUDE.md".to_string(), meta, updated_claude_md);
                let path = self.create_artifact(&artifact).await?;
                created.push(path);
            }
        }

        if !created.is_empty() {
            tracing::info!(
                "Regenerated {} artifacts from {} patches using {} backend",
                created.len(),
                patch_subjects.len(),
                config.backend
            );
        }

        Ok(created)
    }
}

/// Helper to continue or propagate error (for template fallback)
fn continue_or_error<T>(e: anyhow::Error) -> Result<T> {
    Err(e)
}

/// Generate a simple unified diff between two strings
fn generate_simple_diff(old: &str, new: &str) -> String {
    use std::fmt::Write;

    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut diff = String::new();
    let _ = writeln!(diff, "--- current");
    let _ = writeln!(diff, "+++ proposed");

    // Simple line-by-line comparison (not a real diff algorithm)
    let max_len = old_lines.len().max(new_lines.len());

    for i in 0..max_len {
        let old_line = old_lines.get(i).copied();
        let new_line = new_lines.get(i).copied();

        match (old_line, new_line) {
            (Some(o), Some(n)) if o == n => {
                let _ = writeln!(diff, " {}", o);
            }
            (Some(o), Some(n)) => {
                let _ = writeln!(diff, "-{}", o);
                let _ = writeln!(diff, "+{}", n);
            }
            (Some(o), None) => {
                let _ = writeln!(diff, "-{}", o);
            }
            (None, Some(n)) => {
                let _ = writeln!(diff, "+{}", n);
            }
            (None, None) => {}
        }
    }

    diff
}

// =============================================================================
// Rule-Based Artifact Generation
// =============================================================================

/// Generate artifact content based on session context and patches
///
/// This is the rule-based generator - a simple template-based approach
/// that will be replaced with LLM-based generation in Phase 6.
pub fn generate_readme_update(
    current_readme: &str,
    session_context: &str,
    patch_summaries: &[String],
) -> String {
    // For now, just return the current README with a note about changes
    // This is a placeholder for the LLM-based generator
    let changes_section = if patch_summaries.is_empty() {
        String::new()
    } else {
        let changes = patch_summaries.join("\n- ");
        format!(
            "\n\n## Recent Changes\n\n- {}\n\n(Generated from session: {})",
            changes,
            session_context.lines().next().unwrap_or("unknown session")
        )
    };

    format!("{}{}", current_readme, changes_section)
}

/// Generate CLAUDE.md update based on session context
pub fn generate_claude_md_update(
    current_claude_md: &str,
    session_context: &str,
    patch_summaries: &[String],
) -> String {
    // Simple rule-based update - append conventions discovered
    let changes_section = if patch_summaries.is_empty() {
        String::new()
    } else {
        let changes = patch_summaries.join("\n- ");
        format!(
            "\n\n## Session Notes\n\n- {}\n\n(From session context)",
            changes
        )
    };

    // Check if there's new information that should be added
    let _ = session_context; // Will be used in LLM-based generation

    format!("{}{}", current_claude_md, changes_section)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -------------------------------------------------------------------------
    // ArtifactMeta Tests
    // -------------------------------------------------------------------------

    mod artifact_meta {
        use super::*;

        #[test]
        fn creates_new_metadata() {
            let meta = ArtifactMeta::new(
                PathBuf::from("/path/to/README.md"),
                "Added authentication".to_string(),
            );

            assert_eq!(meta.target, PathBuf::from("/path/to/README.md"));
            assert_eq!(meta.reason, "Added authentication");
            assert!(meta.based_on_patches.is_empty());
        }

        #[test]
        fn creates_metadata_with_patches() {
            let meta = ArtifactMeta::with_patches(
                PathBuf::from("/path/to/README.md"),
                "Added auth".to_string(),
                vec![1, 2, 3],
            );

            assert_eq!(meta.based_on_patches, vec![1, 2, 3]);
        }

        #[test]
        fn formats_header_without_patches() {
            let meta = ArtifactMeta {
                target: PathBuf::from("/path/to/README.md"),
                created_at: DateTime::parse_from_rfc3339("2025-12-10T14:30:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                reason: "Added authentication feature".to_string(),
                based_on_patches: Vec::new(),
            };

            let header = meta.to_header();

            assert!(header.starts_with("<!--"));
            assert!(header.ends_with("-->"));
            assert!(header.contains("Target: /path/to/README.md"));
            assert!(header.contains("Created: 2025-12-10 14:30"));
            assert!(header.contains("Reason: Added authentication feature"));
            assert!(!header.contains("Based on patches"));
        }

        #[test]
        fn formats_header_with_patches() {
            let meta = ArtifactMeta {
                target: PathBuf::from("/path/to/README.md"),
                created_at: DateTime::parse_from_rfc3339("2025-12-10T14:30:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                reason: "Added authentication".to_string(),
                based_on_patches: vec![1, 2],
            };

            let header = meta.to_header();

            assert!(header.contains("Based on patches: 0001, 0002"));
        }

        #[test]
        fn parses_header_without_patches() {
            let header = r#"<!--
Target: /path/to/README.md
Created: 2025-12-10 14:30
Reason: Added authentication feature
-->"#;

            let meta = ArtifactMeta::from_header(header).unwrap();

            assert_eq!(meta.target, PathBuf::from("/path/to/README.md"));
            assert_eq!(meta.reason, "Added authentication feature");
            assert!(meta.based_on_patches.is_empty());
        }

        #[test]
        fn parses_header_with_patches() {
            let header = r#"<!--
Target: /path/to/CLAUDE.md
Created: 2025-12-10 15:00
Reason: Updated conventions
Based on patches: 0001, 0002, 0003
-->"#;

            let meta = ArtifactMeta::from_header(header).unwrap();

            assert_eq!(meta.target, PathBuf::from("/path/to/CLAUDE.md"));
            assert_eq!(meta.based_on_patches, vec![1, 2, 3]);
        }

        #[test]
        fn roundtrip_header() {
            let original = ArtifactMeta {
                target: PathBuf::from("/home/user/project/README.md"),
                created_at: DateTime::parse_from_rfc3339("2025-12-10T14:30:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                reason: "Added new feature".to_string(),
                based_on_patches: vec![1, 5, 10],
            };

            let header = original.to_header();
            let parsed = ArtifactMeta::from_header(&header).unwrap();

            assert_eq!(original.target, parsed.target);
            assert_eq!(original.reason, parsed.reason);
            assert_eq!(original.based_on_patches, parsed.based_on_patches);
            // Note: created_at might differ slightly due to formatting precision
        }

        #[test]
        fn returns_error_for_missing_delimiters() {
            let result = ArtifactMeta::from_header("No delimiters here");
            assert!(result.is_err());
        }

        #[test]
        fn returns_error_for_missing_target() {
            let header = r#"<!--
Created: 2025-12-10 14:30
Reason: Some reason
-->"#;

            let result = ArtifactMeta::from_header(header);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Target"));
        }

        #[test]
        fn returns_error_for_missing_created() {
            let header = r#"<!--
Target: /path/to/file.md
Reason: Some reason
-->"#;

            let result = ArtifactMeta::from_header(header);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Created"));
        }

        #[test]
        fn returns_error_for_missing_reason() {
            let header = r#"<!--
Target: /path/to/file.md
Created: 2025-12-10 14:30
-->"#;

            let result = ArtifactMeta::from_header(header);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Reason"));
        }
    }

    // -------------------------------------------------------------------------
    // ArtifactFile Tests
    // -------------------------------------------------------------------------

    mod artifact_file {
        use super::*;

        #[test]
        fn creates_artifact_file() {
            let meta = ArtifactMeta::new(
                PathBuf::from("/path/to/README.md"),
                "Added feature".to_string(),
            );

            let artifact = ArtifactFile::new(
                "README.md".to_string(),
                meta,
                "# Project\n\nDescription here.".to_string(),
            );

            assert_eq!(artifact.filename, "README.md");
            assert!(artifact.content.contains("# Project"));
        }

        #[test]
        fn formats_full_file_content() {
            let meta = ArtifactMeta {
                target: PathBuf::from("/path/to/README.md"),
                created_at: DateTime::parse_from_rfc3339("2025-12-10T14:30:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                reason: "Initial creation".to_string(),
                based_on_patches: Vec::new(),
            };

            let artifact = ArtifactFile::new(
                "README.md".to_string(),
                meta,
                "# My Project\n\nWelcome!".to_string(),
            );

            let content = artifact.to_file_content();

            assert!(content.starts_with("<!--"));
            assert!(content.contains("Target: /path/to/README.md"));
            assert!(content.contains("# My Project"));
            assert!(content.contains("Welcome!"));
        }

        #[test]
        fn parses_file_content() {
            let content = r#"<!--
Target: /path/to/CLAUDE.md
Created: 2025-12-10 14:30
Reason: Updated conventions
-->

# CLAUDE.md

Instructions for the AI assistant.

## Commands
- `cargo test` - Run tests"#;

            let artifact = ArtifactFile::from_file_content("CLAUDE.md", content).unwrap();

            assert_eq!(artifact.filename, "CLAUDE.md");
            assert_eq!(artifact.meta.target, PathBuf::from("/path/to/CLAUDE.md"));
            assert!(artifact.content.starts_with("# CLAUDE.md"));
            assert!(artifact.content.contains("## Commands"));
        }

        #[test]
        fn roundtrip_file_content() {
            let meta = ArtifactMeta {
                target: PathBuf::from("/project/README.md"),
                created_at: DateTime::parse_from_rfc3339("2025-12-10T14:30:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                reason: "Test roundtrip".to_string(),
                based_on_patches: vec![1, 2],
            };

            let original = ArtifactFile::new(
                "README.md".to_string(),
                meta,
                "# Title\n\nContent here.".to_string(),
            );

            let file_content = original.to_file_content();
            let parsed = ArtifactFile::from_file_content("README.md", &file_content).unwrap();

            assert_eq!(original.filename, parsed.filename);
            assert_eq!(original.meta.target, parsed.meta.target);
            assert_eq!(original.meta.reason, parsed.meta.reason);
            assert_eq!(original.content, parsed.content);
        }

        #[test]
        fn returns_error_for_missing_header() {
            let content = "# Just content, no header";
            let result = ArtifactFile::from_file_content("file.md", content);
            assert!(result.is_err());
        }
    }

    // -------------------------------------------------------------------------
    // ArtifactManager Tests
    // -------------------------------------------------------------------------

    mod artifact_manager {
        use super::*;

        async fn setup_test_dir() -> TempDir {
            TempDir::new().unwrap()
        }

        #[tokio::test]
        async fn creates_directories() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            manager.ensure_dirs().await.unwrap();

            assert!(temp.path().join("artifacts/pending").exists());
            assert!(temp.path().join("artifacts/applied").exists());
        }

        #[tokio::test]
        async fn creates_pending_artifact() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            let meta = ArtifactMeta::new(
                PathBuf::from("/project/README.md"),
                "Test artifact".to_string(),
            );
            let artifact =
                ArtifactFile::new("README.md".to_string(), meta, "# Content".to_string());

            let path = manager.create_artifact(&artifact).await.unwrap();

            assert!(path.exists());
            assert!(path.ends_with("pending/README.md"));
        }

        #[tokio::test]
        async fn lists_pending_artifacts() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create two artifacts
            let meta1 = ArtifactMeta::new(
                PathBuf::from("/project/README.md"),
                "Artifact 1".to_string(),
            );
            let artifact1 = ArtifactFile::new("README.md".to_string(), meta1, "# 1".to_string());
            manager.create_artifact(&artifact1).await.unwrap();

            let meta2 = ArtifactMeta::new(
                PathBuf::from("/project/CLAUDE.md"),
                "Artifact 2".to_string(),
            );
            let artifact2 = ArtifactFile::new("CLAUDE.md".to_string(), meta2, "# 2".to_string());
            manager.create_artifact(&artifact2).await.unwrap();

            let pending = manager.list_pending().await.unwrap();

            assert_eq!(pending.len(), 2);
            // Sorted by filename
            assert_eq!(pending[0].filename, "CLAUDE.md");
            assert_eq!(pending[1].filename, "README.md");
        }

        #[tokio::test]
        async fn gets_specific_pending_artifact() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            let meta = ArtifactMeta::new(
                PathBuf::from("/project/README.md"),
                "Test artifact".to_string(),
            );
            let artifact =
                ArtifactFile::new("README.md".to_string(), meta, "# Content".to_string());
            manager.create_artifact(&artifact).await.unwrap();

            let found = manager.get_pending("README.md").await.unwrap();
            assert!(found.is_some());
            assert_eq!(found.unwrap().filename, "README.md");

            let not_found = manager.get_pending("NOTEXIST.md").await.unwrap();
            assert!(not_found.is_none());
        }

        #[tokio::test]
        async fn discards_pending_artifact() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            let meta = ArtifactMeta::new(
                PathBuf::from("/project/README.md"),
                "Test artifact".to_string(),
            );
            let artifact =
                ArtifactFile::new("README.md".to_string(), meta, "# Content".to_string());
            manager.create_artifact(&artifact).await.unwrap();

            let discarded = manager.discard_artifact("README.md").await.unwrap();
            assert!(discarded);

            let pending = manager.list_pending().await.unwrap();
            assert!(pending.is_empty());

            // Discarding non-existent returns false
            let discarded_again = manager.discard_artifact("README.md").await.unwrap();
            assert!(!discarded_again);
        }

        #[tokio::test]
        async fn returns_empty_list_when_no_artifacts() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            let pending = manager.list_pending().await.unwrap();
            assert!(pending.is_empty());

            let applied = manager.list_applied().await.unwrap();
            assert!(applied.is_empty());
        }

        #[tokio::test]
        async fn generates_preview_diff() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create a "current" file in temp directory
            let target_path = temp.path().join("README.md");
            fs::write(&target_path, "# Old Title\n\nOld content.")
                .await
                .unwrap();

            let meta = ArtifactMeta::new(target_path.clone(), "Updated title".to_string());
            let artifact = ArtifactFile::new(
                "README.md".to_string(),
                meta,
                "# New Title\n\nNew content.".to_string(),
            );
            manager.create_artifact(&artifact).await.unwrap();

            let diff = manager.preview_artifact("README.md").await.unwrap();

            assert!(diff.contains("--- current"));
            assert!(diff.contains("+++ proposed"));
            assert!(diff.contains("-# Old Title"));
            assert!(diff.contains("+# New Title"));
        }

        #[tokio::test]
        async fn generates_preview_for_new_file() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Target file does NOT exist
            let target_path = temp.path().join("NEW_FILE.md");

            let meta = ArtifactMeta::new(target_path.clone(), "New file".to_string());
            let artifact = ArtifactFile::new(
                "NEW_FILE.md".to_string(),
                meta,
                "# New File\n\nThis is brand new content.".to_string(),
            );
            manager.create_artifact(&artifact).await.unwrap();

            let diff = manager.preview_artifact("NEW_FILE.md").await.unwrap();

            // All lines should be additions
            assert!(diff.contains("--- current"));
            assert!(diff.contains("+++ proposed"));
            assert!(diff.contains("+# New File"));
            assert!(diff.contains("+This is brand new content."));
        }

        #[tokio::test]
        async fn apply_artifact_copies_to_target() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create target directory (simulating git_root)
            let git_root = temp.path().join("repo");
            fs::create_dir_all(&git_root).await.unwrap();

            // Initialize a git repo for the git add command
            let _ = std::process::Command::new("git")
                .args(["init"])
                .current_dir(&git_root)
                .output();

            let target_path = git_root.join("README.md");
            let meta = ArtifactMeta::new(target_path.clone(), "Test artifact".to_string());
            let artifact = ArtifactFile::new(
                "README.md".to_string(),
                meta,
                "# Applied Content\n\nThis was applied.".to_string(),
            );
            manager.create_artifact(&artifact).await.unwrap();

            // Apply the artifact
            let result_path = manager
                .apply_artifact("README.md", &git_root)
                .await
                .unwrap();

            // Verify target file was created with correct content
            assert!(target_path.exists());
            let content = fs::read_to_string(&target_path).await.unwrap();
            assert_eq!(content, "# Applied Content\n\nThis was applied.");
            assert_eq!(result_path, target_path);
        }

        #[tokio::test]
        async fn apply_artifact_moves_to_applied() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create target directory
            let git_root = temp.path().join("repo");
            fs::create_dir_all(&git_root).await.unwrap();

            // Initialize a git repo
            let _ = std::process::Command::new("git")
                .args(["init"])
                .current_dir(&git_root)
                .output();

            let target_path = git_root.join("README.md");
            let meta = ArtifactMeta::new(target_path.clone(), "Test artifact".to_string());
            let artifact =
                ArtifactFile::new("README.md".to_string(), meta, "# Content".to_string());
            manager.create_artifact(&artifact).await.unwrap();

            // Verify artifact is in pending
            let pending_before = manager.list_pending().await.unwrap();
            assert_eq!(pending_before.len(), 1);

            // Apply the artifact
            manager
                .apply_artifact("README.md", &git_root)
                .await
                .unwrap();

            // Verify artifact moved from pending to applied
            let pending_after = manager.list_pending().await.unwrap();
            assert!(pending_after.is_empty());

            let applied = manager.list_applied().await.unwrap();
            assert_eq!(applied.len(), 1);
            assert_eq!(applied[0].filename, "README.md");
        }

        #[tokio::test]
        async fn apply_all_artifacts_applies_multiple() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create target directory
            let git_root = temp.path().join("repo");
            fs::create_dir_all(&git_root).await.unwrap();

            // Initialize a git repo
            let _ = std::process::Command::new("git")
                .args(["init"])
                .current_dir(&git_root)
                .output();

            // Create two artifacts
            let meta1 = ArtifactMeta::new(git_root.join("README.md"), "First".to_string());
            let artifact1 =
                ArtifactFile::new("README.md".to_string(), meta1, "# README".to_string());
            manager.create_artifact(&artifact1).await.unwrap();

            let meta2 = ArtifactMeta::new(git_root.join("CLAUDE.md"), "Second".to_string());
            let artifact2 =
                ArtifactFile::new("CLAUDE.md".to_string(), meta2, "# CLAUDE".to_string());
            manager.create_artifact(&artifact2).await.unwrap();

            // Apply all
            let results = manager.apply_all_artifacts(&git_root).await.unwrap();

            assert_eq!(results.len(), 2);

            // Verify both files exist
            assert!(git_root.join("README.md").exists());
            assert!(git_root.join("CLAUDE.md").exists());

            // Verify all moved to applied
            let pending = manager.list_pending().await.unwrap();
            assert!(pending.is_empty());

            let applied = manager.list_applied().await.unwrap();
            assert_eq!(applied.len(), 2);
        }

        #[tokio::test]
        async fn apply_artifact_returns_error_for_nonexistent() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            let git_root = temp.path().join("repo");
            fs::create_dir_all(&git_root).await.unwrap();

            let result = manager.apply_artifact("NONEXISTENT.md", &git_root).await;

            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("not found"));
        }

        #[tokio::test]
        async fn regenerate_from_patches_creates_readme_artifact() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create a git root with README
            let git_root = temp.path().join("repo");
            fs::create_dir_all(&git_root).await.unwrap();
            fs::write(
                git_root.join("README.md"),
                "# My Project\n\nOriginal content.",
            )
            .await
            .unwrap();

            // Regenerate artifacts from patches
            let patches = vec!["feat(auth): add login".to_string()];
            let context = "Goal: Implement authentication";

            let created = manager
                .regenerate_from_patches(&git_root, &patches, context)
                .await
                .unwrap();

            // Should create one artifact for README
            assert_eq!(created.len(), 1);

            let pending = manager.list_pending().await.unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].filename, "README.md");
            assert!(pending[0].content.contains("## Recent Changes"));
            assert!(pending[0].content.contains("feat(auth): add login"));
        }

        #[tokio::test]
        async fn regenerate_from_patches_creates_both_readme_and_claude() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create a git root with both README and CLAUDE.md
            let git_root = temp.path().join("repo");
            fs::create_dir_all(&git_root).await.unwrap();
            fs::write(git_root.join("README.md"), "# My Project")
                .await
                .unwrap();
            fs::write(git_root.join("CLAUDE.md"), "# CLAUDE.md\n\nInstructions.")
                .await
                .unwrap();

            // Regenerate artifacts from patches
            let patches = vec!["feat: new feature".to_string()];
            let context = "Goal: Add feature";

            let created = manager
                .regenerate_from_patches(&git_root, &patches, context)
                .await
                .unwrap();

            // Should create two artifacts
            assert_eq!(created.len(), 2);

            let pending = manager.list_pending().await.unwrap();
            assert_eq!(pending.len(), 2);
        }

        #[tokio::test]
        async fn regenerate_from_patches_no_artifacts_when_no_patches() {
            let temp = setup_test_dir().await;
            let manager = ArtifactManager::new(temp.path().to_path_buf());

            // Create a git root with README
            let git_root = temp.path().join("repo");
            fs::create_dir_all(&git_root).await.unwrap();
            fs::write(git_root.join("README.md"), "# My Project")
                .await
                .unwrap();

            // Regenerate with no patches
            let patches: Vec<String> = vec![];
            let context = "Goal: Nothing";

            let created = manager
                .regenerate_from_patches(&git_root, &patches, context)
                .await
                .unwrap();

            // Should not create any artifacts (no changes to make)
            assert!(created.is_empty());

            let pending = manager.list_pending().await.unwrap();
            assert!(pending.is_empty());
        }
    }

    // -------------------------------------------------------------------------
    // Rule-Based Generation Tests
    // -------------------------------------------------------------------------

    mod generation {
        use super::*;

        #[test]
        fn generates_readme_with_changes() {
            let current = "# Project\n\nA cool project.";
            let context = "Goal: Add authentication";
            let patches = vec!["feat(auth): add login".to_string()];

            let result = generate_readme_update(current, context, &patches);

            assert!(result.contains("# Project"));
            assert!(result.contains("## Recent Changes"));
            assert!(result.contains("feat(auth): add login"));
        }

        #[test]
        fn generates_readme_without_changes() {
            let current = "# Project\n\nA cool project.";
            let context = "Goal: Review code";
            let patches: Vec<String> = vec![];

            let result = generate_readme_update(current, context, &patches);

            assert_eq!(result, current);
        }

        #[test]
        fn generates_claude_md_with_changes() {
            let current = "# CLAUDE.md\n\nInstructions.";
            let context = "Session context here";
            let patches = vec!["Added new convention".to_string()];

            let result = generate_claude_md_update(current, context, &patches);

            assert!(result.contains("# CLAUDE.md"));
            assert!(result.contains("## Session Notes"));
        }
    }

    // -------------------------------------------------------------------------
    // Helper Function Tests
    // -------------------------------------------------------------------------

    mod helpers {
        use super::*;

        #[test]
        fn generates_simple_diff() {
            let old = "line1\nline2\nline3";
            let new = "line1\nmodified\nline3";

            let diff = generate_simple_diff(old, new);

            assert!(diff.contains("--- current"));
            assert!(diff.contains("+++ proposed"));
            assert!(diff.contains(" line1"));
            assert!(diff.contains("-line2"));
            assert!(diff.contains("+modified"));
        }

        #[test]
        fn generates_diff_for_added_lines() {
            let old = "line1";
            let new = "line1\nline2\nline3";

            let diff = generate_simple_diff(old, new);

            assert!(diff.contains("+line2"));
            assert!(diff.contains("+line3"));
        }

        #[test]
        fn generates_diff_for_removed_lines() {
            let old = "line1\nline2\nline3";
            let new = "line1";

            let diff = generate_simple_diff(old, new);

            assert!(diff.contains("-line2"));
            assert!(diff.contains("-line3"));
        }
    }

    // -------------------------------------------------------------------------
    // Artifact Synthesis Backend Tests
    // -------------------------------------------------------------------------

    mod synthesis_backend {
        use super::*;

        #[test]
        fn backend_from_str_template() {
            let backend: ArtifactSynthesisBackend = "template".parse().unwrap();
            assert_eq!(backend, ArtifactSynthesisBackend::Template);
        }

        #[test]
        fn backend_from_str_vertex() {
            let backend: ArtifactSynthesisBackend = "vertex_anthropic".parse().unwrap();
            assert_eq!(backend, ArtifactSynthesisBackend::VertexAnthropic);

            // Short form
            let backend: ArtifactSynthesisBackend = "vertex".parse().unwrap();
            assert_eq!(backend, ArtifactSynthesisBackend::VertexAnthropic);
        }

        #[test]
        fn backend_from_str_openai() {
            let backend: ArtifactSynthesisBackend = "openai".parse().unwrap();
            assert_eq!(backend, ArtifactSynthesisBackend::OpenAi);
        }

        #[test]
        fn backend_from_str_grok() {
            let backend: ArtifactSynthesisBackend = "grok".parse().unwrap();
            assert_eq!(backend, ArtifactSynthesisBackend::Grok);
        }

        #[test]
        fn backend_from_str_invalid() {
            let result: Result<ArtifactSynthesisBackend, _> = "invalid".parse();
            assert!(result.is_err());
        }

        #[test]
        fn backend_display() {
            assert_eq!(ArtifactSynthesisBackend::Template.to_string(), "template");
            assert_eq!(
                ArtifactSynthesisBackend::VertexAnthropic.to_string(),
                "vertex_anthropic"
            );
            assert_eq!(ArtifactSynthesisBackend::OpenAi.to_string(), "openai");
            assert_eq!(ArtifactSynthesisBackend::Grok.to_string(), "grok");
        }

        #[test]
        fn config_default_is_template() {
            let config = ArtifactSynthesisConfig::default();
            assert_eq!(config.backend, ArtifactSynthesisBackend::Template);
            assert!(!config.uses_llm());
        }

        #[test]
        fn config_uses_llm_when_not_template() {
            let mut config = ArtifactSynthesisConfig::default();
            config.backend = ArtifactSynthesisBackend::OpenAi;
            assert!(config.uses_llm());

            config.backend = ArtifactSynthesisBackend::VertexAnthropic;
            assert!(config.uses_llm());

            config.backend = ArtifactSynthesisBackend::Grok;
            assert!(config.uses_llm());
        }
    }

    // -------------------------------------------------------------------------
    // Artifact Synthesis Input Tests
    // -------------------------------------------------------------------------

    mod synthesis_input {
        use super::*;

        #[test]
        fn creates_synthesis_input() {
            let input = ArtifactSynthesisInput::new(
                "# README".to_string(),
                vec!["feat: add feature".to_string()],
                "Goal: Add new feature".to_string(),
            );

            assert_eq!(input.existing_content, "# README");
            assert_eq!(input.patches_summary.len(), 1);
            assert_eq!(input.session_context, "Goal: Add new feature");
        }

        #[test]
        fn builds_readme_prompt_with_patches() {
            let input = ArtifactSynthesisInput::new(
                "# My Project".to_string(),
                vec!["feat: add login".to_string(), "fix: fix bug".to_string()],
                "Session context".to_string(),
            );

            let prompt = input.build_readme_prompt();

            assert!(prompt.contains("# My Project"));
            assert!(prompt.contains("1. feat: add login"));
            assert!(prompt.contains("2. fix: fix bug"));
            assert!(prompt.contains("Session context"));
        }

        #[test]
        fn builds_readme_prompt_without_patches() {
            let input = ArtifactSynthesisInput::new(
                "# My Project".to_string(),
                vec![],
                "Session context".to_string(),
            );

            let prompt = input.build_readme_prompt();

            assert!(prompt.contains("# My Project"));
            assert!(prompt.contains("No patches available"));
        }

        #[test]
        fn builds_claude_md_prompt_with_patches() {
            let input = ArtifactSynthesisInput::new(
                "# CLAUDE.md\n\nInstructions".to_string(),
                vec!["refactor: update structure".to_string()],
                "Goal: Refactor".to_string(),
            );

            let prompt = input.build_claude_md_prompt();

            assert!(prompt.contains("# CLAUDE.md"));
            assert!(prompt.contains("1. refactor: update structure"));
            assert!(prompt.contains("Goal: Refactor"));
        }
    }

    // -------------------------------------------------------------------------
    // Artifact Synthesis Result Tests
    // -------------------------------------------------------------------------

    mod synthesis_result {
        use super::*;

        #[test]
        fn creates_synthesis_result() {
            let result = ArtifactSynthesisResult {
                content: "# Updated README".to_string(),
                backend: "template".to_string(),
            };

            assert_eq!(result.content, "# Updated README");
            assert_eq!(result.backend, "template");
        }

        #[test]
        fn synthesis_result_serializes() {
            let result = ArtifactSynthesisResult {
                content: "# Content".to_string(),
                backend: "openai".to_string(),
            };

            let json = serde_json::to_string(&result).unwrap();
            assert!(json.contains("\"content\":\"# Content\""));
            assert!(json.contains("\"backend\":\"openai\""));
        }
    }

    // -------------------------------------------------------------------------
    // Template Synthesis Tests (synchronous, no API calls)
    // -------------------------------------------------------------------------

    mod template_synthesis {
        use super::*;

        #[tokio::test]
        async fn synthesize_readme_with_template_backend() {
            let config = ArtifactSynthesisConfig::default();
            let input = ArtifactSynthesisInput::new(
                "# Project".to_string(),
                vec!["feat: new feature".to_string()],
                "Goal: Add feature".to_string(),
            );

            let result = synthesize_readme(&config, &input).await.unwrap();

            assert_eq!(result.backend, "template");
            assert!(result.content.contains("# Project"));
            assert!(result.content.contains("## Recent Changes"));
        }

        #[tokio::test]
        async fn synthesize_claude_md_with_template_backend() {
            let config = ArtifactSynthesisConfig::default();
            let input = ArtifactSynthesisInput::new(
                "# CLAUDE.md\n\nInstructions here.".to_string(),
                vec!["refactor: update structure".to_string()],
                "Session: Refactor codebase".to_string(),
            );

            let result = synthesize_claude_md(&config, &input).await.unwrap();

            assert_eq!(result.backend, "template");
            assert!(result.content.contains("# CLAUDE.md"));
            assert!(result.content.contains("## Session Notes"));
        }

        #[tokio::test]
        async fn synthesize_readme_no_changes_when_no_patches() {
            let config = ArtifactSynthesisConfig::default();
            let input = ArtifactSynthesisInput::new(
                "# Project\n\nExisting content.".to_string(),
                vec![],
                "No-op session".to_string(),
            );

            let result = synthesize_readme(&config, &input).await.unwrap();

            // Template returns content unchanged when no patches
            assert_eq!(result.content, "# Project\n\nExisting content.");
        }
    }
}

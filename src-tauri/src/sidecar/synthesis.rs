//! L2 Synthesis: LLM-based commit message generation.
//!
//! This module provides commit message synthesis using either:
//! - Template-based (rule-based) generation
//! - LLM-based generation via configurable backends
//!
//! ## Backends
//!
//! - `template` - Rule-based, no API calls (default)
//! - `vertex_anthropic` - Anthropic Claude via Vertex AI
//! - `openai` - OpenAI API (or compatible)
//! - `grok` - Grok API (xAI)

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::settings::schema::{
    QbitSettings, SidecarSettings, SynthesisGrokSettings, SynthesisOpenAiSettings,
    SynthesisVertexSettings,
};

// =============================================================================
// Prompt Templates
// =============================================================================

/// System prompt for LLM-based commit message generation
pub const COMMIT_MESSAGE_SYSTEM_PROMPT: &str = r#"You are a commit message generator. Generate concise, conventional commit messages from git diffs.

## Guidelines
- Use conventional commit format: type(scope): description
- Types: feat, fix, refactor, docs, test, chore, perf, style, build, ci
- First line (subject) must be <= 72 characters
- Body explains what changed and why (not how)
- Be specific but concise

## Conventional Commit Types
- feat: A new feature for the user
- fix: A bug fix for the user
- refactor: Code restructuring without behavior change
- docs: Documentation only changes
- test: Adding or updating tests
- chore: Maintenance tasks, dependencies, tooling
- perf: Performance improvements
- style: Code style/formatting changes
- build: Build system or external dependency changes
- ci: CI configuration changes

## Format
```
type(scope): short description

Optional body explaining the change in more detail.
What was changed and why (not how).
```

Return ONLY the commit message, no additional text or markdown formatting."#;

/// User prompt template for commit message generation
pub const COMMIT_MESSAGE_USER_PROMPT: &str = r#"Generate a commit message for the following changes:

## Session Context
{context}

## Git Diff
```diff
{diff}
```

## Files Changed
{files}

Generate a conventional commit message for these changes."#;

// =============================================================================
// Configuration
// =============================================================================

/// Synthesis backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisBackend {
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

impl std::str::FromStr for SynthesisBackend {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "template" => Ok(SynthesisBackend::Template),
            "vertex_anthropic" | "vertex" => Ok(SynthesisBackend::VertexAnthropic),
            "openai" => Ok(SynthesisBackend::OpenAi),
            "grok" => Ok(SynthesisBackend::Grok),
            _ => bail!("Unknown synthesis backend: {}", s),
        }
    }
}

impl std::fmt::Display for SynthesisBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SynthesisBackend::Template => write!(f, "template"),
            SynthesisBackend::VertexAnthropic => write!(f, "vertex_anthropic"),
            SynthesisBackend::OpenAi => write!(f, "openai"),
            SynthesisBackend::Grok => write!(f, "grok"),
        }
    }
}

/// Configuration for synthesis operations
#[derive(Debug, Clone)]
pub struct SynthesisConfig {
    /// Whether synthesis is enabled
    pub enabled: bool,
    /// Which backend to use
    pub backend: SynthesisBackend,
    /// Vertex AI settings (when backend = vertex_anthropic)
    pub vertex: SynthesisVertexSettings,
    /// OpenAI settings (when backend = openai)
    pub openai: SynthesisOpenAiSettings,
    /// Grok settings (when backend = grok)
    pub grok: SynthesisGrokSettings,
}

impl Default for SynthesisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: SynthesisBackend::Template,
            vertex: SynthesisVertexSettings::default(),
            openai: SynthesisOpenAiSettings::default(),
            grok: SynthesisGrokSettings::default(),
        }
    }
}

impl SynthesisConfig {
    /// Create config from QbitSettings
    #[allow(dead_code)]
    pub fn from_settings(settings: &QbitSettings) -> Self {
        let sidecar = &settings.sidecar;
        let backend = sidecar.synthesis_backend.parse().unwrap_or_default();

        Self {
            enabled: sidecar.synthesis_enabled,
            backend,
            vertex: sidecar.synthesis_vertex.clone(),
            openai: sidecar.synthesis_openai.clone(),
            grok: sidecar.synthesis_grok.clone(),
        }
    }

    /// Create config from SidecarSettings only
    pub fn from_sidecar_settings(settings: &SidecarSettings) -> Self {
        let backend = settings.synthesis_backend.parse().unwrap_or_default();

        Self {
            enabled: settings.synthesis_enabled,
            backend,
            vertex: settings.synthesis_vertex.clone(),
            openai: settings.synthesis_openai.clone(),
            grok: settings.synthesis_grok.clone(),
        }
    }
}

// =============================================================================
// Synthesis Input/Output
// =============================================================================

/// Input for commit message synthesis
#[derive(Debug, Clone)]
pub struct SynthesisInput {
    /// Git diff content
    pub diff: String,
    /// Files changed (for context)
    pub files: Vec<PathBuf>,
    /// Session context (from state.md)
    pub session_context: Option<String>,
}

impl SynthesisInput {
    /// Create a new synthesis input
    pub fn new(diff: String, files: Vec<PathBuf>) -> Self {
        Self {
            diff,
            files,
            session_context: None,
        }
    }

    /// Add session context
    pub fn with_context(mut self, context: String) -> Self {
        self.session_context = Some(context);
        self
    }

    /// Format files list for prompt
    fn format_files(&self) -> String {
        self.files
            .iter()
            .map(|p| format!("- {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Build the user prompt for LLM
    pub fn build_prompt(&self) -> String {
        let context = self
            .session_context
            .as_deref()
            .unwrap_or("No session context available.");

        COMMIT_MESSAGE_USER_PROMPT
            .replace("{context}", context)
            .replace("{diff}", &self.diff)
            .replace("{files}", &self.format_files())
    }
}

/// Result of commit message synthesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    /// The generated commit message
    pub message: String,
    /// Which backend was used
    pub backend: String,
    /// Whether this was regenerated
    pub regenerated: bool,
}

// =============================================================================
// Synthesizer Trait and Implementations
// =============================================================================

/// Trait for commit message synthesis
#[async_trait::async_trait]
pub trait CommitMessageSynthesizer: Send + Sync {
    /// Generate a commit message from input
    async fn synthesize(&self, input: &SynthesisInput) -> Result<SynthesisResult>;

    /// Get the backend name
    #[allow(dead_code)]
    fn backend_name(&self) -> &'static str;
}

/// Template-based (rule-based) synthesizer
pub struct TemplateSynthesizer;

impl TemplateSynthesizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TemplateSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CommitMessageSynthesizer for TemplateSynthesizer {
    async fn synthesize(&self, input: &SynthesisInput) -> Result<SynthesisResult> {
        let message = generate_template_message(&input.files, &input.diff);
        Ok(SynthesisResult {
            message,
            backend: "template".to_string(),
            regenerated: false,
        })
    }

    fn backend_name(&self) -> &'static str {
        "template"
    }
}

/// OpenAI-based synthesizer (also works with compatible APIs)
pub struct OpenAiSynthesizer {
    api_key: String,
    model: String,
    base_url: Option<String>,
}

impl OpenAiSynthesizer {
    pub fn new(config: &SynthesisOpenAiSettings) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .context("OpenAI API key not configured")?;

        Ok(Self {
            api_key,
            model: config.model.clone(),
            base_url: config.base_url.clone(),
        })
    }
}

#[async_trait::async_trait]
impl CommitMessageSynthesizer for OpenAiSynthesizer {
    async fn synthesize(&self, input: &SynthesisInput) -> Result<SynthesisResult> {
        let base_url = self
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1");

        let client = reqwest::Client::new();

        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": COMMIT_MESSAGE_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": input.build_prompt()
                }
            ],
            "max_tokens": 500,
            "temperature": 0.3
        });

        let response = client
            .post(format!("{}/chat/completions", base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
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
        let message = response_body["choices"][0]["message"]["content"]
            .as_str()
            .context("Invalid response format from OpenAI")?
            .trim()
            .to_string();

        Ok(SynthesisResult {
            message,
            backend: "openai".to_string(),
            regenerated: false,
        })
    }

    fn backend_name(&self) -> &'static str {
        "openai"
    }
}

/// Grok-based synthesizer (xAI)
pub struct GrokSynthesizer {
    api_key: String,
    model: String,
}

impl GrokSynthesizer {
    pub fn new(config: &SynthesisGrokSettings) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("GROK_API_KEY").ok())
            .or_else(|| std::env::var("XAI_API_KEY").ok())
            .context("Grok API key not configured")?;

        Ok(Self {
            api_key,
            model: config.model.clone(),
        })
    }
}

#[async_trait::async_trait]
impl CommitMessageSynthesizer for GrokSynthesizer {
    async fn synthesize(&self, input: &SynthesisInput) -> Result<SynthesisResult> {
        let client = reqwest::Client::new();

        // Grok uses OpenAI-compatible API
        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": COMMIT_MESSAGE_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": input.build_prompt()
                }
            ],
            "max_tokens": 500,
            "temperature": 0.3
        });

        let response = client
            .post("https://api.x.ai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
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
        let message = response_body["choices"][0]["message"]["content"]
            .as_str()
            .context("Invalid response format from Grok")?
            .trim()
            .to_string();

        Ok(SynthesisResult {
            message,
            backend: "grok".to_string(),
            regenerated: false,
        })
    }

    fn backend_name(&self) -> &'static str {
        "grok"
    }
}

/// Vertex AI Anthropic synthesizer
pub struct VertexAnthropicSynthesizer {
    project_id: String,
    location: String,
    model: String,
    credentials_path: Option<String>,
}

impl VertexAnthropicSynthesizer {
    pub fn new(config: &SynthesisVertexSettings) -> Result<Self> {
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

        Ok(Self {
            project_id,
            location,
            model: config.model.clone(),
            credentials_path: config.credentials_path.clone(),
        })
    }

    /// Get an access token using gcloud or service account
    async fn get_access_token(&self) -> Result<String> {
        // Try service account credentials first
        if let Some(creds_path) = &self.credentials_path {
            return self.get_token_from_service_account(creds_path).await;
        }

        // Fall back to GOOGLE_APPLICATION_CREDENTIALS
        if let Ok(creds_path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            return self.get_token_from_service_account(&creds_path).await;
        }

        // Fall back to gcloud CLI
        self.get_token_from_gcloud().await
    }

    async fn get_token_from_service_account(&self, _creds_path: &str) -> Result<String> {
        // For now, fall back to gcloud - full service account auth requires more setup
        // TODO: Implement proper service account JWT auth
        self.get_token_from_gcloud().await
    }

    async fn get_token_from_gcloud(&self) -> Result<String> {
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
}

#[async_trait::async_trait]
impl CommitMessageSynthesizer for VertexAnthropicSynthesizer {
    async fn synthesize(&self, input: &SynthesisInput) -> Result<SynthesisResult> {
        let access_token = self.get_access_token().await?;

        let client = reqwest::Client::new();

        // Vertex AI Anthropic uses a specific endpoint format
        let url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:rawPredict",
            self.location, self.project_id, self.location, self.model
        );

        let request_body = serde_json::json!({
            "anthropic_version": "vertex-2023-10-16",
            "max_tokens": 500,
            "system": COMMIT_MESSAGE_SYSTEM_PROMPT,
            "messages": [
                {
                    "role": "user",
                    "content": input.build_prompt()
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
        let message = response_body["content"][0]["text"]
            .as_str()
            .context("Invalid response format from Vertex AI")?
            .trim()
            .to_string();

        Ok(SynthesisResult {
            message,
            backend: "vertex_anthropic".to_string(),
            regenerated: false,
        })
    }

    fn backend_name(&self) -> &'static str {
        "vertex_anthropic"
    }
}

// =============================================================================
// Factory Function
// =============================================================================

/// Create a synthesizer based on configuration
pub fn create_synthesizer(config: &SynthesisConfig) -> Result<Box<dyn CommitMessageSynthesizer>> {
    match config.backend {
        SynthesisBackend::Template => Ok(Box::new(TemplateSynthesizer::new())),
        SynthesisBackend::OpenAi => Ok(Box::new(OpenAiSynthesizer::new(&config.openai)?)),
        SynthesisBackend::Grok => Ok(Box::new(GrokSynthesizer::new(&config.grok)?)),
        SynthesisBackend::VertexAnthropic => {
            Ok(Box::new(VertexAnthropicSynthesizer::new(&config.vertex)?))
        }
    }
}

// =============================================================================
// Template-Based Generation (Rule-Based)
// =============================================================================

/// Generate a commit message using template/rule-based approach
pub fn generate_template_message(files: &[PathBuf], diff: &str) -> String {
    // Analyze the changes
    let analysis = analyze_changes(files, diff);

    // Determine commit type
    let commit_type = infer_commit_type(&analysis);

    // Determine scope
    let scope = infer_scope(files);

    // Generate subject line
    let subject = generate_subject(&analysis, commit_type);

    // Generate body (optional, for more complex changes)
    let body = generate_body(&analysis);

    // Format the message
    if let Some(s) = scope {
        if body.is_empty() {
            format!("{}({}): {}", commit_type, s, subject)
        } else {
            format!("{}({}): {}\n\n{}", commit_type, s, subject, body)
        }
    } else if body.is_empty() {
        format!("{}: {}", commit_type, subject)
    } else {
        format!("{}: {}\n\n{}", commit_type, subject, body)
    }
}

/// Analysis of changes for template-based generation
#[derive(Debug, Default)]
struct ChangeAnalysis {
    /// Number of files added
    files_added: usize,
    /// Number of files modified
    files_modified: usize,
    /// Number of files deleted
    files_deleted: usize,
    /// Number of lines added
    lines_added: usize,
    /// Number of lines deleted
    lines_deleted: usize,
    /// Whether changes appear to be tests
    is_test: bool,
    /// Whether changes appear to be documentation
    is_docs: bool,
    /// Whether changes appear to be configuration
    is_config: bool,
    /// Key file names
    key_files: Vec<String>,
}

fn analyze_changes(files: &[PathBuf], diff: &str) -> ChangeAnalysis {
    let mut analysis = ChangeAnalysis::default();

    // Count files by type
    for file in files {
        let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let path_str = file.to_string_lossy().to_lowercase();

        // Check file type
        if path_str.contains("test") || path_str.contains("spec") {
            analysis.is_test = true;
        }
        if filename.ends_with(".md")
            || filename.ends_with(".txt")
            || path_str.contains("doc")
            || filename == "README"
        {
            analysis.is_docs = true;
        }
        if filename.ends_with(".toml")
            || filename.ends_with(".json")
            || filename.ends_with(".yaml")
            || filename.ends_with(".yml")
            || filename == ".env"
            || filename.starts_with(".")
        {
            analysis.is_config = true;
        }

        // Extract key file name (without extension)
        if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
            if !analysis.key_files.contains(&stem.to_string()) && analysis.key_files.len() < 3 {
                analysis.key_files.push(stem.to_string());
            }
        }
    }

    // Analyze diff
    for line in diff.lines() {
        if line.starts_with("new file") {
            analysis.files_added += 1;
        } else if line.starts_with("deleted file") {
            analysis.files_deleted += 1;
        } else if line.starts_with('+') && !line.starts_with("+++") {
            analysis.lines_added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            analysis.lines_deleted += 1;
        }
    }

    // Modified = total - added - deleted
    analysis.files_modified = files
        .len()
        .saturating_sub(analysis.files_added + analysis.files_deleted);

    analysis
}

fn infer_commit_type(analysis: &ChangeAnalysis) -> &'static str {
    if analysis.is_test {
        return "test";
    }
    if analysis.is_docs {
        return "docs";
    }
    if analysis.is_config {
        return "chore";
    }
    if analysis.files_added > 0 && analysis.files_modified == 0 && analysis.files_deleted == 0 {
        return "feat";
    }
    if analysis.files_deleted > 0 && analysis.files_added == 0 {
        return "refactor";
    }
    if analysis.lines_deleted > analysis.lines_added * 2 {
        return "refactor";
    }
    if analysis.files_modified > 0 && analysis.files_added == 0 {
        // Small changes are likely fixes, larger ones are features
        if analysis.lines_added < 50 {
            return "fix";
        }
        return "feat";
    }
    "chore"
}

fn infer_scope(files: &[PathBuf]) -> Option<String> {
    if files.is_empty() {
        return None;
    }

    // Find common directory
    let first = &files[0];
    let components: Vec<_> = first.components().collect();

    // Get the file name separately (we'll skip it when looking for directories)
    let filename = first.file_name().and_then(|n| n.to_str());

    if components.len() > 1 {
        // Use the first meaningful directory component (excluding the filename)
        for component in &components {
            let name = component.as_os_str().to_string_lossy();
            // Skip src, lib, hidden dirs, and the filename itself
            if name != "src" && name != "lib" && !name.starts_with('.') && filename != Some(&*name)
            {
                // Limit scope length
                if name.len() <= 15 {
                    return Some(name.to_string());
                }
            }
        }
    }

    // Use file stem if only one file
    if files.len() == 1 {
        if let Some(stem) = first.file_stem().and_then(|s| s.to_str()) {
            if stem.len() <= 15 {
                return Some(stem.to_string());
            }
        }
    }

    None
}

fn generate_subject(analysis: &ChangeAnalysis, commit_type: &str) -> String {
    let action = match (
        analysis.files_added,
        analysis.files_modified,
        analysis.files_deleted,
    ) {
        (n, 0, 0) if n > 0 => "add",
        (0, n, 0) if n > 0 => "update",
        (0, 0, n) if n > 0 => "remove",
        (a, m, 0) if a > 0 && m > 0 => "add and update",
        (0, m, d) if m > 0 && d > 0 => "update and remove",
        _ => "update",
    };

    let target = if !analysis.key_files.is_empty() {
        if analysis.key_files.len() == 1 {
            analysis.key_files[0].clone()
        } else {
            format!(
                "{} and {} more",
                analysis.key_files[0],
                analysis.key_files.len() - 1
            )
        }
    } else {
        match commit_type {
            "test" => "tests".to_string(),
            "docs" => "documentation".to_string(),
            "chore" => "configuration".to_string(),
            _ => "files".to_string(),
        }
    };

    format!("{} {}", action, target)
}

fn generate_body(analysis: &ChangeAnalysis) -> String {
    // Only generate body for larger changes
    let total_changes = analysis.lines_added + analysis.lines_deleted;
    if total_changes < 20 {
        return String::new();
    }

    let mut parts = Vec::new();

    if analysis.files_added > 0 {
        parts.push(format!(
            "{} file{} added",
            analysis.files_added,
            if analysis.files_added == 1 { "" } else { "s" }
        ));
    }
    if analysis.files_modified > 0 {
        parts.push(format!(
            "{} file{} modified",
            analysis.files_modified,
            if analysis.files_modified == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    if analysis.files_deleted > 0 {
        parts.push(format!(
            "{} file{} deleted",
            analysis.files_deleted,
            if analysis.files_deleted == 1 { "" } else { "s" }
        ));
    }

    if parts.is_empty() {
        return String::new();
    }

    format!(
        "Changes: {} (+{} -{} lines)",
        parts.join(", "),
        analysis.lines_added,
        analysis.lines_deleted
    )
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // SynthesisBackend tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_synthesis_backend_from_str() {
        assert_eq!(
            "template".parse::<SynthesisBackend>().unwrap(),
            SynthesisBackend::Template
        );
        assert_eq!(
            "vertex_anthropic".parse::<SynthesisBackend>().unwrap(),
            SynthesisBackend::VertexAnthropic
        );
        assert_eq!(
            "vertex".parse::<SynthesisBackend>().unwrap(),
            SynthesisBackend::VertexAnthropic
        );
        assert_eq!(
            "openai".parse::<SynthesisBackend>().unwrap(),
            SynthesisBackend::OpenAi
        );
        assert_eq!(
            "grok".parse::<SynthesisBackend>().unwrap(),
            SynthesisBackend::Grok
        );
        assert!("invalid".parse::<SynthesisBackend>().is_err());
    }

    #[test]
    fn test_synthesis_backend_display() {
        assert_eq!(SynthesisBackend::Template.to_string(), "template");
        assert_eq!(
            SynthesisBackend::VertexAnthropic.to_string(),
            "vertex_anthropic"
        );
        assert_eq!(SynthesisBackend::OpenAi.to_string(), "openai");
        assert_eq!(SynthesisBackend::Grok.to_string(), "grok");
    }

    // -------------------------------------------------------------------------
    // SynthesisInput tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_synthesis_input_creation() {
        let input = SynthesisInput::new(
            "diff content".to_string(),
            vec![PathBuf::from("src/main.rs")],
        );
        assert_eq!(input.diff, "diff content");
        assert_eq!(input.files.len(), 1);
        assert!(input.session_context.is_none());
    }

    #[test]
    fn test_synthesis_input_with_context() {
        let input = SynthesisInput::new("diff".to_string(), vec![])
            .with_context("session context".to_string());
        assert_eq!(input.session_context, Some("session context".to_string()));
    }

    #[test]
    fn test_synthesis_input_format_files() {
        let input = SynthesisInput::new(
            "".to_string(),
            vec![PathBuf::from("src/main.rs"), PathBuf::from("src/lib.rs")],
        );
        let formatted = input.format_files();
        assert!(formatted.contains("- src/main.rs"));
        assert!(formatted.contains("- src/lib.rs"));
    }

    #[test]
    fn test_synthesis_input_build_prompt() {
        let input = SynthesisInput::new(
            "+fn new() {}".to_string(),
            vec![PathBuf::from("src/lib.rs")],
        )
        .with_context("Adding a new function".to_string());

        let prompt = input.build_prompt();
        assert!(prompt.contains("Adding a new function"));
        assert!(prompt.contains("+fn new() {}"));
        assert!(prompt.contains("- src/lib.rs"));
    }

    // -------------------------------------------------------------------------
    // Template synthesizer tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_template_synthesizer_basic() {
        let synthesizer = TemplateSynthesizer::new();
        let input = SynthesisInput::new(
            "+pub fn hello() {}\n".to_string(),
            vec![PathBuf::from("src/lib.rs")],
        );

        let result = synthesizer.synthesize(&input).await.unwrap();
        assert_eq!(result.backend, "template");
        assert!(!result.regenerated);
        assert!(!result.message.is_empty());
    }

    #[test]
    fn test_template_synthesizer_backend_name() {
        let synthesizer = TemplateSynthesizer::new();
        assert_eq!(synthesizer.backend_name(), "template");
    }

    // -------------------------------------------------------------------------
    // Template-based message generation tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_generate_template_message_new_file() {
        let files = vec![PathBuf::from("src/auth.rs")];
        let diff = "new file mode 100644\n+pub fn authenticate() {}";

        let message = generate_template_message(&files, diff);
        assert!(message.contains("feat") || message.contains("add"));
        assert!(message.contains("auth"));
    }

    #[test]
    fn test_generate_template_message_modified_file() {
        let files = vec![PathBuf::from("src/lib.rs")];
        let diff = "-old line\n+new line";

        let message = generate_template_message(&files, diff);
        assert!(message.contains("update") || message.contains("fix"));
    }

    #[test]
    fn test_generate_template_message_test_file() {
        let files = vec![PathBuf::from("tests/auth_test.rs")];
        let diff = "+#[test]\n+fn test_auth() {}";

        let message = generate_template_message(&files, diff);
        assert!(message.starts_with("test"));
    }

    #[test]
    fn test_generate_template_message_docs() {
        let files = vec![PathBuf::from("README.md")];
        let diff = "+## New Section";

        let message = generate_template_message(&files, diff);
        assert!(message.starts_with("docs"));
    }

    #[test]
    fn test_generate_template_message_config() {
        let files = vec![PathBuf::from("Cargo.toml")];
        let diff = "+[dependencies]\n+tokio = \"1.0\"";

        let message = generate_template_message(&files, diff);
        assert!(message.starts_with("chore"));
    }

    #[test]
    fn test_generate_template_message_multiple_files() {
        let files = vec![
            PathBuf::from("src/lib.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/utils.rs"),
        ];
        let diff = "+new code\n-old code";

        let message = generate_template_message(&files, diff);
        assert!(message.contains("and") || message.contains("files"));
    }

    // -------------------------------------------------------------------------
    // Change analysis tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_analyze_changes_counts_additions() {
        let files = vec![PathBuf::from("src/new.rs")];
        let diff = "new file mode 100644\n+line1\n+line2\n+line3";

        let analysis = analyze_changes(&files, diff);
        assert_eq!(analysis.files_added, 1);
        assert_eq!(analysis.lines_added, 3);
    }

    #[test]
    fn test_analyze_changes_counts_deletions() {
        let files = vec![PathBuf::from("src/old.rs")];
        let diff = "deleted file mode 100644\n-line1\n-line2";

        let analysis = analyze_changes(&files, diff);
        assert_eq!(analysis.files_deleted, 1);
        assert_eq!(analysis.lines_deleted, 2);
    }

    #[test]
    fn test_analyze_changes_detects_test_files() {
        let files = vec![PathBuf::from("tests/my_test.rs")];
        let diff = "";

        let analysis = analyze_changes(&files, diff);
        assert!(analysis.is_test);
    }

    #[test]
    fn test_analyze_changes_detects_docs() {
        let files = vec![PathBuf::from("docs/guide.md")];
        let diff = "";

        let analysis = analyze_changes(&files, diff);
        assert!(analysis.is_docs);
    }

    #[test]
    fn test_analyze_changes_detects_config() {
        let files = vec![PathBuf::from("config.yaml")];
        let diff = "";

        let analysis = analyze_changes(&files, diff);
        assert!(analysis.is_config);
    }

    // -------------------------------------------------------------------------
    // Commit type inference tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_infer_commit_type_test() {
        let mut analysis = ChangeAnalysis::default();
        analysis.is_test = true;
        assert_eq!(infer_commit_type(&analysis), "test");
    }

    #[test]
    fn test_infer_commit_type_docs() {
        let mut analysis = ChangeAnalysis::default();
        analysis.is_docs = true;
        assert_eq!(infer_commit_type(&analysis), "docs");
    }

    #[test]
    fn test_infer_commit_type_feat_new_files() {
        let mut analysis = ChangeAnalysis::default();
        analysis.files_added = 2;
        assert_eq!(infer_commit_type(&analysis), "feat");
    }

    #[test]
    fn test_infer_commit_type_refactor_large_deletion() {
        let mut analysis = ChangeAnalysis::default();
        analysis.lines_deleted = 100;
        analysis.lines_added = 10;
        assert_eq!(infer_commit_type(&analysis), "refactor");
    }

    // -------------------------------------------------------------------------
    // Scope inference tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_infer_scope_single_file() {
        let files = vec![PathBuf::from("src/auth.rs")];
        let scope = infer_scope(&files);
        assert_eq!(scope, Some("auth".to_string()));
    }

    #[test]
    fn test_infer_scope_empty_files() {
        let files: Vec<PathBuf> = vec![];
        let scope = infer_scope(&files);
        assert!(scope.is_none());
    }

    #[test]
    fn test_infer_scope_skips_src() {
        let files = vec![PathBuf::from("src/auth/login.rs")];
        let scope = infer_scope(&files);
        // Should pick "auth" not "src"
        assert_eq!(scope, Some("auth".to_string()));
    }

    // -------------------------------------------------------------------------
    // SynthesisConfig tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_synthesis_config_default() {
        let config = SynthesisConfig::default();
        assert!(config.enabled);
        assert_eq!(config.backend, SynthesisBackend::Template);
    }

    #[test]
    fn test_synthesis_config_from_sidecar_settings() {
        let settings = SidecarSettings {
            enabled: true,
            synthesis_enabled: true,
            synthesis_backend: "openai".to_string(),
            ..Default::default()
        };

        let config = SynthesisConfig::from_sidecar_settings(&settings);
        assert!(config.enabled);
        assert_eq!(config.backend, SynthesisBackend::OpenAi);
    }

    // -------------------------------------------------------------------------
    // Factory tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_create_synthesizer_template() {
        let config = SynthesisConfig::default();
        let synthesizer = create_synthesizer(&config).unwrap();
        assert_eq!(synthesizer.backend_name(), "template");
    }

    #[test]
    fn test_create_synthesizer_openai_no_key() {
        let config = SynthesisConfig {
            backend: SynthesisBackend::OpenAi,
            openai: SynthesisOpenAiSettings {
                api_key: None,
                ..Default::default()
            },
            ..Default::default()
        };
        // Should fail if no API key is set
        // This test only passes if OPENAI_API_KEY env var is not set
        if std::env::var("OPENAI_API_KEY").is_err() {
            assert!(create_synthesizer(&config).is_err());
        }
    }
}

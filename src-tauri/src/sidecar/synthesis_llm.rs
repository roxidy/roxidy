//! LLM abstraction for synthesis operations.
//!
//! This module provides a trait-based abstraction over different LLM backends
//! for generating commit messages, summaries, and other synthesis tasks.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;

use super::config::{LlmProvider, SynthesisBackend};
use super::models::ModelManager;

/// Trait for LLM backends used in synthesis
#[async_trait]
pub trait SynthesisLlm: Send + Sync {
    /// Generate a response using chat format (system + user messages)
    async fn generate_chat(&self, system: &str, user: &str, max_tokens: usize) -> Result<String>;

    /// Check if the backend is available/ready
    fn is_available(&self) -> bool;

    /// Get a description of this backend for logging
    fn description(&self) -> String;
}

/// Local LLM backend using mistral.rs (Qwen, etc.)
pub struct LocalLlm {
    model_manager: Arc<ModelManager>,
}

impl LocalLlm {
    pub fn new(model_manager: Arc<ModelManager>) -> Self {
        Self { model_manager }
    }
}

#[async_trait]
impl SynthesisLlm for LocalLlm {
    async fn generate_chat(&self, system: &str, user: &str, max_tokens: usize) -> Result<String> {
        self.model_manager
            .generate_chat(system, user, max_tokens)
            .await
    }

    fn is_available(&self) -> bool {
        self.model_manager.llm_available()
    }

    fn description(&self) -> String {
        "Local LLM (Qwen via mistral.rs)".to_string()
    }
}

/// Remote LLM backend using various providers
pub struct RemoteLlm {
    provider: LlmProvider,
    client: RemoteClient,
}

/// Internal client enum for different remote providers
enum RemoteClient {
    VertexAnthropic(rig_anthropic_vertex::CompletionModel),
    // Future: OpenAI, Grok, etc.
}

impl RemoteLlm {
    /// Create a new remote LLM from provider configuration
    pub async fn from_provider(provider: LlmProvider) -> Result<Self> {
        let client = match &provider {
            LlmProvider::VertexAnthropic {
                project_id,
                location,
                model,
                credentials_path,
            } => {
                let creds_path = credentials_path.clone().unwrap_or_else(|| {
                    dirs::home_dir()
                        .unwrap_or_default()
                        .join(".config/gcloud/application_default_credentials.json")
                        .to_string_lossy()
                        .to_string()
                });

                let vertex_client =
                    rig_anthropic_vertex::Client::from_service_account(&creds_path, project_id, location)
                        .await
                        .context("Failed to create Vertex AI client")?;

                RemoteClient::VertexAnthropic(vertex_client.completion_model(model))
            }
            LlmProvider::OpenAI { .. } => {
                anyhow::bail!("OpenAI provider not yet implemented")
            }
            LlmProvider::Grok { .. } => {
                anyhow::bail!("Grok provider not yet implemented")
            }
            LlmProvider::OpenAICompatible { .. } => {
                anyhow::bail!("OpenAI-compatible provider not yet implemented")
            }
        };

        Ok(Self { provider, client })
    }
}

#[async_trait]
impl SynthesisLlm for RemoteLlm {
    async fn generate_chat(&self, system: &str, user: &str, max_tokens: usize) -> Result<String> {
        use rig::completion::{AssistantContent, CompletionModel, CompletionRequest, Message};
        use rig::message::{Text, UserContent};
        use rig::one_or_many::OneOrMany;

        match &self.client {
            RemoteClient::VertexAnthropic(model) => {
                let chat_history = vec![Message::User {
                    content: OneOrMany::one(UserContent::Text(Text {
                        text: user.to_string(),
                    })),
                }];

                let request = CompletionRequest {
                    preamble: Some(system.to_string()),
                    chat_history: OneOrMany::many(chat_history.clone())
                        .unwrap_or_else(|_| OneOrMany::one(chat_history[0].clone())),
                    documents: vec![],
                    tools: vec![],
                    temperature: Some(0.3),
                    max_tokens: Some(max_tokens as u64),
                    tool_choice: None,
                    additional_params: None,
                };

                let response = model
                    .completion(request)
                    .await
                    .context("Vertex AI completion request failed")?;

                // Extract text from the response
                let text = response
                    .choice
                    .iter()
                    .filter_map(|c| {
                        if let AssistantContent::Text(t) = c {
                            Some(t.text.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");

                Ok(text.trim().to_string())
            }
        }
    }

    fn is_available(&self) -> bool {
        // Remote is always "available" - actual availability checked on request
        true
    }

    fn description(&self) -> String {
        format!(
            "{} ({})",
            self.provider.provider_name(),
            self.provider.model_name()
        )
    }
}

/// Template-only backend (no LLM, always returns error to trigger fallback)
pub struct TemplateLlm;

#[async_trait]
impl SynthesisLlm for TemplateLlm {
    async fn generate_chat(&self, _system: &str, _user: &str, _max_tokens: usize) -> Result<String> {
        anyhow::bail!("Template mode - no LLM available")
    }

    fn is_available(&self) -> bool {
        false
    }

    fn description(&self) -> String {
        "Template-only (no LLM)".to_string()
    }
}

/// Create a synthesis LLM from backend configuration
pub async fn create_synthesis_llm(
    backend: &SynthesisBackend,
    model_manager: Arc<ModelManager>,
) -> Result<Arc<dyn SynthesisLlm>> {
    match backend {
        SynthesisBackend::Local => {
            tracing::info!("[synthesis-llm] Using local LLM backend");
            Ok(Arc::new(LocalLlm::new(model_manager)))
        }
        SynthesisBackend::Remote { provider } => {
            tracing::info!(
                "[synthesis-llm] Using remote LLM backend: {}",
                provider.provider_name()
            );
            let remote = RemoteLlm::from_provider(provider.clone()).await?;
            Ok(Arc::new(remote))
        }
        SynthesisBackend::Template => {
            tracing::info!("[synthesis-llm] Using template-only backend");
            Ok(Arc::new(TemplateLlm))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_llm_not_available() {
        let llm = TemplateLlm;
        assert!(!llm.is_available());
    }

    #[tokio::test]
    async fn test_template_llm_errors() {
        let llm = TemplateLlm;
        let result = llm.generate_chat("system", "user", 100).await;
        assert!(result.is_err());
    }
}

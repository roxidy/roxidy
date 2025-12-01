//! CompletionModel implementation for Anthropic on Vertex AI.

use rig::completion::{
    self, AssistantContent, CompletionError, CompletionRequest, CompletionResponse, Message,
    ToolDefinition, Usage,
};
use rig::one_or_many::OneOrMany;
use rig::streaming::{RawStreamingChoice, StreamingCompletionResponse};
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::streaming::StreamingResponse;
use crate::types::{self, ContentBlock, Role, ThinkingConfig, ANTHROPIC_VERSION, DEFAULT_MAX_TOKENS};

/// Default max tokens for different Claude models
fn default_max_tokens_for_model(model: &str) -> u32 {
    if model.contains("opus") {
        32000
    } else if model.contains("sonnet") {
        8192
    } else if model.contains("haiku") {
        8192
    } else {
        DEFAULT_MAX_TOKENS
    }
}

/// Completion model for Anthropic Claude on Vertex AI.
#[derive(Clone)]
pub struct CompletionModel {
    client: Client,
    model: String,
    /// Optional thinking configuration for extended reasoning
    thinking: Option<ThinkingConfig>,
}

impl CompletionModel {
    /// Create a new completion model.
    pub fn new(client: Client, model: String) -> Self {
        Self { client, model, thinking: None }
    }

    /// Enable extended thinking with the specified token budget.
    /// Note: When thinking is enabled, temperature is automatically set to 1.
    pub fn with_thinking(mut self, budget_tokens: u32) -> Self {
        self.thinking = Some(ThinkingConfig::new(budget_tokens));
        self
    }

    /// Enable extended thinking with default budget (10,000 tokens).
    pub fn with_default_thinking(mut self) -> Self {
        self.thinking = Some(ThinkingConfig::default_budget());
        self
    }

    /// Get the model identifier.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Convert rig's Message to Anthropic message format.
    fn convert_message(msg: &Message) -> types::Message {
        match msg {
            Message::User { content } => {
                let blocks: Vec<ContentBlock> = content
                    .iter()
                    .filter_map(|c| {
                        use rig::message::UserContent;
                        match c {
                            UserContent::Text(text) => Some(ContentBlock::Text {
                                text: text.text.clone(),
                            }),
                            UserContent::ToolResult(result) => Some(ContentBlock::ToolResult {
                                tool_use_id: result.id.clone(),
                                content: serde_json::to_string(&result.content)
                                    .unwrap_or_default(),
                                is_error: None,
                            }),
                            // Skip other content types that Anthropic doesn't support directly
                            _ => None,
                        }
                    })
                    .collect();

                types::Message {
                    role: Role::User,
                    content: if blocks.is_empty() {
                        vec![ContentBlock::Text {
                            text: String::new(),
                        }]
                    } else {
                        blocks
                    },
                }
            }
            Message::Assistant { content, .. } => {
                // When thinking is enabled, assistant messages must start with thinking blocks
                // Collect thinking blocks first, then other content
                let mut thinking_blocks: Vec<ContentBlock> = Vec::new();
                let mut other_blocks: Vec<ContentBlock> = Vec::new();

                for c in content.iter() {
                    match c {
                        AssistantContent::Text(text) => {
                            other_blocks.push(ContentBlock::Text {
                                text: text.text.clone(),
                            });
                        }
                        AssistantContent::ToolCall(tool_call) => {
                            // Ensure input is always a valid object (Anthropic API requirement)
                            let input = match &tool_call.function.arguments {
                                serde_json::Value::Object(_) => tool_call.function.arguments.clone(),
                                serde_json::Value::Null => serde_json::json!({}),
                                other => serde_json::json!({ "value": other }),
                            };
                            other_blocks.push(ContentBlock::ToolUse {
                                id: tool_call.id.clone(),
                                name: tool_call.function.name.clone(),
                                input,
                            });
                        }
                        AssistantContent::Reasoning(reasoning) => {
                            // Include thinking blocks for extended thinking mode
                            let thinking_text = reasoning.reasoning.join("");
                            if !thinking_text.is_empty() {
                                thinking_blocks.push(ContentBlock::Thinking {
                                    thinking: thinking_text,
                                    // Signature is required but we may not have it from history
                                    // Use empty string as placeholder (API may reject this)
                                    signature: reasoning.signature.clone().unwrap_or_default(),
                                });
                            }
                        }
                    }
                }

                // Combine: thinking blocks first (required by API), then other content
                let mut blocks = thinking_blocks;
                blocks.append(&mut other_blocks);

                types::Message {
                    role: Role::Assistant,
                    content: if blocks.is_empty() {
                        vec![ContentBlock::Text {
                            text: String::new(),
                        }]
                    } else {
                        blocks
                    },
                }
            }
        }
    }

    /// Convert rig's ToolDefinition to Anthropic format.
    fn convert_tool(tool: &ToolDefinition) -> types::ToolDefinition {
        types::ToolDefinition {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.parameters.clone(),
        }
    }

    /// Build an Anthropic request from a rig CompletionRequest.
    fn build_request(&self, request: &CompletionRequest, stream: bool) -> types::CompletionRequest {
        // Convert chat history to messages
        let mut messages: Vec<types::Message> = request
            .chat_history
            .iter()
            .map(Self::convert_message)
            .collect();

        // Add normalized documents as user messages
        for doc in &request.documents {
            messages.push(types::Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: format!("[Document: {}]\n{}", doc.id, doc.text),
                }],
            });
        }

        // Determine max tokens
        let mut max_tokens = request
            .max_tokens
            .map(|t| t as u32)
            .unwrap_or_else(|| default_max_tokens_for_model(&self.model));

        // When thinking is enabled, max_tokens must be greater than budget_tokens
        if let Some(ref thinking) = self.thinking {
            let min_required = thinking.budget_tokens + 1;
            if max_tokens <= thinking.budget_tokens {
                max_tokens = min_required.max(thinking.budget_tokens + 8192);
            }
        }

        // Convert tools
        let tools: Option<Vec<types::ToolDefinition>> = if request.tools.is_empty() {
            None
        } else {
            Some(request.tools.iter().map(Self::convert_tool).collect())
        };

        // When thinking is enabled, temperature must be 1
        let temperature = if self.thinking.is_some() {
            Some(1.0)
        } else {
            request.temperature.map(|t| t as f32)
        };

        types::CompletionRequest {
            anthropic_version: ANTHROPIC_VERSION.to_string(),
            messages,
            max_tokens,
            system: request.preamble.clone(),
            temperature,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            tools,
            stream: if stream { Some(true) } else { None },
            thinking: self.thinking.clone(),
        }
    }

    /// Convert Anthropic response to rig's CompletionResponse.
    fn convert_response(
        response: types::CompletionResponse,
    ) -> CompletionResponse<types::CompletionResponse> {
        use rig::message::{Text, ToolCall, ToolFunction};

        let choice: Vec<AssistantContent> = response
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(AssistantContent::Text(Text {
                    text: text.clone(),
                })),
                ContentBlock::ToolUse { id, name, input } => {
                    Some(AssistantContent::ToolCall(ToolCall {
                        id: id.clone(),
                        call_id: None,
                        function: ToolFunction {
                            name: name.clone(),
                            arguments: input.clone(),
                        },
                    }))
                }
                _ => None,
            })
            .collect();

        CompletionResponse {
            choice: OneOrMany::many(choice).unwrap_or_else(|_| OneOrMany::one(AssistantContent::Text(Text { text: String::new() }))),
            usage: Usage {
                input_tokens: response.usage.input_tokens as u64,
                output_tokens: response.usage.output_tokens as u64,
                total_tokens: (response.usage.input_tokens + response.usage.output_tokens) as u64,
            },
            raw_response: response,
        }
    }
}

/// Response type for streaming (wraps our streaming response)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamingCompletionResponseData {
    /// Accumulated text
    pub text: String,
    /// Token usage (filled at end)
    pub usage: Option<types::Usage>,
}

impl rig::completion::GetTokenUsage for StreamingCompletionResponseData {
    fn token_usage(&self) -> Option<Usage> {
        self.usage.as_ref().map(|u| Usage {
            input_tokens: u.input_tokens as u64,
            output_tokens: u.output_tokens as u64,
            total_tokens: (u.input_tokens + u.output_tokens) as u64,
        })
    }
}

impl completion::CompletionModel for CompletionModel {
    type Response = types::CompletionResponse;
    type StreamingResponse = StreamingCompletionResponseData;

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        let anthropic_request = self.build_request(&request, false);

        // Build URL for streamRawPredict (non-streaming uses rawPredict)
        let url = self.client.endpoint_url(&self.model, "rawPredict");

        // Get headers with auth
        let headers = self
            .client
            .build_headers()
            .await
            .map_err(|e| CompletionError::ProviderError(e.to_string()))?;

        // Make the request
        let response = self
            .client
            .http_client()
            .post(&url)
            .headers(headers)
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| CompletionError::RequestError(Box::new(e)))?;

        // Check for errors
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(CompletionError::ProviderError(format!(
                "API error ({}): {}",
                status, body
            )));
        }

        // Parse response
        let body = response
            .text()
            .await
            .map_err(|e| CompletionError::RequestError(Box::new(e)))?;

        let anthropic_response: types::CompletionResponse = serde_json::from_str(&body)?;

        Ok(Self::convert_response(anthropic_response))
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        let anthropic_request = self.build_request(&request, true);

        // Log request details
        tracing::info!("stream(): Building request with thinking={:?}", anthropic_request.thinking.as_ref().map(|t| t.budget_tokens));
        tracing::debug!("stream(): max_tokens={}, messages={}", anthropic_request.max_tokens, anthropic_request.messages.len());

        // Build URL for streamRawPredict
        let url = self.client.endpoint_url(&self.model, "streamRawPredict");
        tracing::info!("stream(): POST {}", url);

        // Get headers with auth
        let headers = self
            .client
            .build_headers()
            .await
            .map_err(|e| CompletionError::ProviderError(e.to_string()))?;

        // Make the request
        let response = self
            .client
            .http_client()
            .post(&url)
            .headers(headers)
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("stream(): Request failed: {}", e);
                CompletionError::RequestError(Box::new(e))
            })?;

        let status = response.status();
        tracing::info!("stream(): Response status: {}", status);

        // Check for errors
        if !status.is_success() {
            let status_code = status.as_u16();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("stream(): API error ({}): {}", status_code, body);
            return Err(CompletionError::ProviderError(format!(
                "API error ({}): {}",
                status_code, body
            )));
        }

        // Create streaming response
        tracing::info!("stream(): Creating streaming response wrapper, status={}", status);
        let stream = StreamingResponse::new(response);

        // Convert to rig's streaming format
        use futures::StreamExt;

        let mapped_stream = stream.map(|chunk_result| {
            use crate::streaming::StreamChunk;

            chunk_result
                .map(|chunk| {
                    let raw_choice = match chunk {
                        StreamChunk::TextDelta { text, .. } => {
                            tracing::debug!("map_to_raw: TextDelta -> Message len={}", text.len());
                            RawStreamingChoice::Message(text)
                        }
                        StreamChunk::ToolUseStart { id, name } => {
                            tracing::info!("map_to_raw: ToolUseStart -> ToolCall name={}", name);
                            RawStreamingChoice::ToolCall {
                                id: id.clone(),
                                call_id: Some(id),
                                name,
                                arguments: serde_json::json!({}), // Must be a valid object
                            }
                        }
                        StreamChunk::ToolInputDelta { partial_json } => {
                            tracing::debug!("map_to_raw: ToolInputDelta -> ToolCallDelta len={}", partial_json.len());
                            RawStreamingChoice::ToolCallDelta {
                                id: String::new(),
                                delta: partial_json,
                            }
                        }
                        StreamChunk::Done { usage, .. } => {
                            tracing::info!("map_to_raw: Done -> FinalResponse usage={:?}", usage);
                            // Return final response with usage info
                            RawStreamingChoice::FinalResponse(StreamingCompletionResponseData {
                                text: String::new(),
                                usage,
                            })
                        }
                        StreamChunk::Error { message } => {
                            tracing::error!("map_to_raw: Error -> Message error={}", message);
                            // Can't return error directly, emit as message
                            RawStreamingChoice::Message(format!("[Error: {}]", message))
                        }
                        StreamChunk::ThinkingDelta { thinking } => {
                            tracing::debug!("map_to_raw: ThinkingDelta -> Reasoning len={}", thinking.len());
                            // Emit thinking content using native reasoning type
                            RawStreamingChoice::Reasoning {
                                id: None,
                                reasoning: thinking,
                                signature: None,
                            }
                        }
                        StreamChunk::ThinkingSignature { signature } => {
                            tracing::info!("map_to_raw: ThinkingSignature -> Reasoning with signature len={}", signature.len());
                            // Emit signature as a Reasoning event (empty reasoning, signature set)
                            RawStreamingChoice::Reasoning {
                                id: None,
                                reasoning: String::new(),
                                signature: Some(signature),
                            }
                        }
                    };
                    raw_choice
                })
                .map_err(|e| {
                    tracing::error!("map_to_raw: chunk error: {}", e);
                    CompletionError::ProviderError(e.to_string())
                })
        });

        tracing::info!("Returning StreamingCompletionResponse");
        Ok(StreamingCompletionResponse::stream(Box::pin(mapped_stream)))
    }
}

impl std::fmt::Debug for CompletionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletionModel")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

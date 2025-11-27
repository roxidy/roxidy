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
use crate::types::{self, ContentBlock, Role, ANTHROPIC_VERSION, DEFAULT_MAX_TOKENS};

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
}

impl CompletionModel {
    /// Create a new completion model.
    pub fn new(client: Client, model: String) -> Self {
        Self { client, model }
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
                let blocks: Vec<ContentBlock> = content
                    .iter()
                    .filter_map(|c| match c {
                        AssistantContent::Text(text) => Some(ContentBlock::Text {
                            text: text.text.clone(),
                        }),
                        AssistantContent::ToolCall(tool_call) => Some(ContentBlock::ToolUse {
                            id: tool_call.id.clone(),
                            name: tool_call.function.name.clone(),
                            input: tool_call.function.arguments.clone(),
                        }),
                        // Skip reasoning and image content
                        _ => None,
                    })
                    .collect();

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
        let max_tokens = request
            .max_tokens
            .map(|t| t as u32)
            .unwrap_or_else(|| default_max_tokens_for_model(&self.model));

        // Convert tools
        let tools: Option<Vec<types::ToolDefinition>> = if request.tools.is_empty() {
            None
        } else {
            Some(request.tools.iter().map(Self::convert_tool).collect())
        };

        types::CompletionRequest {
            anthropic_version: ANTHROPIC_VERSION.to_string(),
            messages,
            max_tokens,
            system: request.preamble.clone(),
            temperature: request.temperature.map(|t| t as f32),
            top_p: None,
            top_k: None,
            stop_sequences: None,
            tools,
            stream: if stream { Some(true) } else { None },
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

        // Build URL for streamRawPredict
        let url = self.client.endpoint_url(&self.model, "streamRawPredict");

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

        // Create streaming response
        let stream = StreamingResponse::new(response);

        // Convert to rig's streaming format
        use futures::StreamExt;

        let mapped_stream = stream.map(|chunk_result| {
            use crate::streaming::StreamChunk;

            chunk_result
                .map(|chunk| match chunk {
                    StreamChunk::TextDelta { text, .. } => {
                        RawStreamingChoice::Message(text)
                    }
                    StreamChunk::ToolUseStart { id, name } => {
                        RawStreamingChoice::ToolCall {
                            id: id.clone(),
                            call_id: Some(id),
                            name,
                            arguments: serde_json::Value::Null,
                        }
                    }
                    StreamChunk::ToolInputDelta { partial_json } => {
                        RawStreamingChoice::ToolCallDelta {
                            id: String::new(),
                            delta: partial_json,
                        }
                    }
                    StreamChunk::Done { usage, .. } => {
                        // Return final response with usage info
                        RawStreamingChoice::FinalResponse(StreamingCompletionResponseData {
                            text: String::new(),
                            usage,
                        })
                    }
                    StreamChunk::Error { message } => {
                        // Can't return error directly, emit as message
                        RawStreamingChoice::Message(format!("[Error: {}]", message))
                    }
                })
                .map_err(|e| CompletionError::ProviderError(e.to_string()))
        });

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

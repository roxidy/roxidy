//! Streaming response handling for Anthropic Vertex AI.

use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::error::AnthropicVertexError;
use crate::types::{ContentDelta, StreamEvent, Usage};

/// A streaming response from the Anthropic Vertex AI API.
pub struct StreamingResponse {
    /// The underlying byte stream
    inner: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    /// Buffer for incomplete SSE data
    buffer: String,
    /// Accumulated text content
    accumulated_text: String,
    /// Whether the stream has completed
    done: bool,
}

impl StreamingResponse {
    /// Create a new streaming response from a reqwest response.
    pub fn new(response: reqwest::Response) -> Self {
        Self {
            inner: Box::pin(response.bytes_stream()),
            buffer: String::new(),
            accumulated_text: String::new(),
            done: false,
        }
    }

    /// Get the accumulated text so far.
    pub fn accumulated_text(&self) -> &str {
        &self.accumulated_text
    }

    /// Parse an SSE line into a stream event.
    fn parse_sse_line(line: &str) -> Option<Result<StreamEvent, AnthropicVertexError>> {
        // SSE format: "data: {...}\n\n" or "event: ...\ndata: {...}\n\n"
        let line = line.trim();

        if line.is_empty() || line.starts_with(':') {
            return None;
        }

        if let Some(data) = line.strip_prefix("data: ") {
            // Skip [DONE] message
            if data == "[DONE]" {
                return None;
            }

            match serde_json::from_str::<StreamEvent>(data) {
                Ok(event) => Some(Ok(event)),
                Err(e) => Some(Err(AnthropicVertexError::ParseError(format!(
                    "Failed to parse stream event: {} - data: {}",
                    e, data
                )))),
            }
        } else {
            None
        }
    }
}

/// A chunk from the streaming response.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text delta
    TextDelta {
        text: String,
        accumulated: String,
    },
    /// Thinking/reasoning delta (extended thinking mode)
    ThinkingDelta {
        thinking: String,
    },
    /// Tool use started
    ToolUseStart {
        id: String,
        name: String,
    },
    /// Tool input delta
    ToolInputDelta {
        partial_json: String,
    },
    /// Stream completed
    Done {
        stop_reason: Option<String>,
        usage: Option<Usage>,
    },
    /// Error occurred
    Error {
        message: String,
    },
}

impl Stream for StreamingResponse {
    type Item = Result<StreamChunk, AnthropicVertexError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }

        loop {
            // Check if we have complete lines in the buffer
            if let Some(newline_pos) = self.buffer.find("\n\n") {
                let line = self.buffer[..newline_pos].to_string();
                self.buffer = self.buffer[newline_pos + 2..].to_string();

                if let Some(result) = Self::parse_sse_line(&line) {
                    match result {
                        Ok(event) => {
                            let chunk = self.event_to_chunk(event);
                            if let Some(chunk) = chunk {
                                return Poll::Ready(Some(Ok(chunk)));
                            }
                            // Continue processing if we got a non-yielding event
                            continue;
                        }
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                }
                continue;
            }

            // Need more data from the stream
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        self.buffer.push_str(text);
                    }
                    // Continue to process the buffer
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(AnthropicVertexError::StreamError(
                        e.to_string(),
                    ))));
                }
                Poll::Ready(None) => {
                    self.done = true;
                    // Process any remaining buffer
                    if !self.buffer.is_empty() {
                        if let Some(result) = Self::parse_sse_line(&self.buffer) {
                            self.buffer.clear();
                            match result {
                                Ok(event) => {
                                    if let Some(chunk) = self.event_to_chunk(event) {
                                        return Poll::Ready(Some(Ok(chunk)));
                                    }
                                }
                                Err(e) => return Poll::Ready(Some(Err(e))),
                            }
                        }
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl StreamingResponse {
    /// Convert a stream event to a stream chunk.
    fn event_to_chunk(&mut self, event: StreamEvent) -> Option<StreamChunk> {
        match event {
            StreamEvent::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::TextDelta { text } => {
                    self.accumulated_text.push_str(&text);
                    Some(StreamChunk::TextDelta {
                        text,
                        accumulated: self.accumulated_text.clone(),
                    })
                }
                ContentDelta::InputJsonDelta { partial_json } => {
                    Some(StreamChunk::ToolInputDelta { partial_json })
                }
                ContentDelta::ThinkingDelta { thinking } => {
                    Some(StreamChunk::ThinkingDelta { thinking })
                }
                ContentDelta::SignatureDelta { .. } => {
                    // Signature deltas are for verification, not displayed
                    None
                }
            },
            StreamEvent::ContentBlockStart { content_block, .. } => {
                match content_block {
                    crate::types::ContentBlock::ToolUse { id, name, .. } => {
                        Some(StreamChunk::ToolUseStart { id, name })
                    }
                    _ => None, // Text blocks don't need special handling at start
                }
            }
            StreamEvent::MessageDelta { delta, usage } => {
                self.done = true;
                Some(StreamChunk::Done {
                    stop_reason: delta.stop_reason.map(|r| format!("{:?}", r)),
                    usage: Some(usage),
                })
            }
            StreamEvent::MessageStop => {
                self.done = true;
                Some(StreamChunk::Done {
                    stop_reason: None,
                    usage: None,
                })
            }
            StreamEvent::Error { error } => Some(StreamChunk::Error {
                message: error.message,
            }),
            _ => None, // Ping, MessageStart, ContentBlockStop don't produce chunks
        }
    }
}

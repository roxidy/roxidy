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
    /// Accumulated thinking signature (for extended thinking)
    accumulated_signature: String,
    /// Whether the stream has completed
    done: bool,
}

impl StreamingResponse {
    /// Create a new streaming response from a reqwest response.
    pub fn new(response: reqwest::Response) -> Self {
        tracing::info!("StreamingResponse::new - creating stream from response");
        tracing::debug!("StreamingResponse::new - content-type: {:?}", response.headers().get("content-type"));
        tracing::debug!("StreamingResponse::new - content-length: {:?}", response.headers().get("content-length"));
        Self {
            inner: Box::pin(response.bytes_stream()),
            buffer: String::new(),
            accumulated_text: String::new(),
            accumulated_signature: String::new(),
            done: false,
        }
    }

    /// Get the accumulated text so far.
    pub fn accumulated_text(&self) -> &str {
        &self.accumulated_text
    }

    /// Parse an SSE line into a stream event.
    fn parse_sse_line(line: &str) -> Option<Result<StreamEvent, AnthropicVertexError>> {
        // SSE format: "event: ...\ndata: {...}" or just "data: {...}"
        // We need to extract the data portion from the message
        let line = line.trim();

        if line.is_empty() || line.starts_with(':') {
            return None;
        }

        // Find the data line - it might be after an event line
        let data_content = if let Some(data_start) = line.find("data: ") {
            let data_part = &line[data_start + 6..]; // Skip "data: "
            // Trim any trailing whitespace
            data_part.trim()
        } else {
            tracing::trace!("SSE: No data field found in: {}", &line[..line.len().min(100)]);
            return None;
        };

        // Skip [DONE] message
        if data_content == "[DONE]" {
            tracing::debug!("SSE: Received [DONE] marker");
            return None;
        }

        match serde_json::from_str::<StreamEvent>(data_content) {
            Ok(event) => {
                tracing::debug!("SSE: Parsed event type: {:?}", std::mem::discriminant(&event));
                Some(Ok(event))
            }
            Err(e) => {
                tracing::warn!("SSE: Failed to parse event: {} - data: {}", e, &data_content[..data_content.len().min(200)]);
                Some(Err(AnthropicVertexError::ParseError(format!(
                    "Failed to parse stream event: {} - data: {}",
                    e, data_content
                ))))
            }
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
    /// Thinking signature (emitted when signature is complete)
    ThinkingSignature {
        signature: String,
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
            tracing::trace!("poll_next: already done");
            return Poll::Ready(None);
        }

        loop {
            // Check if we have complete lines in the buffer
            if let Some(newline_pos) = self.buffer.find("\n\n") {
                let line = self.buffer[..newline_pos].to_string();
                self.buffer = self.buffer[newline_pos + 2..].to_string();
                tracing::trace!("poll_next: found SSE line, {} chars remaining in buffer", self.buffer.len());

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
                    let bytes_len = bytes.len();
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        self.buffer.push_str(text);
                        tracing::debug!("poll_next: received {} bytes, buffer now {} chars", bytes_len, self.buffer.len());
                        // Log first 200 chars of buffer for debugging
                        if self.buffer.len() < 500 {
                            tracing::debug!("poll_next: buffer content: {:?}", self.buffer);
                        }
                    } else {
                        tracing::warn!("poll_next: received {} bytes but not valid UTF-8", bytes_len);
                    }
                    // Continue to process the buffer
                }
                Poll::Ready(Some(Err(e))) => {
                    tracing::error!("poll_next: stream error: {}", e);
                    return Poll::Ready(Some(Err(AnthropicVertexError::StreamError(
                        e.to_string(),
                    ))));
                }
                Poll::Ready(None) => {
                    tracing::info!("poll_next: stream ended, buffer has {} chars remaining", self.buffer.len());
                    if !self.buffer.is_empty() {
                        tracing::debug!("poll_next: remaining buffer: {:?}", &self.buffer[..self.buffer.len().min(500)]);
                    }
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
        let chunk = match event {
            StreamEvent::ContentBlockDelta { delta, index } => match delta {
                ContentDelta::TextDelta { text } => {
                    tracing::debug!("event_to_chunk: TextDelta index={} len={}", index, text.len());
                    self.accumulated_text.push_str(&text);
                    Some(StreamChunk::TextDelta {
                        text,
                        accumulated: self.accumulated_text.clone(),
                    })
                }
                ContentDelta::InputJsonDelta { partial_json } => {
                    tracing::debug!("event_to_chunk: InputJsonDelta index={} len={}", index, partial_json.len());
                    Some(StreamChunk::ToolInputDelta { partial_json })
                }
                ContentDelta::ThinkingDelta { thinking } => {
                    tracing::debug!("event_to_chunk: ThinkingDelta index={} len={}", index, thinking.len());
                    Some(StreamChunk::ThinkingDelta { thinking })
                }
                ContentDelta::SignatureDelta { signature } => {
                    tracing::debug!("event_to_chunk: SignatureDelta index={} len={}", index, signature.len());
                    // Accumulate signature for later emission
                    self.accumulated_signature.push_str(&signature);
                    None
                }
            },
            StreamEvent::ContentBlockStart { content_block, index } => {
                match content_block {
                    crate::types::ContentBlock::ToolUse { id, name, .. } => {
                        tracing::info!("event_to_chunk: ToolUseStart index={} name={}", index, name);
                        Some(StreamChunk::ToolUseStart { id, name })
                    }
                    crate::types::ContentBlock::Thinking { .. } => {
                        tracing::debug!("event_to_chunk: Thinking block start index={}", index);
                        None // Thinking content comes via ThinkingDelta
                    }
                    _ => {
                        tracing::debug!("event_to_chunk: ContentBlockStart index={} (text, skipped)", index);
                        None // Text blocks don't need special handling at start
                    }
                }
            }
            StreamEvent::MessageDelta { delta, usage } => {
                tracing::info!("event_to_chunk: MessageDelta stop_reason={:?} usage={:?}", delta.stop_reason, usage);
                self.done = true;
                Some(StreamChunk::Done {
                    stop_reason: delta.stop_reason.map(|r| format!("{:?}", r)),
                    usage: Some(usage),
                })
            }
            StreamEvent::MessageStop => {
                tracing::info!("event_to_chunk: MessageStop");
                self.done = true;
                Some(StreamChunk::Done {
                    stop_reason: None,
                    usage: None,
                })
            }
            StreamEvent::Error { error } => {
                tracing::error!("event_to_chunk: Error type={} message={}", error.error_type, error.message);
                Some(StreamChunk::Error {
                    message: error.message,
                })
            }
            StreamEvent::MessageStart { .. } => {
                tracing::debug!("event_to_chunk: MessageStart (skipped)");
                None
            }
            StreamEvent::ContentBlockStop { index } => {
                tracing::debug!("event_to_chunk: ContentBlockStop index={}", index);
                // If we have an accumulated signature, emit it now (thinking block ended)
                if !self.accumulated_signature.is_empty() {
                    let signature = std::mem::take(&mut self.accumulated_signature);
                    tracing::info!("event_to_chunk: Emitting ThinkingSignature len={}", signature.len());
                    Some(StreamChunk::ThinkingSignature { signature })
                } else {
                    None
                }
            }
            StreamEvent::Ping => {
                tracing::trace!("event_to_chunk: Ping (skipped)");
                None
            }
        };
        chunk
    }
}

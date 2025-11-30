//! Token truncation strategies for large content
//!
//! Implements head+tail preservation strategies based on VTCode's design.

use super::token_budget::TokenBudgetManager;

/// Ratio of content to preserve from head vs tail for code content
const CODE_HEAD_RATIO: f64 = 0.7;
/// Ratio of content to preserve from head vs tail for log/text content
const LOG_HEAD_RATIO: f64 = 0.4;
/// Minimum content length before truncation kicks in
const MIN_TRUNCATION_LENGTH: usize = 100;
/// Byte fuse limit for safety truncation
const BYTE_FUSE_LIMIT: usize = 100_000;

/// Type of content for truncation strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    /// Code-like content (preserve more from beginning)
    Code,
    /// Log/output content (preserve more from end)
    Log,
    /// General text
    Text,
}

impl ContentType {
    /// Detect content type heuristically
    pub fn detect(content: &str) -> Self {
        // Count code-like characters
        let code_chars = content
            .chars()
            .filter(|c| matches!(c, '{' | '}' | '[' | ']' | '(' | ')' | ';' | ':'))
            .count();

        let total_chars = content.len();
        let code_ratio = code_chars as f64 / total_chars.max(1) as f64;

        // Check for log-like patterns
        let has_timestamps = content.contains("20") && content.contains(":");
        let has_log_levels = content.contains("INFO")
            || content.contains("WARN")
            || content.contains("ERROR")
            || content.contains("DEBUG");

        if has_timestamps && has_log_levels {
            ContentType::Log
        } else if code_ratio > 0.02 {
            ContentType::Code
        } else {
            ContentType::Text
        }
    }

    /// Get head ratio for this content type
    pub fn head_ratio(&self) -> f64 {
        match self {
            ContentType::Code => CODE_HEAD_RATIO,
            ContentType::Log => LOG_HEAD_RATIO,
            ContentType::Text => 0.5,
        }
    }
}

/// Result of a truncation operation
#[derive(Debug, Clone)]
pub struct TruncationResult {
    /// The truncated content
    pub content: String,
    /// Whether truncation was applied
    pub truncated: bool,
    /// Original length in characters
    pub original_chars: usize,
    /// Resulting length in characters
    pub result_chars: usize,
    /// Number of lines removed
    pub lines_removed: usize,
    /// Estimated tokens saved
    pub tokens_saved: usize,
}

/// Truncate content by token limit using head+tail strategy
pub fn truncate_by_tokens(content: &str, max_tokens: usize) -> TruncationResult {
    let original_chars = content.len();
    let original_tokens = TokenBudgetManager::estimate_tokens(content);

    if original_tokens <= max_tokens || original_chars < MIN_TRUNCATION_LENGTH {
        return TruncationResult {
            content: content.to_string(),
            truncated: false,
            original_chars,
            result_chars: original_chars,
            lines_removed: 0,
            tokens_saved: 0,
        };
    }

    let content_type = ContentType::detect(content);
    let head_ratio = content_type.head_ratio();

    // Calculate target character count based on token estimate
    let target_tokens = max_tokens;
    let ratio = target_tokens as f64 / original_tokens as f64;
    let target_chars = (original_chars as f64 * ratio) as usize;

    let head_chars = (target_chars as f64 * head_ratio) as usize;
    let tail_chars = target_chars.saturating_sub(head_chars);

    truncate_head_tail(content, head_chars, tail_chars)
}

/// Truncate content by character limit using head+tail strategy
pub fn truncate_by_chars(content: &str, max_chars: usize) -> TruncationResult {
    let original_chars = content.len();

    if original_chars <= max_chars || original_chars < MIN_TRUNCATION_LENGTH {
        return TruncationResult {
            content: content.to_string(),
            truncated: false,
            original_chars,
            result_chars: original_chars,
            lines_removed: 0,
            tokens_saved: 0,
        };
    }

    let content_type = ContentType::detect(content);
    let head_ratio = content_type.head_ratio();

    let head_chars = (max_chars as f64 * head_ratio) as usize;
    let tail_chars = max_chars.saturating_sub(head_chars);

    truncate_head_tail(content, head_chars, tail_chars)
}

/// Core truncation logic preserving head and tail
fn truncate_head_tail(content: &str, head_chars: usize, tail_chars: usize) -> TruncationResult {
    let original_chars = content.len();
    let original_lines: Vec<&str> = content.lines().collect();

    // Find safe UTF-8 boundaries
    let head_end = find_char_boundary(content, head_chars);
    let tail_start = find_char_boundary_reverse(content, tail_chars);

    // Avoid overlap
    if head_end >= tail_start {
        // Content is small enough, just truncate from end
        let safe_end = find_char_boundary(content, head_chars + tail_chars);
        let truncated = &content[..safe_end];
        return TruncationResult {
            content: truncated.to_string(),
            truncated: true,
            original_chars,
            result_chars: truncated.len(),
            lines_removed: original_lines
                .len()
                .saturating_sub(truncated.lines().count()),
            tokens_saved: TokenBudgetManager::estimate_tokens(content)
                .saturating_sub(TokenBudgetManager::estimate_tokens(truncated)),
        };
    }

    let head = &content[..head_end];
    let tail = &content[tail_start..];

    // Count removed lines
    let middle = &content[head_end..tail_start];
    let middle_lines = middle.lines().count();

    // Build result with marker
    let result = format!(
        "{}\n\n[... {} lines truncated ...]\n\n{}",
        head.trim_end(),
        middle_lines,
        tail.trim_start()
    );

    TruncationResult {
        content: result.clone(),
        truncated: true,
        original_chars,
        result_chars: result.len(),
        lines_removed: middle_lines,
        tokens_saved: TokenBudgetManager::estimate_tokens(content)
            .saturating_sub(TokenBudgetManager::estimate_tokens(&result)),
    }
}

/// Find a safe char boundary at or before the given byte index
fn find_char_boundary(s: &str, byte_index: usize) -> usize {
    if byte_index >= s.len() {
        return s.len();
    }

    let mut idx = byte_index.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Find a safe char boundary from the end
fn find_char_boundary_reverse(s: &str, bytes_from_end: usize) -> usize {
    if bytes_from_end >= s.len() {
        return 0;
    }

    let target = s.len().saturating_sub(bytes_from_end);
    let mut idx = target;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

/// Safe truncation to byte limit with UTF-8 awareness
pub fn safe_truncate_to_bytes(content: &str, max_bytes: usize) -> TruncationResult {
    let original_chars = content.len();

    if content.len() <= max_bytes {
        return TruncationResult {
            content: content.to_string(),
            truncated: false,
            original_chars,
            result_chars: original_chars,
            lines_removed: 0,
            tokens_saved: 0,
        };
    }

    let safe_end = find_char_boundary(content, max_bytes.saturating_sub(50));
    let truncated = format!(
        "{}\n[... content truncated by byte fuse ...]",
        &content[..safe_end]
    );

    TruncationResult {
        content: truncated.clone(),
        truncated: true,
        original_chars,
        result_chars: truncated.len(),
        lines_removed: content[safe_end..].lines().count(),
        tokens_saved: TokenBudgetManager::estimate_tokens(content)
            .saturating_sub(TokenBudgetManager::estimate_tokens(&truncated)),
    }
}

/// Aggregate and truncate tool output for model consumption
pub fn aggregate_tool_output(output: &str, max_tokens: usize) -> TruncationResult {
    // First apply token-based truncation
    let result = truncate_by_tokens(output, max_tokens);

    // Then apply byte fuse as safety limit
    if result.content.len() > BYTE_FUSE_LIMIT {
        safe_truncate_to_bytes(&result.content, BYTE_FUSE_LIMIT)
    } else {
        result
    }
}

/// Truncate JSON output, trying to preserve structure
pub fn truncate_json_output(json: &str, max_tokens: usize) -> TruncationResult {
    let original_tokens = TokenBudgetManager::estimate_tokens(json);

    if original_tokens <= max_tokens {
        return TruncationResult {
            content: json.to_string(),
            truncated: false,
            original_chars: json.len(),
            result_chars: json.len(),
            lines_removed: 0,
            tokens_saved: 0,
        };
    }

    // For JSON, try to parse and summarize if possible
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
        let summary = summarize_json_value(&value, 3); // Max depth 3
        let summary_str =
            serde_json::to_string_pretty(&summary).unwrap_or_else(|_| json.to_string());

        if TokenBudgetManager::estimate_tokens(&summary_str) <= max_tokens {
            return TruncationResult {
                content: summary_str.clone(),
                truncated: true,
                original_chars: json.len(),
                result_chars: summary_str.len(),
                lines_removed: 0,
                tokens_saved: original_tokens
                    .saturating_sub(TokenBudgetManager::estimate_tokens(&summary_str)),
            };
        }
    }

    // Fallback to standard truncation
    truncate_by_tokens(json, max_tokens)
}

/// Summarize a JSON value to reduce size
fn summarize_json_value(value: &serde_json::Value, max_depth: usize) -> serde_json::Value {
    if max_depth == 0 {
        return match value {
            serde_json::Value::Array(arr) => {
                serde_json::Value::String(format!("[... {} items ...]", arr.len()))
            }
            serde_json::Value::Object(obj) => {
                serde_json::Value::String(format!("{{... {} keys ...}}", obj.len()))
            }
            _ => value.clone(),
        };
    }

    match value {
        serde_json::Value::Array(arr) => {
            if arr.len() > 5 {
                let mut summarized: Vec<serde_json::Value> = arr
                    .iter()
                    .take(3)
                    .map(|v| summarize_json_value(v, max_depth - 1))
                    .collect();
                summarized.push(serde_json::Value::String(format!(
                    "... {} more items ...",
                    arr.len() - 3
                )));
                serde_json::Value::Array(summarized)
            } else {
                serde_json::Value::Array(
                    arr.iter()
                        .map(|v| summarize_json_value(v, max_depth - 1))
                        .collect(),
                )
            }
        }
        serde_json::Value::Object(obj) => {
            let mut summarized = serde_json::Map::new();
            for (i, (k, v)) in obj.iter().enumerate() {
                if i >= 10 {
                    summarized.insert(
                        "...".to_string(),
                        serde_json::Value::String(format!("{} more keys", obj.len() - 10)),
                    );
                    break;
                }
                summarized.insert(k.clone(), summarize_json_value(v, max_depth - 1));
            }
            serde_json::Value::Object(summarized)
        }
        serde_json::Value::String(s) if s.len() > 200 => {
            serde_json::Value::String(format!("{}... [truncated]", &s[..200]))
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_detection() {
        let code = "fn main() { let x = 5; }";
        let log = "2024-01-01 10:00:00 INFO Starting server";
        let text = "This is just regular text without special markers.";

        assert_eq!(ContentType::detect(code), ContentType::Code);
        assert_eq!(ContentType::detect(log), ContentType::Log);
        assert_eq!(ContentType::detect(text), ContentType::Text);
    }

    #[test]
    fn test_no_truncation_under_limit() {
        let content = "Short content";
        let result = truncate_by_tokens(content, 1000);
        assert!(!result.truncated);
        assert_eq!(result.content, content);
    }

    #[test]
    fn test_truncation_preserves_boundaries() {
        // Create a long enough content to exceed MIN_TRUNCATION_LENGTH (100)
        let content = "Line 1: This is some content\nLine 2: More content here\nLine 3: Even more content\nLine 4: Additional text\nLine 5: Keep going\nLine 6: Still more\nLine 7: And more\nLine 8: Almost done\nLine 9: Nearly there\nLine 10: The end";
        // Content is ~210 chars, truncate to 120 should trigger truncation
        let result = truncate_by_chars(content, 120);
        assert!(result.truncated);
        assert!(result.content.len() < content.len());
    }

    #[test]
    fn test_utf8_safety() {
        let content = "Hello 世界! This is a test with unicode: 🎉🎊🎁";
        let result = safe_truncate_to_bytes(content, 20);
        // Should not panic and should be valid UTF-8
        assert!(result.content.is_ascii() || result.content.chars().count() > 0);
    }

    #[test]
    fn test_json_summarization() {
        let json = serde_json::json!({
            "items": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            "nested": {
                "deep": {
                    "value": "test"
                }
            }
        });

        let summarized = summarize_json_value(&json, 2);
        // Array should be truncated
        if let serde_json::Value::Object(obj) = &summarized {
            if let Some(serde_json::Value::Array(arr)) = obj.get("items") {
                assert!(arr.len() <= 5);
            }
        }
    }
}

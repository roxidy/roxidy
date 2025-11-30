//! Loop detection and protection for the AI agent system.
//!
//! This module provides mechanisms to detect and prevent infinite loops
//! and runaway agent behavior by tracking:
//! - Total turn count per request
//! - Tool calls per turn (inner loops)
//! - Repeated identical tool calls with the same arguments
//!
//! Based on vtcode's loop protection implementation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for loop protection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopProtectionConfig {
    /// Maximum number of tool call iterations per turn.
    /// Prevents infinite tool-calling cycles within a single agent turn.
    /// Default: 100
    #[serde(default = "default_max_tool_loops")]
    pub max_tool_loops: usize,

    /// Maximum number of times the same tool can be called with identical
    /// arguments within a single turn. Helps detect stuck agents.
    /// Default: 5
    #[serde(default = "default_max_repeated_tool_calls")]
    pub max_repeated_tool_calls: usize,

    /// Threshold at which to warn the user about potential loops.
    /// When repeated calls reach this percentage of max, a warning is emitted.
    /// Default: 0.6 (60%)
    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f64,

    /// Whether loop detection is enabled.
    /// Default: true
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_max_tool_loops() -> usize {
    100
}

fn default_max_repeated_tool_calls() -> usize {
    5
}

fn default_warning_threshold() -> f64 {
    0.6
}

fn default_enabled() -> bool {
    true
}

impl Default for LoopProtectionConfig {
    fn default() -> Self {
        Self {
            max_tool_loops: default_max_tool_loops(),
            max_repeated_tool_calls: default_max_repeated_tool_calls(),
            warning_threshold: default_warning_threshold(),
            enabled: default_enabled(),
        }
    }
}

/// Response from loop detection check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopDetectionResult {
    /// Tool call is allowed to proceed.
    Allowed,

    /// Warning: approaching the repeat threshold.
    Warning {
        /// The tool that's being repeated.
        tool_name: String,
        /// Current repeat count.
        current_count: usize,
        /// Maximum allowed repeats.
        max_count: usize,
        /// Message to display.
        message: String,
    },

    /// Blocked: repeat threshold exceeded.
    Blocked {
        /// The tool that was blocked.
        tool_name: String,
        /// How many times it was called with identical args.
        repeat_count: usize,
        /// Maximum allowed repeats.
        max_count: usize,
        /// Message to display.
        message: String,
    },

    /// Blocked: maximum tool iterations reached for this turn.
    MaxIterationsReached {
        /// Current iteration count.
        iterations: usize,
        /// Maximum allowed iterations.
        max_iterations: usize,
        /// Message to display.
        message: String,
    },
}

impl LoopDetectionResult {
    /// Returns true if the tool call is allowed to proceed.
    #[allow(dead_code)]
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            LoopDetectionResult::Allowed | LoopDetectionResult::Warning { .. }
        )
    }

    /// Returns true if the tool call should be blocked.
    #[allow(dead_code)]
    pub fn is_blocked(&self) -> bool {
        matches!(
            self,
            LoopDetectionResult::Blocked { .. } | LoopDetectionResult::MaxIterationsReached { .. }
        )
    }

    /// Returns a message describing the result.
    #[allow(dead_code)]
    pub fn message(&self) -> Option<&str> {
        match self {
            LoopDetectionResult::Allowed => None,
            LoopDetectionResult::Warning { message, .. } => Some(message),
            LoopDetectionResult::Blocked { message, .. } => Some(message),
            LoopDetectionResult::MaxIterationsReached { message, .. } => Some(message),
        }
    }
}

/// Creates a signature for a tool call based on name and arguments.
/// Used to detect repeated identical calls.
fn make_signature(tool_name: &str, args: &serde_json::Value) -> String {
    // Serialize args in a canonical way (sorted keys)
    let args_str = serde_json::to_string(args).unwrap_or_default();
    format!("{}:{}", tool_name, args_str)
}

/// Detects potential model loops from repetitive tool calls.
///
/// Tracks tool call signatures (name + args) and their occurrence counts
/// to identify when the agent is stuck in a loop.
#[derive(Debug)]
pub struct LoopDetector {
    /// Tracks call signatures and their counts for the current turn.
    repeated_calls: HashMap<String, usize>,

    /// Total tool call count for the current turn.
    iteration_count: usize,

    /// Configuration for loop protection.
    config: LoopProtectionConfig,

    /// Whether detection is currently disabled for this session.
    disabled_for_session: bool,
}

impl LoopDetector {
    /// Create a new loop detector with the given configuration.
    pub fn new(config: LoopProtectionConfig) -> Self {
        Self {
            repeated_calls: HashMap::new(),
            iteration_count: 0,
            config,
            disabled_for_session: false,
        }
    }

    /// Create a new loop detector with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(LoopProtectionConfig::default())
    }

    /// Record a tool call and check for loops.
    ///
    /// Returns a `LoopDetectionResult` indicating whether the call should proceed.
    pub fn record_tool_call(
        &mut self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> LoopDetectionResult {
        // If disabled, allow everything
        if self.disabled_for_session || !self.config.enabled {
            self.iteration_count += 1;
            return LoopDetectionResult::Allowed;
        }

        // Check total iterations first
        self.iteration_count += 1;
        if self.iteration_count > self.config.max_tool_loops {
            return LoopDetectionResult::MaxIterationsReached {
                iterations: self.iteration_count,
                max_iterations: self.config.max_tool_loops,
                message: format!(
                    "Maximum tool call limit ({}) reached for this turn. \
                     The agent may be stuck in a loop. Consider adjusting \
                     loop_protection.max_tool_loops in settings if more iterations are needed.",
                    self.config.max_tool_loops
                ),
            };
        }

        // Check repeated calls with identical arguments
        let signature = make_signature(tool_name, args);
        let count = self.repeated_calls.entry(signature).or_insert(0);
        *count += 1;

        let max = self.config.max_repeated_tool_calls;
        let warning_at = (max as f64 * self.config.warning_threshold).ceil() as usize;

        if *count > max {
            LoopDetectionResult::Blocked {
                tool_name: tool_name.to_string(),
                repeat_count: *count,
                max_count: max,
                message: format!(
                    "Tool '{}' has been called {} times with identical arguments. \
                     This appears to be a loop. Consider adjusting \
                     loop_protection.max_repeated_tool_calls in settings if this is intentional.",
                    tool_name, count
                ),
            }
        } else if *count >= warning_at {
            LoopDetectionResult::Warning {
                tool_name: tool_name.to_string(),
                current_count: *count,
                max_count: max,
                message: format!(
                    "Tool '{}' has been called {} times with identical arguments. \
                     {} more calls will trigger loop protection.",
                    tool_name,
                    count,
                    max - *count
                ),
            }
        } else {
            LoopDetectionResult::Allowed
        }
    }

    /// Get the current iteration count.
    #[allow(dead_code)]
    pub fn iteration_count(&self) -> usize {
        self.iteration_count
    }

    /// Reset all tracking for a new turn.
    pub fn reset(&mut self) {
        self.repeated_calls.clear();
        self.iteration_count = 0;
    }

    /// Reset tracking for a specific tool signature only.
    /// Useful when the user acknowledges a potential loop but wants to continue.
    #[allow(dead_code)]
    pub fn reset_signature(&mut self, tool_name: &str, args: &serde_json::Value) {
        let signature = make_signature(tool_name, args);
        self.repeated_calls.remove(&signature);
    }

    /// Disable loop detection for the remainder of this session.
    pub fn disable_for_session(&mut self) {
        self.disabled_for_session = true;
    }

    /// Re-enable loop detection.
    pub fn enable(&mut self) {
        self.disabled_for_session = false;
    }

    /// Check if loop detection is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && !self.disabled_for_session
    }

    /// Get the current configuration.
    pub fn config(&self) -> &LoopProtectionConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: LoopProtectionConfig) {
        self.config = config;
    }

    /// Get statistics about current loop detection state.
    pub fn stats(&self) -> LoopDetectorStats {
        let most_repeated = self
            .repeated_calls
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(sig, count)| {
                // Extract tool name from signature (before the ':')
                let tool_name = sig.split(':').next().unwrap_or("unknown").to_string();
                (tool_name, *count)
            });

        let (most_repeated_tool, most_repeated_count) = match most_repeated {
            Some((name, count)) => (Some(name), count),
            None => (None, 0),
        };

        LoopDetectorStats {
            iteration_count: self.iteration_count,
            max_iterations: self.config.max_tool_loops,
            unique_signatures: self.repeated_calls.len(),
            most_repeated_tool,
            most_repeated_count,
            is_enabled: self.is_enabled(),
        }
    }
}

/// Statistics about the current loop detection state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDetectorStats {
    /// Current iteration count.
    pub iteration_count: usize,
    /// Maximum allowed iterations.
    pub max_iterations: usize,
    /// Number of unique tool call signatures.
    pub unique_signatures: usize,
    /// Name of the most repeated tool (if any).
    pub most_repeated_tool: Option<String>,
    /// Count of the most repeated tool.
    pub most_repeated_count: usize,
    /// Whether loop detection is enabled.
    pub is_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_default_config() {
        let config = LoopProtectionConfig::default();
        assert_eq!(config.max_tool_loops, 100);
        assert_eq!(config.max_repeated_tool_calls, 5);
        assert!(config.enabled);
    }

    #[test]
    fn test_allowed_calls() {
        let mut detector = LoopDetector::with_defaults();

        // First few calls should be allowed
        for i in 0..3 {
            let result =
                detector.record_tool_call("read_file", &json!({"path": format!("file{}.txt", i)}));
            assert!(result.is_allowed());
            assert!(!result.is_blocked());
        }
    }

    #[test]
    fn test_warning_threshold() {
        let mut detector = LoopDetector::with_defaults();
        let args = json!({"path": "same_file.txt"});

        // With max=5 and threshold=0.6, warning should trigger at call 3 (ceil(5 * 0.6) = 3)
        for i in 0..3 {
            let result = detector.record_tool_call("read_file", &args);
            if i < 2 {
                assert_eq!(result, LoopDetectionResult::Allowed);
            } else {
                assert!(matches!(result, LoopDetectionResult::Warning { .. }));
            }
        }
    }

    #[test]
    fn test_blocked_at_threshold() {
        let config = LoopProtectionConfig {
            max_repeated_tool_calls: 3,
            ..Default::default()
        };
        let mut detector = LoopDetector::new(config);
        let args = json!({"command": "ls -la"});

        // Calls 1-3 should be allowed (or warning)
        for _ in 0..3 {
            let result = detector.record_tool_call("run_pty_cmd", &args);
            assert!(result.is_allowed());
        }

        // Call 4 should be blocked
        let result = detector.record_tool_call("run_pty_cmd", &args);
        assert!(result.is_blocked());
        assert!(matches!(
            result,
            LoopDetectionResult::Blocked {
                repeat_count: 4,
                ..
            }
        ));
    }

    #[test]
    fn test_max_iterations_reached() {
        let config = LoopProtectionConfig {
            max_tool_loops: 5,
            ..Default::default()
        };
        let mut detector = LoopDetector::new(config);

        // Make 5 different calls (no repeat blocking)
        for i in 0..5 {
            let result = detector.record_tool_call("tool", &json!({"i": i}));
            assert!(result.is_allowed());
        }

        // 6th call should hit max iterations
        let result = detector.record_tool_call("tool", &json!({"i": 5}));
        assert!(matches!(
            result,
            LoopDetectionResult::MaxIterationsReached { .. }
        ));
    }

    #[test]
    fn test_reset_clears_counts() {
        let mut detector = LoopDetector::with_defaults();
        let args = json!({"path": "file.txt"});

        // Make some calls
        detector.record_tool_call("read_file", &args);
        detector.record_tool_call("read_file", &args);
        assert_eq!(detector.iteration_count(), 2);

        // Reset
        detector.reset();
        assert_eq!(detector.iteration_count(), 0);

        // Should start fresh
        let result = detector.record_tool_call("read_file", &args);
        assert_eq!(result, LoopDetectionResult::Allowed);
    }

    #[test]
    fn test_disabled_for_session() {
        let config = LoopProtectionConfig {
            max_repeated_tool_calls: 2,
            ..Default::default()
        };
        let mut detector = LoopDetector::new(config);
        let args = json!({"x": 1});

        // Make 2 calls, third would normally be blocked
        detector.record_tool_call("tool", &args);
        detector.record_tool_call("tool", &args);

        // Disable detection
        detector.disable_for_session();
        assert!(!detector.is_enabled());

        // Now should be allowed
        let result = detector.record_tool_call("tool", &args);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_different_args_not_counted() {
        let config = LoopProtectionConfig {
            max_repeated_tool_calls: 2,
            ..Default::default()
        };
        let mut detector = LoopDetector::new(config);

        // Different args should not trigger repeat detection
        for i in 0..10 {
            let result =
                detector.record_tool_call("read_file", &json!({"path": format!("file{}.txt", i)}));
            // Should hit max iterations at 100, not repeat threshold
            assert!(
                result.is_allowed()
                    || matches!(result, LoopDetectionResult::MaxIterationsReached { .. })
            );
        }
    }

    #[test]
    fn test_stats() {
        let mut detector = LoopDetector::with_defaults();

        detector.record_tool_call("read_file", &json!({"path": "a.txt"}));
        detector.record_tool_call("read_file", &json!({"path": "a.txt"}));
        detector.record_tool_call("write_file", &json!({"path": "b.txt"}));

        let stats = detector.stats();
        assert_eq!(stats.iteration_count, 3);
        assert_eq!(stats.unique_signatures, 2);
        assert_eq!(stats.most_repeated_tool, Some("read_file".to_string()));
        assert_eq!(stats.most_repeated_count, 2);
    }
}

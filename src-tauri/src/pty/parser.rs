use vte::{Params, Parser, Perform};

/// Events extracted from OSC 133 sequences
#[derive(Debug, Clone)]
pub enum OscEvent {
    /// OSC 133 ; A - Prompt start
    PromptStart,
    /// OSC 133 ; B - Prompt end (user can type)
    PromptEnd,
    /// OSC 133 ; C [; command] - Command execution started
    CommandStart { command: Option<String> },
    /// OSC 133 ; D ; N - Command finished with exit code N
    CommandEnd { exit_code: i32 },
    /// OSC 7 - Current working directory changed
    DirectoryChanged { path: String },
}

impl OscEvent {
    /// Convert to a tuple of (event_name, CommandBlockEvent) for emission.
    /// Returns None for DirectoryChanged events (handled separately).
    pub fn to_command_block_event(
        self,
        session_id: &str,
    ) -> Option<(&'static str, super::manager::CommandBlockEvent)> {
        use super::manager::CommandBlockEvent;

        Some(match self {
            OscEvent::PromptStart => (
                "command_block",
                CommandBlockEvent {
                    session_id: session_id.to_string(),
                    command: None,
                    exit_code: None,
                    event_type: "prompt_start".to_string(),
                },
            ),
            OscEvent::PromptEnd => (
                "command_block",
                CommandBlockEvent {
                    session_id: session_id.to_string(),
                    command: None,
                    exit_code: None,
                    event_type: "prompt_end".to_string(),
                },
            ),
            OscEvent::CommandStart { command } => (
                "command_block",
                CommandBlockEvent {
                    session_id: session_id.to_string(),
                    command,
                    exit_code: None,
                    event_type: "command_start".to_string(),
                },
            ),
            OscEvent::CommandEnd { exit_code } => (
                "command_block",
                CommandBlockEvent {
                    session_id: session_id.to_string(),
                    command: None,
                    exit_code: Some(exit_code),
                    event_type: "command_end".to_string(),
                },
            ),
            OscEvent::DirectoryChanged { .. } => return None,
        })
    }
}

/// Terminal output parser that extracts OSC sequences
pub struct TerminalParser {
    parser: Parser,
    performer: OscPerformer,
}

impl TerminalParser {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            performer: OscPerformer::new(),
        }
    }

    /// Parse terminal output and extract OSC events
    pub fn parse(&mut self, data: &[u8]) -> Vec<OscEvent> {
        self.performer.events.clear();
        for byte in data {
            self.parser.advance(&mut self.performer, *byte);
        }
        std::mem::take(&mut self.performer.events)
    }
}

impl Default for TerminalParser {
    fn default() -> Self {
        Self::new()
    }
}

struct OscPerformer {
    events: Vec<OscEvent>,
}

impl OscPerformer {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    fn handle_osc(&mut self, params: &[&[u8]]) {
        if params.is_empty() {
            return;
        }

        // Parse the OSC command number
        let cmd = match std::str::from_utf8(params[0]) {
            Ok(s) => s,
            Err(_) => return,
        };

        match cmd {
            // OSC 133 - Semantic prompt sequences
            "133" => self.handle_osc_133(params),
            // OSC 7 - Current working directory
            "7" => self.handle_osc_7(params),
            _ => {}
        }
    }

    fn handle_osc_133(&mut self, params: &[&[u8]]) {
        if params.len() < 2 {
            return;
        }

        let marker = match std::str::from_utf8(params[1]) {
            Ok(s) => s,
            Err(_) => return,
        };

        // Get extra argument from params[2] if present
        let extra_arg = params
            .get(2)
            .and_then(|p| std::str::from_utf8(p).ok());

        // Match on first character, handling both "C" and "C;command" formats
        match marker.chars().next() {
            Some('A') => self.events.push(OscEvent::PromptStart),
            Some('B') => self.events.push(OscEvent::PromptEnd),
            Some('C') => {
                // Command may come from marker suffix (C;cmd) or params[2]
                let command = marker
                    .strip_prefix("C;")
                    .or(extra_arg)
                    .map(|s| s.to_string());
                self.events.push(OscEvent::CommandStart { command });
            }
            Some('D') => {
                // Exit code may come from marker suffix (D;0) or params[2]
                let exit_code = marker
                    .strip_prefix("D;")
                    .or(extra_arg)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                self.events.push(OscEvent::CommandEnd { exit_code });
            }
            _ => {}
        }
    }

    fn handle_osc_7(&mut self, params: &[&[u8]]) {
        // OSC 7 format: file://hostname/path
        if params.len() < 2 {
            tracing::debug!("[cwd-sync] OSC 7 received but params.len() < 2");
            return;
        }

        let url = match std::str::from_utf8(params[1]) {
            Ok(s) => s,
            Err(_) => {
                tracing::debug!("[cwd-sync] OSC 7 URL is not valid UTF-8");
                return;
            }
        };

        tracing::debug!("[cwd-sync] OSC 7 URL: {}", url);

        // Parse file:// URL
        if let Some(path) = url.strip_prefix("file://") {
            // Remove hostname (everything up to the next /)
            if let Some(idx) = path.find('/') {
                let path = &path[idx..];
                // URL decode the path
                let path = urlencoding_decode(path);
                tracing::info!("[cwd-sync] Directory changed to: {}", path);
                self.events.push(OscEvent::DirectoryChanged { path });
            } else {
                tracing::debug!("[cwd-sync] OSC 7 path has no slash after hostname");
            }
        } else {
            tracing::debug!("[cwd-sync] OSC 7 URL does not start with file://");
        }
    }
}

impl Perform for OscPerformer {
    fn print(&mut self, _c: char) {}
    fn execute(&mut self, _byte: u8) {}
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        self.handle_osc(params);
    }
}

/// Simple URL decoding for paths
fn urlencoding_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let mut hex = String::new();
            if let Some(&h1) = chars.peek() {
                hex.push(h1);
                chars.next();
            }
            if let Some(&h2) = chars.peek() {
                hex.push(h2);
                chars.next();
            }
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // OSC 133 - Prompt lifecycle tests
    // ===========================================

    #[test]
    fn test_osc_133_prompt_start() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; A ST (using BEL as terminator)
        let data = b"\x1b]133;A\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], OscEvent::PromptStart));
    }

    #[test]
    fn test_osc_133_prompt_end() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; B ST (using BEL as terminator)
        let data = b"\x1b]133;B\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], OscEvent::PromptEnd));
    }

    #[test]
    fn test_osc_133_prompt_start_with_st_terminator() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; A ST (using ESC \ as string terminator)
        let data = b"\x1b]133;A\x1b\\";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], OscEvent::PromptStart));
    }

    // ===========================================
    // OSC 133 - Command lifecycle tests
    // ===========================================

    #[test]
    fn test_osc_133_command_start_no_command() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; C ST (no command text)
        let data = b"\x1b]133;C\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandStart { command } = &events[0] {
            assert!(command.is_none());
        } else {
            panic!("Expected CommandStart");
        }
    }

    #[test]
    fn test_osc_133_command_with_text() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; C ; ls -la ST
        let data = b"\x1b]133;C;ls -la\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandStart { command } = &events[0] {
            assert_eq!(command.as_deref(), Some("ls -la"));
        } else {
            panic!("Expected CommandStart");
        }
    }

    #[test]
    fn test_osc_133_command_with_complex_text() {
        let mut parser = TerminalParser::new();
        // Complex command with pipes, flags, etc.
        let data = b"\x1b]133;C;cat file.txt | grep -E 'pattern' | head -n 10\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandStart { command } = &events[0] {
            assert_eq!(
                command.as_deref(),
                Some("cat file.txt | grep -E 'pattern' | head -n 10")
            );
        } else {
            panic!("Expected CommandStart");
        }
    }

    #[test]
    fn test_osc_133_command_end_success() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; D ; 0 ST
        let data = b"\x1b]133;D;0\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandEnd { exit_code } = &events[0] {
            assert_eq!(*exit_code, 0);
        } else {
            panic!("Expected CommandEnd");
        }
    }

    #[test]
    fn test_osc_133_command_end_failure() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; D ; 1 ST (command failed)
        let data = b"\x1b]133;D;1\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandEnd { exit_code } = &events[0] {
            assert_eq!(*exit_code, 1);
        } else {
            panic!("Expected CommandEnd");
        }
    }

    #[test]
    fn test_osc_133_command_end_signal() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; D ; 130 ST (Ctrl+C typically gives 128 + 2 = 130)
        let data = b"\x1b]133;D;130\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandEnd { exit_code } = &events[0] {
            assert_eq!(*exit_code, 130);
        } else {
            panic!("Expected CommandEnd");
        }
    }

    #[test]
    fn test_osc_133_command_end_no_exit_code() {
        let mut parser = TerminalParser::new();
        // OSC 133 ; D ST (no exit code, defaults to 0)
        let data = b"\x1b]133;D\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandEnd { exit_code } = &events[0] {
            assert_eq!(*exit_code, 0);
        } else {
            panic!("Expected CommandEnd");
        }
    }

    // ===========================================
    // Full command lifecycle test
    // ===========================================

    #[test]
    fn test_full_command_lifecycle() {
        let mut parser = TerminalParser::new();

        // Simulate a full command lifecycle:
        // 1. Prompt starts
        // 2. Prompt ends (user can type)
        // 3. Command starts (user pressed enter)
        // 4. Command ends with exit code

        let prompt_start = b"\x1b]133;A\x07";
        let prompt_end = b"\x1b]133;B\x07";
        let command_start = b"\x1b]133;C;echo hello\x07";
        let command_end = b"\x1b]133;D;0\x07";

        let events = parser.parse(prompt_start);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], OscEvent::PromptStart));

        let events = parser.parse(prompt_end);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], OscEvent::PromptEnd));

        let events = parser.parse(command_start);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandStart { command } = &events[0] {
            assert_eq!(command.as_deref(), Some("echo hello"));
        } else {
            panic!("Expected CommandStart");
        }

        let events = parser.parse(command_end);
        assert_eq!(events.len(), 1);
        if let OscEvent::CommandEnd { exit_code } = &events[0] {
            assert_eq!(*exit_code, 0);
        } else {
            panic!("Expected CommandEnd");
        }
    }

    #[test]
    fn test_multiple_events_in_single_parse() {
        let mut parser = TerminalParser::new();
        // Multiple OSC sequences in one chunk
        let data = b"\x1b]133;A\x07\x1b]133;B\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], OscEvent::PromptStart));
        assert!(matches!(events[1], OscEvent::PromptEnd));
    }

    // ===========================================
    // OSC 7 - Directory change tests
    // ===========================================

    #[test]
    fn test_osc_7_directory() {
        let mut parser = TerminalParser::new();
        // OSC 7 ; file://hostname/Users/test ST
        let data = b"\x1b]7;file://localhost/Users/test\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::DirectoryChanged { path } = &events[0] {
            assert_eq!(path, "/Users/test");
        } else {
            panic!("Expected DirectoryChanged");
        }
    }

    #[test]
    fn test_osc_7_directory_with_spaces() {
        let mut parser = TerminalParser::new();
        // Path with URL-encoded spaces (%20)
        let data = b"\x1b]7;file://localhost/Users/test/My%20Documents\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::DirectoryChanged { path } = &events[0] {
            assert_eq!(path, "/Users/test/My Documents");
        } else {
            panic!("Expected DirectoryChanged");
        }
    }

    #[test]
    fn test_osc_7_directory_deep_path() {
        let mut parser = TerminalParser::new();
        let data = b"\x1b]7;file://macbook.local/Users/xlyk/Code/qbit/src-tauri\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        if let OscEvent::DirectoryChanged { path } = &events[0] {
            assert_eq!(path, "/Users/xlyk/Code/qbit/src-tauri");
        } else {
            panic!("Expected DirectoryChanged");
        }
    }

    // ===========================================
    // URL encoding/decoding tests
    // ===========================================

    #[test]
    fn test_urlencoding_decode_simple() {
        assert_eq!(urlencoding_decode("/path/to/file"), "/path/to/file");
    }

    #[test]
    fn test_urlencoding_decode_space() {
        assert_eq!(
            urlencoding_decode("/path/My%20Documents"),
            "/path/My Documents"
        );
    }

    #[test]
    fn test_urlencoding_decode_multiple_encoded() {
        assert_eq!(
            urlencoding_decode("/path%20with%20multiple%20spaces"),
            "/path with multiple spaces"
        );
    }

    #[test]
    fn test_urlencoding_decode_special_chars() {
        // %23 = #, %26 = &, %3D = =
        assert_eq!(urlencoding_decode("/path%23file"), "/path#file");
    }

    #[test]
    fn test_urlencoding_decode_invalid_hex() {
        // Invalid hex sequence should be preserved
        assert_eq!(urlencoding_decode("/path%ZZ"), "/path%ZZ");
    }

    #[test]
    fn test_urlencoding_decode_incomplete_percent() {
        // Incomplete percent encoding at end - only 1 hex digit
        // Current implementation tries to decode anyway (0x02 = STX control char)
        assert_eq!(urlencoding_decode("/path%2"), "/path\u{2}");
    }

    // ===========================================
    // Edge cases and robustness tests
    // ===========================================

    #[test]
    fn test_parser_ignores_regular_text() {
        let mut parser = TerminalParser::new();
        // Regular terminal output with no OSC sequences
        let data = b"Hello, world!\nThis is normal output.\n";
        let events = parser.parse(data);
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_parser_handles_mixed_content() {
        let mut parser = TerminalParser::new();
        // Normal text mixed with OSC sequences
        let data = b"Some output\x1b]133;A\x07more output\x1b]133;B\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], OscEvent::PromptStart));
        assert!(matches!(events[1], OscEvent::PromptEnd));
    }

    #[test]
    fn test_parser_handles_ansi_escape_codes() {
        let mut parser = TerminalParser::new();
        // ANSI color codes should be ignored, OSC should be parsed
        let data = b"\x1b[32mgreen text\x1b[0m\x1b]133;A\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], OscEvent::PromptStart));
    }

    #[test]
    fn test_parser_ignores_unknown_osc() {
        let mut parser = TerminalParser::new();
        // OSC 0 (window title) - should be ignored
        let data = b"\x1b]0;Window Title\x07";
        let events = parser.parse(data);
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_parser_empty_input() {
        let mut parser = TerminalParser::new();
        let events = parser.parse(b"");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_parser_partial_osc_sequence() {
        let mut parser = TerminalParser::new();
        // Incomplete OSC sequence (no terminator)
        let data = b"\x1b]133;A";
        let events = parser.parse(data);
        // VTE parser buffers incomplete sequences, so nothing should be emitted yet
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_parser_is_stateless_between_calls() {
        let mut parser = TerminalParser::new();

        // First parse
        let events1 = parser.parse(b"\x1b]133;A\x07");
        assert_eq!(events1.len(), 1);

        // Second parse - events from first call should be cleared
        let events2 = parser.parse(b"\x1b]133;B\x07");
        assert_eq!(events2.len(), 1);
        assert!(matches!(events2[0], OscEvent::PromptEnd));
    }

    #[test]
    fn test_parser_default_trait() {
        let mut parser = TerminalParser::default();
        assert!(parser.parse(b"\x1b]133;A\x07").len() == 1);
    }
}

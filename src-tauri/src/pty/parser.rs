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

        match marker {
            "A" => {
                self.events.push(OscEvent::PromptStart);
            }
            "B" => {
                self.events.push(OscEvent::PromptEnd);
            }
            "C" => {
                // C marker may include command text: "C;command"
                let command = if params.len() > 2 {
                    std::str::from_utf8(params[2]).ok().map(|s| s.to_string())
                } else {
                    None
                };
                self.events.push(OscEvent::CommandStart { command });
            }
            "D" => {
                // D marker includes exit code: "D;0" or "D;1"
                let exit_code = if params.len() > 2 {
                    std::str::from_utf8(params[2])
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0)
                } else {
                    0
                };
                self.events.push(OscEvent::CommandEnd { exit_code });
            }
            _ => {
                // Handle markers that might have the command/exit code attached
                // e.g., "C;ls -la" comes as a single param "C;ls -la"
                if marker.starts_with("C;") {
                    let command = marker.strip_prefix("C;").map(|s| s.to_string());
                    self.events.push(OscEvent::CommandStart { command });
                } else if marker.starts_with("C") && marker.len() == 1 {
                    self.events.push(OscEvent::CommandStart { command: None });
                } else if marker.starts_with("D;") {
                    let exit_code = marker
                        .strip_prefix("D;")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    self.events.push(OscEvent::CommandEnd { exit_code });
                }
            }
        }
    }

    fn handle_osc_7(&mut self, params: &[&[u8]]) {
        // OSC 7 format: file://hostname/path
        if params.len() < 2 {
            return;
        }

        let url = match std::str::from_utf8(params[1]) {
            Ok(s) => s,
            Err(_) => return,
        };

        // Parse file:// URL
        if let Some(path) = url.strip_prefix("file://") {
            // Remove hostname (everything up to the next /)
            if let Some(idx) = path.find('/') {
                let path = &path[idx..];
                // URL decode the path
                let path = urlencoding_decode(path);
                self.events.push(OscEvent::DirectoryChanged { path });
            }
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
    fn csi_dispatch(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

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
    fn test_osc_133_command_end() {
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
}

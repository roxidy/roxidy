**Shell integration script (~/.config/yourterm/integration.zsh):**

```zsh
# OSC 133 sequences for semantic shell integration
# These are the same sequences iTerm2/VSCode/Warp use

_osc() { printf '\e]133;%s\e\\' "$1" }

# Prompt markers
_prompt_start() { _osc "A" }
_prompt_end() { _osc "B" }

# Command execution markers  
_cmd_start() { _osc "C" }
_cmd_finished() { _osc "D;$?" }  # includes exit code

# Current working directory (OSC 7)
_report_cwd() { printf '\e]7;file://%s%s\e\\' "$HOST" "$PWD" }

# Hook implementations
_yourterm_preexec() {
  _cmd_start
}

_yourterm_precmd() {
  local exit_code=$?
  _cmd_finished
  _report_cwd
  _prompt_start
}

# Emit prompt end after PS1 renders
_yourterm_line_init() {
  _prompt_end
}

# Register hooks
autoload -Uz add-zsh-hook
add-zsh-hook preexec _yourterm_preexec
add-zsh-hook precmd _yourterm_precmd

# ZLE hook for prompt end timing
zle -N zle-line-init _yourterm_line_init
```

**Rust side - VTE parser for command blocks:**

```rust
// Cargo.toml dependencies:
// vte = "0.13"
// portable-pty = "0.8"
// anyhow = "1.0"
// tokio = { version = "1", features = ["full"] }

use std::sync::{Arc, Mutex};
use vte::{Params, Parser, Perform};

#[derive(Debug, Clone, Default)]
pub struct CommandBlock {
    pub command: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub start_time: Option<std::time::Instant>,
    pub duration: Option<std::time::Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ShellState {
    Idle,           // Between commands, at prompt
    PromptActive,   // Prompt is being displayed (after A, before B)
    ReadingInput,   // User is typing (after B, before C)
    Executing,      // Command is running (after C, before D)
}

pub struct TerminalParser {
    state: ShellState,
    current_block: CommandBlock,
    completed_blocks: Vec<CommandBlock>,
    current_output: String,
    osc_buffer: String,
    in_osc: bool,
}

impl TerminalParser {
    pub fn new() -> Self {
        Self {
            state: ShellState::Idle,
            current_block: CommandBlock::default(),
            completed_blocks: Vec::new(),
            current_output: String::new(),
            osc_buffer: String::new(),
            in_osc: false,
        }
    }

    pub fn get_completed_blocks(&mut self) -> Vec<CommandBlock> {
        std::mem::take(&mut self.completed_blocks)
    }

    fn handle_osc(&mut self, data: &str) {
        // OSC 133 format: "133;X" or "133;X;payload"
        if !data.starts_with("133;") {
            return;
        }

        let parts: Vec<&str> = data[4..].splitn(2, ';').collect();
        let marker = parts[0];
        let payload = parts.get(1).copied();

        match marker {
            "A" => {
                // Prompt start
                self.state = ShellState::PromptActive;
            }
            "B" => {
                // Prompt end, user can type
                self.state = ShellState::ReadingInput;
                self.current_block = CommandBlock::default();
            }
            "C" => {
                // Command starting
                self.state = ShellState::Executing;
                self.current_block.start_time = Some(std::time::Instant::now());
                self.current_output.clear();
            }
            "D" => {
                // Command finished
                if let Some(code_str) = payload {
                    self.current_block.exit_code = code_str.parse().ok();
                }
                
                if let Some(start) = self.current_block.start_time {
                    self.current_block.duration = Some(start.elapsed());
                }
                
                self.current_block.output = std::mem::take(&mut self.current_output);
                
                // Only save if there was actually a command
                if !self.current_block.output.is_empty() 
                    || self.current_block.exit_code.is_some() 
                {
                    self.completed_blocks.push(self.current_block.clone());
                }
                
                self.state = ShellState::Idle;
            }
            _ => {}
        }
    }
}

impl Perform for TerminalParser {
    fn print(&mut self, c: char) {
        if self.in_osc {
            self.osc_buffer.push(c);
        } else if self.state == ShellState::Executing {
            self.current_output.push(c);
        }
    }

    fn execute(&mut self, byte: u8) {
        // Control characters (newline, carriage return, etc.)
        if self.state == ShellState::Executing {
            match byte {
                b'\n' => self.current_output.push('\n'),
                b'\r' => {} // ignore CR
                b'\t' => self.current_output.push('\t'),
                _ => {}
            }
        }
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        // DCS sequences - not used for our purposes
    }

    fn put(&mut self, byte: u8) {
        // DCS data
    }

    fn unhook(&mut self) {
        // DCS end
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        // OSC sequence received
        let data: String = params
            .iter()
            .map(|p| String::from_utf8_lossy(p))
            .collect::<Vec<_>>()
            .join(";");
        
        self.handle_osc(&data);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        // CSI sequences (cursor movement, colors, etc.)
        // You'd handle these for proper terminal emulation
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        // ESC sequences
    }
}
```

**Main terminal application:**

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

fn main() -> anyhow::Result<()> {
    let pty_system = native_pty_system();

    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Set up shell with integration script
    let mut cmd = CommandBuilder::new("zsh");
    cmd.env("YOURTERM", "1"); // Let shell know it's running in your terminal
    
    // Option 1: Source integration in .zshrc based on YOURTERM env var
    // Option 2: Use ZDOTDIR to point to custom config directory
    // Option 3: Pass init command
    cmd.args(&["-c", "source ~/.config/yourterm/integration.zsh; exec zsh -i"]);

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave); // Close slave in parent process

    let reader = pair.master.try_clone_reader()?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));

    // Shared parser state
    let parser_state = Arc::new(Mutex::new(TerminalParser::new()));
    let mut vte_parser = vte::Parser::new();

    // Reader thread - processes PTY output
    let parser_clone = Arc::clone(&parser_state);
    let reader_handle = thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let mut state = parser_clone.lock().unwrap();
                    
                    // Feed bytes through VTE parser
                    for byte in &buf[..n] {
                        vte_parser.advance(&mut *state, *byte);
                    }
                    
                    // Check for completed command blocks
                    for block in state.get_completed_blocks() {
                        println!("\n=== Command Block ===");
                        println!("Exit code: {:?}", block.exit_code);
                        println!("Duration: {:?}", block.duration);
                        println!("Output length: {} bytes", block.output.len());
                        println!("Output preview: {}...", 
                            block.output.chars().take(100).collect::<String>());
                        println!("====================\n");
                    }
                    
                    // Also print raw output for now (you'd render this properly)
                    print!("{}", String::from_utf8_lossy(&buf[..n]));
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    break;
                }
            }
        }
    });

    // Writer thread - handles user input
    let writer_clone = Arc::clone(&writer);
    let input_handle = thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut writer = writer_clone.lock().unwrap();
                    let _ = writer.write_all(&buf[..n]);
                }
                Err(_) => break,
            }
        }
    });

    reader_handle.join().ok();
    
    Ok(())
}
```

**Simpler integration script approach (add to user's .zshrc):**

```zsh
# Auto-detect and load integration
if [[ -n "$YOURTERM" ]]; then
    source ~/.config/yourterm/integration.zsh
fi
```

**What you get from this:**

Each `CommandBlock` contains:
- `exit_code` — the actual return code
- `duration` — how long it ran
- `output` — all stdout/stderr text
- Implicit: you know when one command ends and the next begins

**Next steps for a Warp-like experience:**

1. **Terminal grid rendering** — use `alacritty_terminal` or build your own with `crossterm`
2. **Block-based UI** — render each CommandBlock as a collapsible card
3. **Raw mode input** — capture keystrokes directly for autocomplete, history navigation
4. **GPU rendering** — Warp uses wgpu, you could use `wgpu` or `glyphon` for text
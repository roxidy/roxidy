use crate::error::{QbitError, Result};
use parking_lot::Mutex;
use portable_pty::{Child, MasterPty, PtySize};
#[cfg(feature = "tauri")]
use portable_pty::{native_pty_system, CommandBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "tauri")]
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "tauri")]
use std::thread;
#[cfg(feature = "tauri")]
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[cfg(feature = "tauri")]
use super::parser::{OscEvent, TerminalParser};

// Import runtime types for the runtime-based emitter
#[cfg(feature = "tauri")]
use crate::runtime::{QbitRuntime, RuntimeEvent};

// ============================================================================
// PtyEventEmitter Trait - Internal abstraction for event emission
// ============================================================================

/// Internal trait for emitting PTY events.
///
/// This trait abstracts over how PTY events (output, exit, directory changes, etc.)
/// are delivered to consumers. Implementations exist for:
/// - `AppHandleEmitter`: Emits events via Tauri's AppHandle (for GUI)
/// - `RuntimeEmitter`: Emits events via QbitRuntime (for CLI and other runtimes)
///
/// # Thread Safety
/// Implementors must be `Send + Sync + 'static` to work with std::thread spawning
/// in the PTY read loop.
#[cfg(feature = "tauri")]
trait PtyEventEmitter: Send + Sync + 'static {
    /// Emit terminal output data
    fn emit_output(&self, session_id: &str, data: &str);

    /// Emit session ended event
    fn emit_session_ended(&self, session_id: &str);

    /// Emit directory changed event
    fn emit_directory_changed(&self, session_id: &str, path: &str);

    /// Emit command block event (prompt start/end, command start/end)
    fn emit_command_block(&self, event_name: &str, event: CommandBlockEvent);
}

// ============================================================================
// AppHandleEmitter - Tauri-specific implementation
// ============================================================================

/// Event emitter that uses Tauri's AppHandle for GUI event delivery.
///
/// This maintains backward compatibility with the existing Tauri-based event system.
#[cfg(feature = "tauri")]
struct AppHandleEmitter(AppHandle);

#[cfg(feature = "tauri")]
impl PtyEventEmitter for AppHandleEmitter {
    fn emit_output(&self, session_id: &str, data: &str) {
        let _ = self.0.emit(
            "terminal_output",
            TerminalOutputEvent {
                session_id: session_id.to_string(),
                data: data.to_string(),
            },
        );
    }

    fn emit_session_ended(&self, session_id: &str) {
        let _ = self.0.emit(
            "session_ended",
            serde_json::json!({
                "sessionId": session_id
            }),
        );
    }

    fn emit_directory_changed(&self, session_id: &str, path: &str) {
        tracing::info!(
            "[cwd-sync] Emitting directory_changed event: session={}, path={}",
            session_id,
            path
        );
        let _ = self.0.emit(
            "directory_changed",
            DirectoryChangedEvent {
                session_id: session_id.to_string(),
                path: path.to_string(),
            },
        );
    }

    fn emit_command_block(&self, event_name: &str, event: CommandBlockEvent) {
        let _ = self.0.emit(event_name, event);
    }
}

// ============================================================================
// RuntimeEmitter - QbitRuntime-based implementation
// ============================================================================

/// Event emitter that uses QbitRuntime for CLI and other non-Tauri environments.
///
/// This emitter converts PTY events to `RuntimeEvent` variants and emits them
/// through the runtime's `emit()` method. This allows the CLI to receive
/// terminal events through the same abstraction used for AI events.
#[cfg(feature = "tauri")]
struct RuntimeEmitter(Arc<dyn QbitRuntime>);

#[cfg(feature = "tauri")]
impl PtyEventEmitter for RuntimeEmitter {
    fn emit_output(&self, session_id: &str, data: &str) {
        // Convert string data to bytes for RuntimeEvent::TerminalOutput
        let _ = self.0.emit(RuntimeEvent::TerminalOutput {
            session_id: session_id.to_string(),
            data: data.as_bytes().to_vec(),
        });
    }

    fn emit_session_ended(&self, session_id: &str) {
        // Use TerminalExit with no exit code (EOF/closed)
        let _ = self.0.emit(RuntimeEvent::TerminalExit {
            session_id: session_id.to_string(),
            code: None,
        });
    }

    fn emit_directory_changed(&self, session_id: &str, path: &str) {
        tracing::info!(
            "[cwd-sync] Emitting directory_changed via runtime: session={}, path={}",
            session_id,
            path
        );
        // Use Custom event for directory changes (not yet in RuntimeEvent enum)
        let _ = self.0.emit(RuntimeEvent::Custom {
            name: "directory_changed".to_string(),
            payload: serde_json::json!({
                "session_id": session_id,
                "path": path
            }),
        });
    }

    fn emit_command_block(&self, event_name: &str, event: CommandBlockEvent) {
        // Use Custom event for command block events
        let _ = self.0.emit(RuntimeEvent::Custom {
            name: event_name.to_string(),
            payload: serde_json::to_value(&event).unwrap_or_default(),
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtySession {
    pub id: String,
    pub working_directory: String,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalOutputEvent {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandBlockEvent {
    pub session_id: String,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryChangedEvent {
    pub session_id: String,
    pub path: String,
}

/// Internal session state tracking active PTY sessions.
/// Only available when the `tauri` feature is enabled.
#[cfg(feature = "tauri")]
struct ActiveSession {
    #[allow(dead_code)]
    child: Mutex<Box<dyn Child + Send + Sync>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Box<dyn Write + Send>>,
    working_directory: Mutex<PathBuf>,
    rows: Mutex<u16>,
    cols: Mutex<u16>,
}

/// Manager for PTY sessions.
///
/// When the `tauri` feature is enabled, this provides full PTY session management
/// with event emission to the Tauri frontend. Without the feature, it provides
/// a minimal stub for compilation.
#[derive(Default)]
pub struct PtyManager {
    #[cfg(feature = "tauri")]
    sessions: Mutex<HashMap<String, Arc<ActiveSession>>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Internal Implementation
    // ========================================================================

    /// Internal implementation that takes a generic emitter.
    ///
    /// This is the core session creation logic, abstracted over the event
    /// emission mechanism. Both `create_session` (AppHandle) and
    /// `create_session_with_runtime` (QbitRuntime) delegate to this method.
    #[cfg(feature = "tauri")]
    fn create_session_internal<E: PtyEventEmitter>(
        &self,
        emitter: Arc<E>,
        working_directory: Option<PathBuf>,
        rows: u16,
        cols: u16,
    ) -> Result<PtySession> {
        let session_id = Uuid::new_v4().to_string();
        let pty_system = native_pty_system();

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| QbitError::Pty(e.to_string()))?;

        let mut cmd = CommandBuilder::new("zsh");
        cmd.args(["-l"]);

        cmd.env("QBIT", "1");
        cmd.env("QBIT_VERSION", env!("CARGO_PKG_VERSION"));
        cmd.env("TERM", "xterm-256color");

        let work_dir = working_directory.unwrap_or_else(|| {
            // Try INIT_CWD first (set by pnpm/npm to original invocation directory)
            // Then try current_dir, adjusting for src-tauri if needed
            // Fall back to home dir, then root
            if let Ok(init_cwd) = std::env::var("INIT_CWD") {
                return PathBuf::from(init_cwd);
            }

            if let Ok(cwd) = std::env::current_dir() {
                // If we're in src-tauri, go up to project root
                if cwd.ends_with("src-tauri") {
                    if let Some(parent) = cwd.parent() {
                        return parent.to_path_buf();
                    }
                }
                return cwd;
            }

            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
        });
        cmd.cwd(&work_dir);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| QbitError::Pty(e.to_string()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| QbitError::Pty(e.to_string()))?;

        let master = Arc::new(Mutex::new(pair.master));

        let session = Arc::new(ActiveSession {
            child: Mutex::new(child),
            master: master.clone(),
            writer: Mutex::new(writer),
            working_directory: Mutex::new(work_dir.clone()),
            rows: Mutex::new(rows),
            cols: Mutex::new(cols),
        });

        // Store session
        {
            let mut sessions = self.sessions.lock();
            sessions.insert(session_id.clone(), session.clone());
        }

        // Start read thread with the generic emitter
        let reader_session_id = session_id.clone();

        // Get a reader from the master
        let mut reader = {
            let master = master.lock();
            master
                .try_clone_reader()
                .map_err(|e| QbitError::Pty(e.to_string()))?
        };

        thread::spawn(move || {
            let mut parser = TerminalParser::new();
            let mut buf = [0u8; 4096];

            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        emitter.emit_session_ended(&reader_session_id);
                        break;
                    }
                    Ok(n) => {
                        let data = &buf[..n];
                        let events = parser.parse(data);

                        for event in events {
                            match &event {
                                OscEvent::DirectoryChanged { path } => {
                                    emitter.emit_directory_changed(&reader_session_id, path);
                                }
                                _ => {
                                    if let Some((event_name, payload)) =
                                        event.to_command_block_event(&reader_session_id)
                                    {
                                        emitter.emit_command_block(event_name, payload);
                                    }
                                }
                            }
                        }

                        let output = String::from_utf8_lossy(data).to_string();
                        emitter.emit_output(&reader_session_id, &output);
                    }
                    Err(e) => {
                        tracing::error!("Read error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(PtySession {
            id: session_id,
            working_directory: work_dir.to_string_lossy().to_string(),
            rows,
            cols,
        })
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Create a PTY session with Tauri event emission.
    ///
    /// This method is only available when the `tauri` feature is enabled.
    /// It uses `AppHandle` to emit events to the Tauri frontend.
    ///
    /// # Arguments
    /// * `app_handle` - Tauri application handle for event emission
    /// * `working_directory` - Initial working directory (defaults to project root)
    /// * `rows` - Terminal height in rows
    /// * `cols` - Terminal width in columns
    ///
    /// # See Also
    /// * [`create_session_with_runtime`] - Runtime-agnostic version for CLI usage
    #[cfg(feature = "tauri")]
    #[deprecated(
        since = "0.2.0",
        note = "Use create_session_with_runtime() with TauriRuntime instead"
    )]
    pub fn create_session(
        &self,
        app_handle: AppHandle,
        working_directory: Option<PathBuf>,
        rows: u16,
        cols: u16,
    ) -> Result<PtySession> {
        let emitter = Arc::new(AppHandleEmitter(app_handle));
        self.create_session_internal(emitter, working_directory, rows, cols)
    }

    /// Create a PTY session with runtime-based event emission.
    ///
    /// This method is the preferred way to create PTY sessions as it works with
    /// any `QbitRuntime` implementation (Tauri, CLI, or future runtimes).
    ///
    /// # Arguments
    /// * `runtime` - Runtime implementation for event emission
    /// * `working_directory` - Initial working directory (defaults to project root)
    /// * `rows` - Terminal height in rows
    /// * `cols` - Terminal width in columns
    ///
    /// # Example
    /// ```rust,ignore
    /// // With TauriRuntime
    /// let runtime = Arc::new(TauriRuntime::new(app_handle));
    /// let session = pty_manager.create_session_with_runtime(runtime, None, 24, 80)?;
    ///
    /// // With CliRuntime
    /// let runtime = Arc::new(CliRuntime::new(event_tx, true, false, false));
    /// let session = pty_manager.create_session_with_runtime(runtime, None, 24, 80)?;
    /// ```
    #[cfg(feature = "tauri")]
    pub fn create_session_with_runtime(
        &self,
        runtime: Arc<dyn QbitRuntime>,
        working_directory: Option<PathBuf>,
        rows: u16,
        cols: u16,
    ) -> Result<PtySession> {
        let emitter = Arc::new(RuntimeEmitter(runtime));
        self.create_session_internal(emitter, working_directory, rows, cols)
    }

    #[cfg(feature = "tauri")]
    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| QbitError::SessionNotFound(session_id.to_string()))?;

        let mut writer = session.writer.lock();
        writer.write_all(data).map_err(QbitError::Io)?;
        writer.flush().map_err(QbitError::Io)?;

        Ok(())
    }

    #[cfg(feature = "tauri")]
    pub fn resize(&self, session_id: &str, rows: u16, cols: u16) -> Result<()> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| QbitError::SessionNotFound(session_id.to_string()))?;

        let master = session.master.lock();
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| QbitError::Pty(e.to_string()))?;

        *session.rows.lock() = rows;
        *session.cols.lock() = cols;

        Ok(())
    }

    #[cfg(feature = "tauri")]
    pub fn destroy(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock();
        sessions
            .remove(session_id)
            .ok_or_else(|| QbitError::SessionNotFound(session_id.to_string()))?;
        Ok(())
    }

    #[cfg(feature = "tauri")]
    pub fn get_session(&self, session_id: &str) -> Result<PtySession> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| QbitError::SessionNotFound(session_id.to_string()))?;

        let working_directory = session
            .working_directory
            .lock()
            .to_string_lossy()
            .to_string();
        let rows = *session.rows.lock();
        let cols = *session.cols.lock();

        Ok(PtySession {
            id: session_id.to_string(),
            working_directory,
            rows,
            cols,
        })
    }
}

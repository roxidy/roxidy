use crate::error::{Result, QbitError};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use super::parser::{OscEvent, TerminalParser};

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

struct ActiveSession {
    #[allow(dead_code)]
    child: Mutex<Box<dyn Child + Send + Sync>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Box<dyn Write + Send>>,
    working_directory: Mutex<PathBuf>,
    rows: Mutex<u16>,
    cols: Mutex<u16>,
}

// Implement Send + Sync for ActiveSession
unsafe impl Send for ActiveSession {}
unsafe impl Sync for ActiveSession {}

pub struct PtyManager {
    sessions: Mutex<HashMap<String, Arc<ActiveSession>>>,
}

// Implement Send + Sync for PtyManager
unsafe impl Send for PtyManager {}
unsafe impl Sync for PtyManager {}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn create_session(
        &self,
        app_handle: AppHandle,
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

        // Start read thread
        let reader_session_id = session_id.clone();
        let reader_app_handle = app_handle.clone();

        // Get a reader from the master
        let mut reader = {
            let master = master.lock();
            master.try_clone_reader()
                .map_err(|e| QbitError::Pty(e.to_string()))?
        };

        thread::spawn(move || {
            let mut parser = TerminalParser::new();
            let mut buf = [0u8; 4096];

            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        let _ = reader_app_handle.emit("session_ended", serde_json::json!({
                            "sessionId": reader_session_id
                        }));
                        break;
                    }
                    Ok(n) => {
                        let data = &buf[..n];
                        let events = parser.parse(data);

                        for event in events {
                            match event {
                                OscEvent::PromptStart => {
                                    let _ = reader_app_handle.emit("command_block", CommandBlockEvent {
                                        session_id: reader_session_id.clone(),
                                        command: None,
                                        exit_code: None,
                                        event_type: "prompt_start".to_string(),
                                    });
                                }
                                OscEvent::PromptEnd => {
                                    let _ = reader_app_handle.emit("command_block", CommandBlockEvent {
                                        session_id: reader_session_id.clone(),
                                        command: None,
                                        exit_code: None,
                                        event_type: "prompt_end".to_string(),
                                    });
                                }
                                OscEvent::CommandStart { command } => {
                                    let _ = reader_app_handle.emit("command_block", CommandBlockEvent {
                                        session_id: reader_session_id.clone(),
                                        command,
                                        exit_code: None,
                                        event_type: "command_start".to_string(),
                                    });
                                }
                                OscEvent::CommandEnd { exit_code } => {
                                    let _ = reader_app_handle.emit("command_block", CommandBlockEvent {
                                        session_id: reader_session_id.clone(),
                                        command: None,
                                        exit_code: Some(exit_code),
                                        event_type: "command_end".to_string(),
                                    });
                                }
                                OscEvent::DirectoryChanged { path } => {
                                    let _ = reader_app_handle.emit("directory_changed", DirectoryChangedEvent {
                                        session_id: reader_session_id.clone(),
                                        path,
                                    });
                                }
                            }
                        }

                        let output = String::from_utf8_lossy(data).to_string();
                        let _ = reader_app_handle.emit("terminal_output", TerminalOutputEvent {
                            session_id: reader_session_id.clone(),
                            data: output,
                        });
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

    pub fn destroy(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock();
        sessions
            .remove(session_id)
            .ok_or_else(|| QbitError::SessionNotFound(session_id.to_string()))?;
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Result<PtySession> {
        let sessions = self.sessions.lock();
        let session = sessions
            .get(session_id)
            .ok_or_else(|| QbitError::SessionNotFound(session_id.to_string()))?;

        let working_directory = session.working_directory.lock().to_string_lossy().to_string();
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

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

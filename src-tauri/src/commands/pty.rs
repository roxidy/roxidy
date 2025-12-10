use crate::error::Result;
use crate::pty::PtySession;
use crate::runtime::TauriRuntime;
use crate::state::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn pty_create(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    working_directory: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
) -> Result<PtySession> {
    let working_dir = working_directory.map(PathBuf::from);
    let rows = rows.unwrap_or(24);
    let cols = cols.unwrap_or(80);

    // Create TauriRuntime for event emission
    let runtime = Arc::new(TauriRuntime::new(app_handle));

    state
        .pty_manager
        .create_session_with_runtime(runtime, working_dir, rows, cols)
}

#[tauri::command]
pub async fn pty_write(state: State<'_, AppState>, session_id: String, data: String) -> Result<()> {
    state.pty_manager.write(&session_id, data.as_bytes())
}

#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<()> {
    state.pty_manager.resize(&session_id, rows, cols)
}

#[tauri::command]
pub async fn pty_destroy(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state.pty_manager.destroy(&session_id)
}

#[tauri::command]
pub async fn pty_get_session(state: State<'_, AppState>, session_id: String) -> Result<PtySession> {
    state.pty_manager.get_session(&session_id)
}

#[tauri::command]
pub async fn pty_get_foreground_process(state: State<'_, AppState>, session_id: String) -> Result<Option<String>> {
    state.pty_manager.get_foreground_process(&session_id)
}

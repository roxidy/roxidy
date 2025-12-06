//! Tauri commands for settings management.
//!
//! These commands expose the settings system to the frontend, allowing
//! the UI to read and update configuration.

use tauri::State;

use super::schema::QbitSettings;
use crate::state::AppState;

/// Get all settings.
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<QbitSettings, String> {
    Ok(state.settings_manager.get().await)
}

/// Update all settings.
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: QbitSettings,
) -> Result<(), String> {
    state
        .settings_manager
        .update(settings)
        .await
        .map_err(|e| e.to_string())
}

/// Get a specific setting by key (dot notation: "ai.vertex_ai.project_id").
#[tauri::command]
pub async fn get_setting(
    state: State<'_, AppState>,
    key: String,
) -> Result<serde_json::Value, String> {
    state
        .settings_manager
        .get_value(&key)
        .await
        .map_err(|e| e.to_string())
}

/// Set a specific setting by key.
#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    state
        .settings_manager
        .set_value(&key, value)
        .await
        .map_err(|e| e.to_string())
}

/// Reset all settings to defaults.
#[tauri::command]
pub async fn reset_settings(state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings_manager
        .reset()
        .await
        .map_err(|e| e.to_string())
}

/// Check if settings file exists.
#[tauri::command]
pub fn settings_file_exists(state: State<'_, AppState>) -> bool {
    state.settings_manager.exists()
}

/// Get the path to the settings file.
#[tauri::command]
pub fn get_settings_path(state: State<'_, AppState>) -> String {
    state.settings_manager.path().display().to_string()
}

/// Reload settings from disk.
#[tauri::command]
pub async fn reload_settings(state: State<'_, AppState>) -> Result<(), String> {
    state
        .settings_manager
        .reload()
        .await
        .map_err(|e| e.to_string())
}

use crate::error::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct PromptInfo {
    /// Filename without extension (e.g., "refactor")
    pub name: String,
    /// Full path to the file
    pub path: String,
    /// Source: "global" or "local"
    pub source: String,
}

/// List available prompt files from global (~/.qbit/prompts/) and local (.qbit/prompts/) directories.
/// Local prompts override global prompts with the same name.
#[tauri::command]
pub async fn list_prompts(working_directory: Option<String>) -> Result<Vec<PromptInfo>> {
    let mut prompts: HashMap<String, PromptInfo> = HashMap::new();

    // Read global prompts from ~/.qbit/prompts/
    if let Some(home) = dirs::home_dir() {
        let global_dir = home.join(".qbit").join("prompts");
        if global_dir.exists() {
            if let Ok(entries) = fs::read_dir(&global_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            prompts.insert(
                                stem.to_string(),
                                PromptInfo {
                                    name: stem.to_string(),
                                    path: path.to_string_lossy().to_string(),
                                    source: "global".to_string(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    // Read local prompts from {working_directory}/.qbit/prompts/
    // Local prompts override global prompts with the same name
    if let Some(wd) = working_directory {
        let local_dir = PathBuf::from(&wd).join(".qbit").join("prompts");
        if local_dir.exists() {
            if let Ok(entries) = fs::read_dir(&local_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "md") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            prompts.insert(
                                stem.to_string(),
                                PromptInfo {
                                    name: stem.to_string(),
                                    path: path.to_string_lossy().to_string(),
                                    source: "local".to_string(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    // Convert to sorted vector
    let mut result: Vec<PromptInfo> = prompts.into_values().collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(result)
}

/// Read the content of a prompt file.
#[tauri::command]
pub async fn read_prompt(path: String) -> Result<String> {
    let content = fs::read_to_string(&path)?;
    Ok(content)
}

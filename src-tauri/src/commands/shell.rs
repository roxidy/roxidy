use crate::error::{Result, QbitError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const INTEGRATION_VERSION: &str = "1.0.0";

const INTEGRATION_SCRIPT: &str = r#"# ~/.config/qbit/integration.zsh
# Qbit Shell Integration v1.0.0
# Do not edit - managed by Qbit

# Guard against double-sourcing
[[ -n "$QBIT_INTEGRATION_LOADED" ]] && return
export QBIT_INTEGRATION_LOADED=1

# Only run inside Qbit
[[ -z "$QBIT" ]] && return

# ============ OSC Helpers ============

__qbit_osc() {
    printf '\e]133;%s\e\\' "$1"
}

__qbit_report_cwd() {
    printf '\e]7;file://%s%s\e\\' "${HOST:-$(hostname)}" "$PWD"
}

__qbit_notify() {
    printf '\e]9;%s\e\\' "$1"
}

# ============ Prompt Markers ============

__qbit_prompt_start() {
    __qbit_osc "A"
}

__qbit_prompt_end() {
    __qbit_osc "B"
}

__qbit_cmd_start() {
    local cmd="$1"
    if [[ -n "$cmd" ]]; then
        __qbit_osc "C;$cmd"
    else
        __qbit_osc "C"
    fi
    QBIT_CMD_START=$EPOCHREALTIME
}

__qbit_cmd_end() {
    local exit_code=${1:-0}
    __qbit_osc "D;$exit_code"

    if [[ -n "$QBIT_CMD_START" ]]; then
        local duration=$(( ${EPOCHREALTIME%.*} - ${QBIT_CMD_START%.*} ))
        if (( duration > 10 )); then
            __qbit_notify "Command finished (${duration}s)"
        fi
    fi
    unset QBIT_CMD_START
}

# ============ Hook Functions ============

__qbit_preexec() {
    __qbit_cmd_start "$1"
}

__qbit_precmd() {
    local exit_code=$?
    __qbit_cmd_end $exit_code
    __qbit_report_cwd
    __qbit_prompt_start
}

__qbit_line_init() {
    __qbit_prompt_end
}

# ============ Register Hooks ============

autoload -Uz add-zsh-hook

add-zsh-hook -d preexec __qbit_preexec 2>/dev/null
add-zsh-hook -d precmd __qbit_precmd 2>/dev/null

add-zsh-hook preexec __qbit_preexec
add-zsh-hook precmd __qbit_precmd

if [[ -o zle ]]; then
    if (( ${+functions[zle-line-init]} )); then
        functions[__qbit_orig_zle_line_init]="${functions[zle-line-init]}"
        zle-line-init() {
            __qbit_orig_zle_line_init
            __qbit_line_init
        }
    else
        zle-line-init() {
            __qbit_line_init
        }
    fi
    zle -N zle-line-init
fi

__qbit_report_cwd
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IntegrationStatus {
    NotInstalled,
    Installed { version: String },
    Outdated { current: String, latest: String },
}

fn get_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("qbit"))
}

fn get_integration_path() -> Option<PathBuf> {
    get_config_dir().map(|p| p.join("integration.zsh"))
}

fn get_version_path() -> Option<PathBuf> {
    get_config_dir().map(|p| p.join("integration.version"))
}

#[tauri::command]
pub async fn shell_integration_status() -> Result<IntegrationStatus> {
    let version_path = get_version_path()
        .ok_or_else(|| QbitError::Internal("Could not determine config directory".into()))?;

    if !version_path.exists() {
        return Ok(IntegrationStatus::NotInstalled);
    }

    let current_version = fs::read_to_string(&version_path)
        .map_err(|e| QbitError::Io(e))?
        .trim()
        .to_string();

    if current_version == INTEGRATION_VERSION {
        Ok(IntegrationStatus::Installed {
            version: current_version,
        })
    } else {
        Ok(IntegrationStatus::Outdated {
            current: current_version,
            latest: INTEGRATION_VERSION.to_string(),
        })
    }
}

#[tauri::command]
pub async fn shell_integration_install() -> Result<()> {
    let config_dir = get_config_dir()
        .ok_or_else(|| QbitError::Internal("Could not determine config directory".into()))?;

    // Create config directory
    fs::create_dir_all(&config_dir).map_err(|e| QbitError::Io(e))?;

    // Write integration script
    let script_path = config_dir.join("integration.zsh");
    fs::write(&script_path, INTEGRATION_SCRIPT).map_err(|e| QbitError::Io(e))?;

    // Write version marker
    let version_path = config_dir.join("integration.version");
    fs::write(&version_path, INTEGRATION_VERSION).map_err(|e| QbitError::Io(e))?;

    // Update .zshrc
    update_zshrc()?;

    Ok(())
}

#[tauri::command]
pub async fn shell_integration_uninstall() -> Result<()> {
    let config_dir = get_config_dir()
        .ok_or_else(|| QbitError::Internal("Could not determine config directory".into()))?;

    let script_path = config_dir.join("integration.zsh");
    let version_path = config_dir.join("integration.version");

    if script_path.exists() {
        fs::remove_file(&script_path).map_err(|e| QbitError::Io(e))?;
    }
    if version_path.exists() {
        fs::remove_file(&version_path).map_err(|e| QbitError::Io(e))?;
    }

    Ok(())
}

fn update_zshrc() -> Result<()> {
    let zshrc_path = dirs::home_dir()
        .ok_or_else(|| QbitError::Internal("Could not determine home directory".into()))?
        .join(".zshrc");

    let source_line = r#"
# Qbit shell integration
[[ -n "$QBIT" ]] && source ~/.config/qbit/integration.zsh
"#;

    // Check if already present
    if zshrc_path.exists() {
        let content = fs::read_to_string(&zshrc_path).map_err(|e| QbitError::Io(e))?;
        if content.contains("qbit/integration.zsh") {
            return Ok(());
        }
    }

    // Append to .zshrc
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc_path)
        .map_err(|e| QbitError::Io(e))?;

    writeln!(file, "{}", source_line).map_err(|e| QbitError::Io(e))?;

    Ok(())
}

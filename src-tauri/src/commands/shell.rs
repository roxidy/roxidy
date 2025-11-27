use crate::error::{Result, RoxidyError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const INTEGRATION_VERSION: &str = "1.0.0";

const INTEGRATION_SCRIPT: &str = r#"# ~/.config/roxidy/integration.zsh
# Roxidy Shell Integration v1.0.0
# Do not edit - managed by Roxidy

# Guard against double-sourcing
[[ -n "$ROXIDY_INTEGRATION_LOADED" ]] && return
export ROXIDY_INTEGRATION_LOADED=1

# Only run inside Roxidy
[[ -z "$ROXIDY" ]] && return

# ============ OSC Helpers ============

__roxidy_osc() {
    printf '\e]133;%s\e\\' "$1"
}

__roxidy_report_cwd() {
    printf '\e]7;file://%s%s\e\\' "${HOST:-$(hostname)}" "$PWD"
}

__roxidy_notify() {
    printf '\e]9;%s\e\\' "$1"
}

# ============ Prompt Markers ============

__roxidy_prompt_start() {
    __roxidy_osc "A"
}

__roxidy_prompt_end() {
    __roxidy_osc "B"
}

__roxidy_cmd_start() {
    local cmd="$1"
    if [[ -n "$cmd" ]]; then
        __roxidy_osc "C;$cmd"
    else
        __roxidy_osc "C"
    fi
    ROXIDY_CMD_START=$EPOCHREALTIME
}

__roxidy_cmd_end() {
    local exit_code=${1:-0}
    __roxidy_osc "D;$exit_code"

    if [[ -n "$ROXIDY_CMD_START" ]]; then
        local duration=$(( ${EPOCHREALTIME%.*} - ${ROXIDY_CMD_START%.*} ))
        if (( duration > 10 )); then
            __roxidy_notify "Command finished (${duration}s)"
        fi
    fi
    unset ROXIDY_CMD_START
}

# ============ Hook Functions ============

__roxidy_preexec() {
    __roxidy_cmd_start "$1"
}

__roxidy_precmd() {
    local exit_code=$?
    __roxidy_cmd_end $exit_code
    __roxidy_report_cwd
    __roxidy_prompt_start
}

__roxidy_line_init() {
    __roxidy_prompt_end
}

# ============ Register Hooks ============

autoload -Uz add-zsh-hook

add-zsh-hook -d preexec __roxidy_preexec 2>/dev/null
add-zsh-hook -d precmd __roxidy_precmd 2>/dev/null

add-zsh-hook preexec __roxidy_preexec
add-zsh-hook precmd __roxidy_precmd

if [[ -o zle ]]; then
    if (( ${+functions[zle-line-init]} )); then
        functions[__roxidy_orig_zle_line_init]="${functions[zle-line-init]}"
        zle-line-init() {
            __roxidy_orig_zle_line_init
            __roxidy_line_init
        }
    else
        zle-line-init() {
            __roxidy_line_init
        }
    fi
    zle -N zle-line-init
fi

__roxidy_report_cwd
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IntegrationStatus {
    NotInstalled,
    Installed { version: String },
    Outdated { current: String, latest: String },
}

fn get_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("roxidy"))
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
        .ok_or_else(|| RoxidyError::Internal("Could not determine config directory".into()))?;

    if !version_path.exists() {
        return Ok(IntegrationStatus::NotInstalled);
    }

    let current_version = fs::read_to_string(&version_path)
        .map_err(|e| RoxidyError::Io(e))?
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
        .ok_or_else(|| RoxidyError::Internal("Could not determine config directory".into()))?;

    // Create config directory
    fs::create_dir_all(&config_dir).map_err(|e| RoxidyError::Io(e))?;

    // Write integration script
    let script_path = config_dir.join("integration.zsh");
    fs::write(&script_path, INTEGRATION_SCRIPT).map_err(|e| RoxidyError::Io(e))?;

    // Write version marker
    let version_path = config_dir.join("integration.version");
    fs::write(&version_path, INTEGRATION_VERSION).map_err(|e| RoxidyError::Io(e))?;

    // Update .zshrc
    update_zshrc()?;

    Ok(())
}

#[tauri::command]
pub async fn shell_integration_uninstall() -> Result<()> {
    let config_dir = get_config_dir()
        .ok_or_else(|| RoxidyError::Internal("Could not determine config directory".into()))?;

    let script_path = config_dir.join("integration.zsh");
    let version_path = config_dir.join("integration.version");

    if script_path.exists() {
        fs::remove_file(&script_path).map_err(|e| RoxidyError::Io(e))?;
    }
    if version_path.exists() {
        fs::remove_file(&version_path).map_err(|e| RoxidyError::Io(e))?;
    }

    Ok(())
}

fn update_zshrc() -> Result<()> {
    let zshrc_path = dirs::home_dir()
        .ok_or_else(|| RoxidyError::Internal("Could not determine home directory".into()))?
        .join(".zshrc");

    let source_line = r#"
# Roxidy shell integration
[[ -n "$ROXIDY" ]] && source ~/.config/roxidy/integration.zsh
"#;

    // Check if already present
    if zshrc_path.exists() {
        let content = fs::read_to_string(&zshrc_path).map_err(|e| RoxidyError::Io(e))?;
        if content.contains("roxidy/integration.zsh") {
            return Ok(());
        }
    }

    // Append to .zshrc
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc_path)
        .map_err(|e| RoxidyError::Io(e))?;

    writeln!(file, "{}", source_line).map_err(|e| RoxidyError::Io(e))?;

    Ok(())
}

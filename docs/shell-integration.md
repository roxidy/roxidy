# Shell Integration

This document covers how Roxidy installs and manages shell integration for semantic terminal features.

## Overview

Shell integration enables Roxidy to understand command boundaries, exit codes, and working directory changes. Without it, Roxidy still works as a terminal but cannot create command blocks or provide AI context.

## Integration Script Location

```
~/.config/roxidy/
├── integration.zsh      # Main integration script
└── integration.version  # Version marker for updates
```

## The Integration Script

```zsh
# ~/.config/roxidy/integration.zsh
# Roxidy Shell Integration v1.0.0
# Do not edit - managed by Roxidy

# Guard against double-sourcing
[[ -n "$ROXIDY_INTEGRATION_LOADED" ]] && return
export ROXIDY_INTEGRATION_LOADED=1

# Only run inside Roxidy
[[ -z "$ROXIDY" ]] && return

# ============ OSC Helpers ============

# OSC 133 - Semantic prompts (FinalTerm specification)
# Used by iTerm2, VSCode, Warp, and now Roxidy
__roxidy_osc() {
    printf '\e]133;%s\e\\' "$1"
}

# OSC 7 - Current working directory
__roxidy_report_cwd() {
    printf '\e]7;file://%s%s\e\\' "${HOST:-$(hostname)}" "$PWD"
}

# OSC 9 - Notification (optional, for long-running command alerts)
__roxidy_notify() {
    printf '\e]9;%s\e\\' "$1"
}

# ============ Prompt Markers ============

# A - Prompt starts (before PS1)
__roxidy_prompt_start() {
    __roxidy_osc "A"
}

# B - Prompt ends (user can type)
__roxidy_prompt_end() {
    __roxidy_osc "B"
}

# C - Command starts executing (with command text)
# Format: OSC 133 ; C ; <command> ST
__roxidy_cmd_start() {
    local cmd="$1"
    if [[ -n "$cmd" ]]; then
        __roxidy_osc "C;$cmd"
    else
        __roxidy_osc "C"
    fi
    ROXIDY_CMD_START=$EPOCHREALTIME
}

# D - Command finished with exit code
# NOTE: Exit code must be passed explicitly to avoid $? being clobbered
__roxidy_cmd_end() {
    local exit_code=${1:-0}
    __roxidy_osc "D;$exit_code"

    # Notify on long-running commands (>10s)
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
    # Called just before command execution
    # $1 contains the command line in zsh
    __roxidy_cmd_start "$1"
}

__roxidy_precmd() {
    # Called before each prompt
    # IMPORTANT: Capture exit code FIRST before any other operations
    local exit_code=$?

    # End previous command (if any) - pass exit code explicitly
    __roxidy_cmd_end $exit_code

    # Report current directory
    __roxidy_report_cwd

    # Start new prompt
    __roxidy_prompt_start
}

__roxidy_line_init() {
    # Called when ZLE initializes (after prompt rendered)
    __roxidy_prompt_end
}

# ============ Register Hooks ============

autoload -Uz add-zsh-hook

# Remove any existing hooks to avoid duplicates
add-zsh-hook -d preexec __roxidy_preexec 2>/dev/null
add-zsh-hook -d precmd __roxidy_precmd 2>/dev/null

# Add our hooks
add-zsh-hook preexec __roxidy_preexec
add-zsh-hook precmd __roxidy_precmd

# ZLE hook for prompt end timing
# This ensures B is sent AFTER the prompt is fully rendered
if [[ -o zle ]]; then
    # Wrap existing zle-line-init if present
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

# ============ Initial State ============

# Send initial CWD
__roxidy_report_cwd
```

## Installation Process

### Rust Implementation

```rust
// src-tauri/src/shell/integration.rs

use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;

const INTEGRATION_VERSION: &str = "1.0.0";
const INTEGRATION_SCRIPT: &str = include_str!("integration.zsh");

#[derive(Debug, Clone, serde::Serialize)]
pub enum IntegrationStatus {
    NotInstalled,
    Installed { version: String },
    Outdated { current: String, latest: String },
}

pub fn get_config_dir() -> Option<PathBuf> {
    // ~/.config/roxidy/
    dirs::config_dir().map(|p| p.join("roxidy"))
}

pub fn get_integration_path() -> Option<PathBuf> {
    get_config_dir().map(|p| p.join("integration.zsh"))
}

pub fn get_version_path() -> Option<PathBuf> {
    get_config_dir().map(|p| p.join("integration.version"))
}

pub fn check_status() -> anyhow::Result<IntegrationStatus> {
    let version_path = get_version_path().ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    if !version_path.exists() {
        return Ok(IntegrationStatus::NotInstalled);
    }

    let current_version = fs::read_to_string(&version_path)?.trim().to_string();

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

pub fn install() -> anyhow::Result<()> {
    let config_dir = get_config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    // Create config directory
    fs::create_dir_all(&config_dir)?;

    // Write integration script
    let script_path = config_dir.join("integration.zsh");
    fs::write(&script_path, INTEGRATION_SCRIPT)?;

    // Write version marker
    let version_path = config_dir.join("integration.version");
    fs::write(&version_path, INTEGRATION_VERSION)?;

    // Update .zshrc if needed
    update_zshrc()?;

    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let config_dir = get_config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    // Remove integration files
    let script_path = config_dir.join("integration.zsh");
    let version_path = config_dir.join("integration.version");

    if script_path.exists() {
        fs::remove_file(&script_path)?;
    }
    if version_path.exists() {
        fs::remove_file(&version_path)?;
    }

    // Note: We don't remove the .zshrc line automatically
    // as it's guarded by [[ -n "$ROXIDY" ]] and won't run outside Roxidy

    Ok(())
}

fn update_zshrc() -> anyhow::Result<()> {
    let zshrc_path = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
        .join(".zshrc");

    let source_line = r#"
# Roxidy shell integration
[[ -n "$ROXIDY" ]] && source ~/.config/roxidy/integration.zsh
"#;

    // Check if already present
    if zshrc_path.exists() {
        let content = fs::read_to_string(&zshrc_path)?;
        if content.contains("roxidy/integration.zsh") {
            // Already installed
            return Ok(());
        }
    }

    // Append to .zshrc
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&zshrc_path)?;

    use std::io::Write;
    writeln!(file, "{}", source_line)?;

    Ok(())
}
```

## Spawning Shell with Integration

When creating a PTY, set the `ROXIDY` environment variable:

```rust
// src-tauri/src/pty/manager.rs

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

pub fn spawn_shell(working_dir: Option<PathBuf>) -> anyhow::Result<PtyPair> {
    let pty_system = native_pty_system();

    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new("zsh");

    // Signal to shell integration that we're in Roxidy
    cmd.env("ROXIDY", "1");
    cmd.env("ROXIDY_VERSION", env!("CARGO_PKG_VERSION"));

    // Set working directory
    if let Some(dir) = working_dir {
        cmd.cwd(dir);
    }

    // Use login shell for full environment
    cmd.args(&["-l"]);

    let child = pair.slave.spawn_command(cmd)?;

    Ok(PtyPair {
        master: pair.master,
        child,
    })
}
```

## Frontend: Installation UI

```tsx
// src/components/Settings/ShellIntegration.tsx

import { useState, useEffect } from "react";
import {
  shellIntegrationStatus,
  shellIntegrationInstall,
  shellIntegrationUninstall,
} from "../../lib/tauri";

type Status =
  | { type: "loading" }
  | { type: "notInstalled" }
  | { type: "installed"; version: string }
  | { type: "outdated"; current: string; latest: string };

export function ShellIntegration() {
  const [status, setStatus] = useState<Status>({ type: "loading" });
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    checkStatus();
  }, []);

  async function checkStatus() {
    try {
      const result = await shellIntegrationStatus();
      if ("Installed" in result) {
        setStatus({ type: "installed", version: result.Installed.version });
      } else if ("Outdated" in result) {
        setStatus({
          type: "outdated",
          current: result.Outdated.current,
          latest: result.Outdated.latest,
        });
      } else {
        setStatus({ type: "notInstalled" });
      }
    } catch (e) {
      console.error("Failed to check shell integration status:", e);
    }
  }

  async function handleInstall() {
    setInstalling(true);
    try {
      await shellIntegrationInstall();
      await checkStatus();
    } finally {
      setInstalling(false);
    }
  }

  async function handleUninstall() {
    setInstalling(true);
    try {
      await shellIntegrationUninstall();
      await checkStatus();
    } finally {
      setInstalling(false);
    }
  }

  return (
    <div className="shell-integration">
      <h3>Shell Integration</h3>
      <p>
        Shell integration enables command blocks, exit code detection, and
        AI context awareness.
      </p>

      {status.type === "loading" && <p>Checking status...</p>}

      {status.type === "notInstalled" && (
        <div>
          <p className="status status--warning">Not installed</p>
          <button onClick={handleInstall} disabled={installing}>
            {installing ? "Installing..." : "Install"}
          </button>
        </div>
      )}

      {status.type === "installed" && (
        <div>
          <p className="status status--success">
            Installed (v{status.version})
          </p>
          <button onClick={handleUninstall} disabled={installing}>
            Uninstall
          </button>
        </div>
      )}

      {status.type === "outdated" && (
        <div>
          <p className="status status--warning">
            Update available: v{status.current} → v{status.latest}
          </p>
          <button onClick={handleInstall} disabled={installing}>
            {installing ? "Updating..." : "Update"}
          </button>
        </div>
      )}

      <details>
        <summary>Manual installation</summary>
        <p>Add this to your <code>~/.zshrc</code>:</p>
        <pre>
          {`[[ -n "$ROXIDY" ]] && source ~/.config/roxidy/integration.zsh`}
        </pre>
      </details>
    </div>
  );
}
```

## First-Run Experience

On first launch, prompt user to install shell integration:

```tsx
// src/components/Onboarding/ShellSetup.tsx

export function ShellSetup({ onComplete }: { onComplete: () => void }) {
  const [status, setStatus] = useState<"pending" | "installing" | "done">("pending");

  async function handleInstall() {
    setStatus("installing");
    await shellIntegrationInstall();
    setStatus("done");
    onComplete();
  }

  function handleSkip() {
    onComplete();
  }

  return (
    <div className="onboarding-modal">
      <h2>Enable Shell Integration</h2>
      <p>
        Roxidy works best with shell integration, which enables:
      </p>
      <ul>
        <li>Command blocks with exit codes</li>
        <li>Execution time tracking</li>
        <li>AI understanding of your commands</li>
        <li>Working directory awareness</li>
      </ul>

      <div className="actions">
        <button
          onClick={handleInstall}
          disabled={status === "installing"}
          className="primary"
        >
          {status === "installing" ? "Installing..." : "Install Integration"}
        </button>
        <button onClick={handleSkip} className="secondary">
          Skip for now
        </button>
      </div>

      <p className="note">
        This adds a single line to your ~/.zshrc that only activates inside Roxidy.
      </p>
    </div>
  );
}
```

## Troubleshooting

### Integration not working

1. **Check environment variable**: Run `echo $ROXIDY` in Roxidy - should print `1`
2. **Check sourcing**: Run `type __roxidy_preexec` - should show function definition
3. **Check hooks**: Run `add-zsh-hook -L` - should show roxidy hooks

### Conflicts with other tools

The integration is designed to coexist with:
- Oh My Zsh
- Powerlevel10k
- Starship
- Other prompt themes

If conflicts occur, ensure Roxidy integration is sourced **after** other tools in `.zshrc`.

### Performance impact

The integration adds minimal overhead:
- 4 `printf` calls per command (A, B, C, D markers)
- C marker includes command text (from zsh's preexec $1)
- 1 `printf` for CWD after each command
- No external process spawning

## OSC 133 Extensions

Roxidy extends the standard OSC 133 protocol:

| Marker | Standard | Roxidy Extension |
|--------|----------|------------------|
| A | Prompt start | (unchanged) |
| B | Prompt end | (unchanged) |
| C | Command start | `C;command_text` - includes the executed command |
| D | Command end | `D;exit_code` - (unchanged) |

This extension is backwards-compatible - terminals that don't understand the command text will simply ignore it.

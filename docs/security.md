# Security Considerations

This document covers security considerations for Roxidy, including API key storage, sensitive data handling, and AI tool approval.

## API Key Storage

### Current Approach

API keys for AI providers are stored using a layered approach:

1. **Environment Variables (Recommended)**
   - Most secure option - keys never touch disk
   - Set `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `OPENROUTER_API_KEY`
   - Works well for development and power users

2. **SQLite Settings Table (Fallback)**
   - Keys stored in SQLite database at app data directory
   - Database file permissions should be user-only (0600)
   - Future: Consider OS keychain integration (see below)

### Future: OS Keychain Integration

For production, consider integrating with OS-specific secure storage:

| Platform | API | Crate |
|----------|-----|-------|
| macOS | Keychain Services | `security-framework` |
| Windows | Credential Manager | `winapi` / `windows` |
| Linux | Secret Service (GNOME Keyring, KWallet) | `secret-service` |

```rust
// Future implementation sketch
#[cfg(target_os = "macos")]
fn store_api_key(provider: &str, key: &str) -> Result<()> {
    use security_framework::passwords::set_generic_password;
    set_generic_password("dev.roxidy.roxidy", provider, key.as_bytes())?;
    Ok(())
}
```

### Key Rotation

When a user updates their API key:

1. Validate the new key by making a test request
2. If valid, update the stored key
3. Clear any cached authentication tokens
4. Log the key change (without logging the key itself)

## Command History Sanitization

### Problem

Commands may contain sensitive data:
- `export API_KEY=secret123`
- `mysql -u root -ppassword`
- `curl -H "Authorization: Bearer token"`
- `aws configure` (interactive, but history shows command)

### Mitigation Strategies

#### 1. Pattern Detection

```rust
const SENSITIVE_PATTERNS: &[&str] = &[
    r"(?i)(password|passwd|pwd)\s*[=:]\s*\S+",
    r"(?i)(api[_-]?key|apikey)\s*[=:]\s*\S+",
    r"(?i)(secret|token)\s*[=:]\s*\S+",
    r"(?i)(-p|--password)\s*\S+",
    r"(?i)authorization:\s*(bearer|basic)\s+\S+",
    r"(?i)(aws_access_key|aws_secret)\s*[=:]\s*\S+",
];

pub fn sanitize_command(cmd: &str) -> String {
    let mut result = cmd.to_string();
    for pattern in SENSITIVE_PATTERNS {
        let re = Regex::new(pattern).unwrap();
        result = re.replace_all(&result, "[REDACTED]").to_string();
    }
    result
}
```

#### 2. Storage Levels

| Level | Stored | AI Context | Display |
|-------|--------|------------|---------|
| Full | Original command | Sanitized | Sanitized |
| Sanitized | Sanitized only | Sanitized | Sanitized |
| Excluded | Not stored | Not sent | Not shown |

User setting: `history_sensitivity: "full" | "sanitized" | "paranoid"`

#### 3. Exclude Specific Commands

Allow users to configure commands that should never be stored:

```toml
# ~/.config/roxidy/config.toml
[history]
exclude_patterns = [
    "^export .*KEY",
    "^aws configure",
    "^ssh-add",
    "^gpg --",
]
```

## AI Tool Approval Flow

### Risk Classification

| Tool | Risk Level | Default Approval |
|------|------------|------------------|
| `get_command_history` | Low | Auto-approve |
| `read_file` | Low | Auto-approve |
| `list_directory` | Low | Auto-approve |
| `run_command` | Medium | Confirm for destructive patterns |
| `write_file` | High | Always confirm |
| `edit_file` | High | Always confirm |

### Destructive Command Detection

```rust
const DESTRUCTIVE_PATTERNS: &[&str] = &[
    r"^rm\s",
    r"^sudo\s",
    r"^chmod\s",
    r"^chown\s",
    r"^mv\s+.*\s+/",  // Moving to root paths
    r"^dd\s",
    r">\s*/",         // Redirecting to root paths
    r"^git\s+(push|reset|rebase)",
    r"^docker\s+(rm|rmi|system\s+prune)",
    r"^kubectl\s+delete",
];

pub fn requires_confirmation(tool: &str, args: &serde_json::Value) -> bool {
    match tool {
        "write_file" | "edit_file" => true,
        "run_command" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            DESTRUCTIVE_PATTERNS.iter().any(|p| {
                Regex::new(p).map(|re| re.is_match(cmd)).unwrap_or(false)
            })
        }
        _ => false,
    }
}
```

### Approval UI

```
┌─ AI wants to run a command ─────────────────────────────────┐
│                                                              │
│  $ rm -rf ./build                                           │
│                                                              │
│  ⚠️  This command will delete files permanently.            │
│                                                              │
│  [Allow Once]  [Allow Always]  [Deny]  [Edit Command]       │
└──────────────────────────────────────────────────────────────┘
```

### Trust Levels

Users can configure per-tool trust:

```rust
pub enum ToolTrust {
    AlwaysAsk,      // Always require confirmation
    AskDestructive, // Only ask for destructive operations
    AlwaysAllow,    // Never ask (not recommended for write tools)
}

// User settings
pub struct ToolApprovalSettings {
    pub run_command: ToolTrust,
    pub write_file: ToolTrust,
    pub edit_file: ToolTrust,
    pub read_file: ToolTrust,
    // ...
}
```

## File System Sandboxing

### Current State

AI tools have access to the file system with the same permissions as the Roxidy process. This is intentional for a terminal app, but carries risks.

### Guardrails

#### 1. Path Validation

```rust
pub fn validate_path(base_dir: &Path, requested: &Path) -> Result<PathBuf, SecurityError> {
    let canonical = requested.canonicalize()?;

    // Prevent traversal outside working directory (optional, configurable)
    if !canonical.starts_with(base_dir) {
        return Err(SecurityError::PathTraversal);
    }

    // Block sensitive paths
    const BLOCKED_PATHS: &[&str] = &[
        "/.ssh/",
        "/.gnupg/",
        "/.aws/credentials",
        "/etc/passwd",
        "/etc/shadow",
    ];

    let path_str = canonical.to_string_lossy();
    for blocked in BLOCKED_PATHS {
        if path_str.contains(blocked) {
            return Err(SecurityError::BlockedPath);
        }
    }

    Ok(canonical)
}
```

#### 2. Size Limits

```rust
const MAX_READ_SIZE: usize = 10 * 1024 * 1024;  // 10 MB
const MAX_WRITE_SIZE: usize = 1 * 1024 * 1024;  // 1 MB

pub async fn read_file_safe(path: &Path) -> Result<String, Error> {
    let metadata = tokio::fs::metadata(path).await?;
    if metadata.len() > MAX_READ_SIZE as u64 {
        return Err(Error::FileTooLarge);
    }
    tokio::fs::read_to_string(path).await.map_err(Into::into)
}
```

#### 3. Audit Logging

```rust
pub fn log_tool_execution(tool: &str, args: &serde_json::Value, result: &str) {
    tracing::info!(
        tool = tool,
        args = %args,
        result_preview = &result[..result.len().min(100)],
        "AI tool executed"
    );
}
```

## Network Security

### AI API Communication

- All API calls use HTTPS
- Certificate validation enabled by default
- API keys sent only in headers, never in URLs
- Response data not logged (may contain user content)

### Telemetry (Future)

If telemetry is added:

- Opt-in only, clearly explained
- No command content or file contents
- Only aggregate usage statistics
- User can see exactly what's sent

## Error Handling Security

### Information Disclosure

Error messages should not reveal:

- Full file paths outside working directory
- System information (OS version, paths)
- API keys or tokens (even partial)
- Internal implementation details

```rust
// Bad
return Err(anyhow!("Failed to read /Users/john/.ssh/id_rsa: permission denied"));

// Good
return Err(anyhow!("Failed to read file: permission denied"));
```

## Security Checklist for Implementation

### Before POC

- [ ] API keys read from environment variables
- [ ] Basic path validation for file tools
- [ ] Confirmation dialog for write_file tool

### Before Beta

- [ ] Command history sanitization
- [ ] Destructive command detection
- [ ] Audit logging for AI tool executions
- [ ] Size limits on file operations

### Before Release

- [ ] OS keychain integration for API keys
- [ ] User-configurable trust levels
- [ ] Blocked path configuration
- [ ] Security audit of tool implementations

## Threat Model

### In Scope

| Threat | Mitigation |
|--------|------------|
| API key exposure | Env vars, keychain, encrypted storage |
| Sensitive command in history | Pattern detection, sanitization |
| AI executing destructive commands | Confirmation dialogs, pattern detection |
| AI reading sensitive files | Path blocking, user confirmation |
| AI accessing outside working dir | Path validation (optional sandboxing) |

### Out of Scope (User's Responsibility)

- Malicious shell scripts the user runs manually
- Security of the user's system and shell configuration
- Security of third-party AI providers
- Network-level attacks (MITM, etc.)

## Reporting Security Issues

Security issues should be reported via:
- Email: security@roxidy.dev (once set up)
- GitHub Security Advisories (for the repository)

Do not report security issues in public GitHub issues.

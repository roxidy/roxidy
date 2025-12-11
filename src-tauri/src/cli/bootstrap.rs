//! CLI bootstrap - Initialize the full Qbit stack for CLI usage.
//!
//! This module provides `CliContext` which initializes all the same services
//! as the Tauri GUI application, ensuring feature parity between CLI and GUI.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, RwLock};

use crate::ai::agent_bridge::AgentBridge;
use crate::indexer::IndexerState;
use crate::pty::PtyManager;
use crate::runtime::{CliRuntime, QbitRuntime, RuntimeEvent};
use crate::settings::{get_with_env_fallback, QbitSettings, SettingsManager};
use crate::sidecar::SidecarState;
use crate::tavily::TavilyState;

use super::args::Args;

/// Context for CLI execution containing all initialized services.
///
/// This mirrors the Tauri `AppState` but is owned rather than managed by Tauri.
pub struct CliContext {
    /// Runtime abstraction for event emission
    pub runtime: Arc<dyn QbitRuntime>,

    /// Event receiver for output handling
    pub event_rx: mpsc::UnboundedReceiver<RuntimeEvent>,

    /// Agent bridge (initialized lazily via `ensure_agent`)
    bridge: Arc<RwLock<Option<AgentBridge>>>,

    /// Resolved workspace path
    pub workspace: PathBuf,

    /// Settings manager
    pub settings_manager: Arc<SettingsManager>,

    /// PTY manager for shell execution
    pub pty_manager: Arc<PtyManager>,

    /// Code indexer
    pub indexer_state: Arc<IndexerState>,

    /// Web search
    pub tavily_state: Arc<TavilyState>,

    /// Sidecar context capture
    pub sidecar_state: Arc<SidecarState>,

    /// Command-line arguments
    pub args: Args,
}

impl CliContext {
    /// Get a reference to the agent bridge, if initialized.
    pub async fn bridge(&self) -> tokio::sync::RwLockReadGuard<'_, Option<AgentBridge>> {
        self.bridge.read().await
    }

    /// Get a mutable reference to the agent bridge.
    pub async fn bridge_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, Option<AgentBridge>> {
        self.bridge.write().await
    }

    /// Check if the agent is initialized.
    pub async fn is_agent_initialized(&self) -> bool {
        self.bridge.read().await.is_some()
    }

    /// Graceful shutdown - flush sidecar, end sessions, etc.
    pub async fn shutdown(self) -> Result<()> {
        // Finalize agent session if needed
        if let Some(ref bridge) = *self.bridge.read().await {
            bridge.finalize_session().await;
        }

        // Gracefully shutdown sidecar (waits for processor to flush pending events)
        self.sidecar_state.shutdown();

        // Shutdown the runtime
        if let Err(e) = self.runtime.shutdown().await {
            tracing::warn!("Runtime shutdown error: {}", e);
        }

        Ok(())
    }
}

/// Initialize the CLI context with all services.
///
/// This is the main entry point for CLI initialization, mirroring
/// what happens in the Tauri app's `AppState::new()` and `init_ai_agent`.
pub async fn initialize(args: &Args) -> Result<CliContext> {
    // Install TLS provider (required for rustls 0.23+)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Load .env file if present
    if let Err(e) = dotenvy::dotenv() {
        // Only warn on errors other than file not found
        if !matches!(e, dotenvy::Error::Io(_)) {
            tracing::warn!("Failed to load .env file: {}", e);
        }
    }

    // Set session directory to ~/.qbit/sessions
    if std::env::var_os("VT_SESSION_DIR").is_none() {
        if let Some(home) = dirs::home_dir() {
            let qbit_sessions = home.join(".qbit").join("sessions");
            std::env::set_var("VT_SESSION_DIR", &qbit_sessions);
        }
    }

    // Initialize logging based on verbosity
    let log_level = if args.verbose { "debug" } else { "warn" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(format!("qbit={}", log_level).parse().unwrap()),
        )
        .try_init();

    // Resolve workspace path
    let workspace = args.resolve_workspace()?;

    if args.verbose {
        eprintln!("[cli] Workspace: {}", workspace.display());
    }

    // Load settings
    let settings_manager = Arc::new(
        SettingsManager::new()
            .await
            .context("Failed to initialize settings manager")?,
    );

    // Ensure settings file exists (creates template on first run)
    if let Err(e) = settings_manager.ensure_settings_file().await {
        tracing::warn!("Failed to create settings template: {}", e);
    }

    let settings = settings_manager.get().await;

    if args.verbose {
        eprintln!(
            "[cli] Settings loaded from {}",
            settings_manager.path().display()
        );
        eprintln!("[cli] Default provider: {}", settings.ai.default_provider);
        eprintln!("[cli] Default model: {}", settings.ai.default_model);
    }

    // Create event channel
    let (event_tx, event_rx) = mpsc::unbounded_channel::<RuntimeEvent>();

    // Create CLI runtime
    let runtime: Arc<dyn QbitRuntime> = Arc::new(CliRuntime::new(
        event_tx,
        args.auto_approve,
        args.json,
        args.quiet,
    ));

    // Initialize services
    let pty_manager = Arc::new(PtyManager::new());
    let indexer_state = Arc::new(IndexerState::new());
    let tavily_state = Arc::new(TavilyState::new());
    let sidecar_state = Arc::new(SidecarState::new());

    // Initialize indexer for workspace
    if let Err(e) = indexer_state.initialize(workspace.clone()) {
        tracing::warn!("Failed to initialize indexer: {}", e);
    }

    // Initialize sidecar
    if settings.sidecar.enabled {
        if let Err(e) = sidecar_state.initialize(workspace.clone()).await {
            tracing::warn!("Failed to initialize sidecar: {}", e);
        } else if args.verbose {
            eprintln!("[cli] Sidecar initialized");
        }
    }

    // Initialize the agent bridge
    let bridge = initialize_agent(
        &workspace,
        &settings,
        args,
        runtime.clone(),
        pty_manager.clone(),
        indexer_state.clone(),
        tavily_state.clone(),
        sidecar_state.clone(),
    )
    .await?;

    if args.verbose {
        eprintln!("[cli] Agent initialized successfully");
    }

    Ok(CliContext {
        runtime,
        event_rx,
        bridge: Arc::new(RwLock::new(Some(bridge))),
        workspace,
        settings_manager,
        pty_manager,
        indexer_state,
        tavily_state,
        sidecar_state,
        args: args.clone(),
    })
}

/// Initialize the AI agent bridge with all dependencies.
async fn initialize_agent(
    workspace: &PathBuf,
    settings: &QbitSettings,
    args: &Args,
    runtime: Arc<dyn QbitRuntime>,
    pty_manager: Arc<PtyManager>,
    indexer_state: Arc<IndexerState>,
    tavily_state: Arc<TavilyState>,
    sidecar_state: Arc<SidecarState>,
) -> Result<AgentBridge> {
    // Resolve provider: CLI arg > settings > default
    let provider = args
        .provider
        .clone()
        .unwrap_or_else(|| settings.ai.default_provider.clone());

    // Resolve model: CLI arg > settings > provider-specific default
    let model = args
        .model
        .clone()
        .unwrap_or_else(|| settings.ai.default_model.clone());

    if args.verbose {
        eprintln!("[cli] Provider: {}", provider);
        eprintln!("[cli] Model: {}", model);
    }

    // Create the agent bridge based on provider
    let mut bridge = match provider.as_str() {
        "vertex_ai" | "vertex" => {
            // Vertex AI requires credentials file, project ID, and location
            let creds_path = settings
                .ai
                .vertex_ai
                .credentials_path
                .clone()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Vertex AI requires 'ai.vertex_ai.credentials_path' in settings.toml"
                    )
                })?;

            let project_id = settings.ai.vertex_ai.project_id.clone().ok_or_else(|| {
                anyhow::anyhow!("Vertex AI requires 'ai.vertex_ai.project_id' in settings.toml")
            })?;

            let location = settings
                .ai
                .vertex_ai
                .location
                .clone()
                .unwrap_or_else(|| "us-east5".to_string());

            if args.verbose {
                eprintln!("[cli] Vertex AI credentials: {}", creds_path);
                eprintln!("[cli] Vertex AI project: {}", project_id);
                eprintln!("[cli] Vertex AI location: {}", location);
            }

            AgentBridge::new_vertex_anthropic_with_runtime(
                workspace.clone(),
                &creds_path,
                &project_id,
                &location,
                &model,
                runtime,
            )
            .await?
        }
        _ => {
            // API key-based providers (openrouter, anthropic, openai, etc.)
            let api_key = resolve_api_key(settings, &provider, args)?;

            AgentBridge::new_with_runtime(workspace.clone(), &provider, &model, &api_key, runtime)
                .await?
        }
    };

    // Inject dependencies (same as init_ai_agent command in Tauri)
    bridge.set_pty_manager(pty_manager);
    bridge.set_indexer_state(indexer_state);
    bridge.set_tavily_state(tavily_state);
    bridge.set_sidecar_state(sidecar_state);

    Ok(bridge)
}

/// Resolve API key from CLI args, settings, or environment variables.
fn resolve_api_key(settings: &QbitSettings, provider: &str, args: &Args) -> Result<String> {
    // 1. CLI argument takes precedence
    if let Some(ref key) = args.api_key {
        return Ok(key.clone());
    }

    // 2. Check settings based on provider
    let from_settings = match provider {
        "openrouter" => get_with_env_fallback(
            &settings.ai.openrouter.api_key,
            &["OPENROUTER_API_KEY"],
            None,
        ),
        "anthropic" => {
            get_with_env_fallback(&settings.ai.anthropic.api_key, &["ANTHROPIC_API_KEY"], None)
        }
        "openai" => get_with_env_fallback(&settings.ai.openai.api_key, &["OPENAI_API_KEY"], None),
        _ => None,
    };

    from_settings.ok_or_else(|| {
        anyhow::anyhow!(
            "No API key found for provider '{}'. Set it in ~/.qbit/settings.toml, \
             via environment variable, or use --api-key",
            provider
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_api_key_from_args() {
        let settings = QbitSettings::default();
        let mut args = Args::parse_from(["qbit-cli"]);
        args.api_key = Some("test-key".to_string());

        let key = resolve_api_key(&settings, "openrouter", &args).unwrap();
        assert_eq!(key, "test-key");
    }

    // Helper to create Args for testing
    use clap::Parser;
}

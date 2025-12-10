mod ai;
mod error;
mod indexer;
mod pty;
pub mod runtime;
mod settings;
mod sidecar;
#[cfg(feature = "tauri")]
mod state;
mod tavily;
mod web_fetch;

// CLI module (only compiled when cli feature is enabled)
#[cfg(feature = "cli")]
pub mod cli;

// Tauri-specific modules and commands (only compiled when tauri feature is enabled)
#[cfg(feature = "tauri")]
mod commands;

#[cfg(feature = "tauri")]
use ai::{
    add_tool_always_allow, cancel_workflow, clear_ai_conversation, disable_full_auto_mode,
    disable_loop_detection, enable_full_auto_mode, enable_loop_detection, enforce_context_window,
    execute_ai_tool, export_ai_session_transcript, finalize_ai_session, find_ai_session,
    get_ai_conversation_length, get_approval_patterns, get_available_tools, get_context_summary,
    get_context_trim_config, get_context_utilization, get_hitl_config, get_loop_detector_stats,
    get_loop_protection_config, get_openrouter_api_key, get_remaining_tokens,
    get_token_alert_level, get_token_usage_stats, get_tool_approval_pattern, get_tool_policy,
    get_tool_policy_config, get_vertex_ai_config, get_workflow_state, init_ai_agent,
    init_ai_agent_vertex, is_ai_initialized, is_ai_session_persistence_enabled,
    is_context_management_enabled, is_full_auto_mode_enabled, is_loop_detection_enabled,
    list_ai_sessions, list_sub_agents, list_workflow_sessions, list_workflows, load_ai_session,
    load_env_file, remove_tool_always_allow, reset_approval_patterns, reset_context_manager,
    reset_loop_detector, reset_tool_policies, respond_to_tool_approval, restore_ai_session,
    run_workflow_to_completion, send_ai_prompt, set_ai_session_persistence, set_hitl_config,
    set_loop_protection_config, set_tool_policy, set_tool_policy_config, shutdown_ai_agent,
    start_workflow, step_workflow, update_ai_workspace,
};
#[cfg(feature = "tauri")]
use commands::*;
#[cfg(feature = "tauri")]
use indexer::{
    analyze_file, detect_language, extract_symbols, get_file_metrics, get_indexed_file_count,
    get_indexer_workspace, index_directory, index_file, init_indexer, is_indexer_initialized,
    search_code, search_files, shutdown_indexer,
};
#[cfg(feature = "tauri")]
use settings::{
    get_setting, get_settings, get_settings_path, reload_settings, reset_settings, set_setting,
    settings_file_exists, update_settings,
};
#[cfg(feature = "tauri")]
use sidecar::{
    // Cross-session Layer1 query commands
    layer1_get_decisions_by_category,
    layer1_get_state_history,
    layer1_get_unresolved_errors,
    layer1_list_sessions,
    layer1_search_goals,
    layer1_search_similar_decisions,
    layer1_search_similar_errors,
    // Existing sidecar commands
    sidecar_answer_question,
    sidecar_available_backends,
    sidecar_cleanup,
    sidecar_clear_commit_boundary,
    sidecar_complete_goal,
    sidecar_create_indexes,
    sidecar_current_session,
    sidecar_download_models,
    sidecar_end_session,
    sidecar_export_session,
    sidecar_export_session_to_file,
    sidecar_generate_commit,
    sidecar_generate_summary,
    sidecar_get_config,
    sidecar_get_decisions,
    sidecar_get_errors,
    sidecar_get_file_contexts,
    sidecar_get_goals,
    sidecar_get_injectable_context,
    sidecar_get_open_questions,
    sidecar_get_session_checkpoints,
    sidecar_get_session_events,
    sidecar_get_session_state,
    sidecar_import_session,
    sidecar_import_session_from_file,
    sidecar_index_status,
    sidecar_initialize,
    sidecar_list_sessions,
    sidecar_models_status,
    sidecar_pending_files,
    sidecar_query_history,
    sidecar_search_events,
    sidecar_set_backend,
    sidecar_set_config,
    sidecar_shutdown,
    sidecar_start_session,
    sidecar_status,
    sidecar_storage_stats,
};
#[cfg(feature = "tauri")]
use state::AppState;
#[cfg(feature = "tauri")]
use tauri::Manager;

/// Tauri application entry point (only available with tauri feature)
#[cfg(feature = "tauri")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install rustls crypto provider (required for rustls 0.23+)
    // This must be done before any TLS operations (e.g., LanceDB, reqwest)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Load .env file from the project root (if it exists)
    // This loads env vars before anything else needs them
    if let Err(e) = dotenvy::dotenv() {
        // Only warn if file doesn't exist - other errors should be reported
        if !matches!(e, dotenvy::Error::Io(_)) {
            eprintln!("Warning: Failed to load .env file: {}", e);
        }
    }

    // Set session directory to ~/.qbit/sessions (instead of default ~/.vtcode/sessions)
    // This env var is read by vtcode-core's session_archive module
    if std::env::var_os("VT_SESSION_DIR").is_none() {
        if let Some(home) = dirs::home_dir() {
            let qbit_sessions = home.join(".qbit").join("sessions");
            std::env::set_var("VT_SESSION_DIR", &qbit_sessions);
        }
    }

    // Initialize logging (use try_init to avoid panic if already initialized)
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("qbit=debug".parse().unwrap()),
        )
        .try_init();

    // Create tokio runtime for async AppState initialization
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let app_state = runtime.block_on(AppState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(|app| {
            // Auto-initialize sidecar and Layer1 at startup
            let state = app.state::<AppState>();
            let settings_manager = state.settings_manager.clone();
            let sidecar_state = state.sidecar_state.clone();
            let app_handle = app.handle().clone();

            // Spawn async initialization (settings access is async)
            tauri::async_runtime::spawn(async move {
                let settings = settings_manager.get().await;

                if !settings.sidecar.enabled {
                    tracing::debug!(
                        "[tauri-setup] Sidecar disabled in settings, skipping initialization"
                    );
                    return;
                }

                // Set app handle for Layer1 event emission to frontend
                sidecar_state.set_app_handle(app_handle);

                // Get workspace path (default to home directory)
                let workspace = std::env::current_dir()
                    .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default());

                tracing::info!(
                    "[tauri-setup] Initializing sidecar for workspace: {:?}",
                    workspace
                );

                // Initialize sidecar storage
                if let Err(e) = sidecar_state.initialize(workspace).await {
                    tracing::warn!("[tauri-setup] Failed to initialize sidecar: {}", e);
                    return;
                }

                // Initialize Layer1 processor
                if let Err(e) = sidecar_state.initialize_layer1().await {
                    tracing::warn!("[tauri-setup] Failed to initialize Layer1 processor: {}", e);
                } else {
                    tracing::info!("[tauri-setup] Sidecar and Layer1 initialized successfully");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // PTY commands
            pty_create,
            pty_write,
            pty_resize,
            pty_destroy,
            pty_get_session,
            pty_get_foreground_process,
            // Shell integration commands
            shell_integration_status,
            shell_integration_install,
            shell_integration_uninstall,
            // AI commands
            init_ai_agent,
            init_ai_agent_vertex,
            send_ai_prompt,
            execute_ai_tool,
            get_available_tools,
            list_sub_agents,
            shutdown_ai_agent,
            is_ai_initialized,
            get_openrouter_api_key,
            get_vertex_ai_config,
            load_env_file,
            update_ai_workspace,
            clear_ai_conversation,
            get_ai_conversation_length,
            // Session persistence commands
            list_ai_sessions,
            find_ai_session,
            load_ai_session,
            export_ai_session_transcript,
            set_ai_session_persistence,
            is_ai_session_persistence_enabled,
            finalize_ai_session,
            restore_ai_session,
            // HITL commands
            get_approval_patterns,
            get_tool_approval_pattern,
            get_hitl_config,
            set_hitl_config,
            add_tool_always_allow,
            remove_tool_always_allow,
            reset_approval_patterns,
            respond_to_tool_approval,
            // Tool policy commands
            get_tool_policy_config,
            set_tool_policy_config,
            get_tool_policy,
            set_tool_policy,
            reset_tool_policies,
            enable_full_auto_mode,
            disable_full_auto_mode,
            is_full_auto_mode_enabled,
            // Context management commands
            get_context_summary,
            get_token_usage_stats,
            get_token_alert_level,
            get_context_utilization,
            get_remaining_tokens,
            enforce_context_window,
            reset_context_manager,
            get_context_trim_config,
            is_context_management_enabled,
            // Loop protection commands
            get_loop_protection_config,
            set_loop_protection_config,
            get_loop_detector_stats,
            is_loop_detection_enabled,
            disable_loop_detection,
            enable_loop_detection,
            reset_loop_detector,
            // Indexer commands
            init_indexer,
            is_indexer_initialized,
            get_indexer_workspace,
            get_indexed_file_count,
            index_file,
            index_directory,
            search_code,
            search_files,
            analyze_file,
            extract_symbols,
            get_file_metrics,
            detect_language,
            shutdown_indexer,
            // Prompt commands
            list_prompts,
            read_prompt,
            // File commands
            list_workspace_files,
            // Theme commands
            list_themes,
            read_theme,
            save_theme,
            delete_theme,
            save_theme_asset,
            get_theme_asset_path,
            // Workflow commands (generic)
            list_workflows,
            start_workflow,
            step_workflow,
            run_workflow_to_completion,
            get_workflow_state,
            list_workflow_sessions,
            cancel_workflow,
            // Settings commands
            get_settings,
            update_settings,
            get_setting,
            set_setting,
            reset_settings,
            settings_file_exists,
            get_settings_path,
            reload_settings,
            // Sidecar commands
            sidecar_status,
            sidecar_initialize,
            sidecar_start_session,
            sidecar_end_session,
            sidecar_current_session,
            sidecar_generate_commit,
            sidecar_generate_summary,
            sidecar_query_history,
            sidecar_search_events,
            sidecar_get_session_events,
            sidecar_get_session_checkpoints,
            sidecar_list_sessions,
            sidecar_storage_stats,
            sidecar_models_status,
            sidecar_download_models,
            sidecar_get_config,
            sidecar_set_config,
            sidecar_shutdown,
            sidecar_export_session,
            sidecar_export_session_to_file,
            sidecar_import_session,
            sidecar_import_session_from_file,
            sidecar_pending_files,
            sidecar_clear_commit_boundary,
            sidecar_cleanup,
            sidecar_index_status,
            sidecar_create_indexes,
            sidecar_set_backend,
            sidecar_available_backends,
            // Layer 1 commands (single session)
            sidecar_get_session_state,
            sidecar_get_injectable_context,
            sidecar_get_goals,
            sidecar_get_file_contexts,
            sidecar_get_decisions,
            sidecar_get_errors,
            sidecar_get_open_questions,
            sidecar_answer_question,
            sidecar_complete_goal,
            // Layer 1 cross-session query commands
            layer1_search_similar_decisions,
            layer1_get_decisions_by_category,
            layer1_get_unresolved_errors,
            layer1_search_similar_errors,
            layer1_list_sessions,
            layer1_search_goals,
            layer1_get_state_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

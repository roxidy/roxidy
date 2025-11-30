mod ai;
mod commands;
mod error;
mod indexer;
mod pty;
mod state;

use ai::{
    add_tool_always_allow, clear_ai_conversation, disable_full_auto_mode, disable_loop_detection,
    enable_full_auto_mode, enable_loop_detection, enforce_context_window, execute_ai_tool,
    export_ai_session_transcript, finalize_ai_session, find_ai_session, get_ai_conversation_length,
    get_approval_patterns, get_available_tools, get_context_summary, get_context_trim_config,
    get_context_utilization, get_hitl_config, get_loop_detector_stats, get_loop_protection_config,
    get_openrouter_api_key, get_remaining_tokens, get_token_alert_level, get_token_usage_stats,
    get_tool_approval_pattern, get_tool_policy, get_tool_policy_config, get_vertex_ai_config,
    init_ai_agent, init_ai_agent_vertex, is_ai_initialized, is_ai_session_persistence_enabled,
    is_context_management_enabled, is_full_auto_mode_enabled, is_loop_detection_enabled,
    list_ai_sessions, load_ai_session, load_env_file, remove_tool_always_allow,
    reset_approval_patterns, reset_context_manager, reset_loop_detector, reset_tool_policies,
    respond_to_tool_approval, restore_ai_session, send_ai_prompt, set_ai_session_persistence,
    set_hitl_config, set_loop_protection_config, set_tool_policy, set_tool_policy_config,
    shutdown_ai_agent, update_ai_workspace,
};
use commands::*;
use indexer::{
    analyze_file, detect_language, extract_symbols, get_file_metrics, get_indexed_file_count,
    get_indexer_workspace, index_directory, index_file, init_indexer, is_indexer_initialized,
    search_code, search_files, shutdown_indexer,
};
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // PTY commands
            pty_create,
            pty_write,
            pty_resize,
            pty_destroy,
            pty_get_session,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

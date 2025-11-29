mod ai;
mod commands;
mod error;
mod pty;
mod state;

use ai::{
    execute_ai_tool, get_available_tools, get_openrouter_api_key, get_vertex_ai_config,
    init_ai_agent, init_ai_agent_vertex, is_ai_initialized, load_env_file, send_ai_prompt,
    shutdown_ai_agent, update_ai_workspace,
};
use commands::*;
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

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("qbit=debug".parse().unwrap()),
        )
        .init();

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

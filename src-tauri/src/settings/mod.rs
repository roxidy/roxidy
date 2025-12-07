//! Centralized TOML-based settings system for Qbit.
//!
//! Settings are loaded from `~/.qbit/settings.toml` with environment variable
//! interpolation support. The system maintains backward compatibility with
//! existing environment variables through the `get_with_env_fallback` helper.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::settings::{SettingsManager, get_with_env_fallback};
//!
//! // Load settings
//! let manager = SettingsManager::new().await?;
//! let settings = manager.get().await;
//!
//! // Get a value with environment variable fallback
//! let api_key = get_with_env_fallback(
//!     &settings.api_keys.tavily,
//!     &["TAVILY_API_KEY"],
//!     None,
//! );
//! ```

#[cfg(feature = "tauri")]
pub mod commands;
pub mod loader;
pub mod schema;

#[cfg(feature = "tauri")]
pub use commands::*;
pub use loader::{get_with_env_fallback, SettingsManager};
// Re-export for external use (e.g., tests, other crates)
#[allow(unused_imports)]
pub use schema::QbitSettings;

//! Sidecar Context Capture System
//!
//! A background system that passively captures session context during Qbit agent
//! interactions using a simple markdown-based storage approach.
//!
//! ## Architecture
//!
//! Each session is stored as a directory in `~/.qbit/sessions/{session_id}/`:
//!
//! ```text
//! ~/.qbit/sessions/{session_id}/
//!   state.md          # YAML frontmatter (metadata) + markdown body (context)
//!   patches/
//!     staged/         # Pending patches in git format-patch style
//!       0001-*.patch  # Patch file
//!       0001-*.meta.toml  # Patch metadata (timestamp, author, files)
//!     applied/        # Applied patches (moved after git am)
//!   artifacts/
//!     pending/        # Proposed documentation updates awaiting review
//!     applied/        # Previously applied artifacts (archived)
//! ```
//!
//! ### state.md
//! Combined metadata and session state in a single file:
//! - YAML frontmatter: session_id, cwd, git info, timestamps, status
//! - Markdown body: LLM-managed context (goals, progress, files in focus)
//!
//! ### patches/
//! Git format-patch style patches for staged commits:
//! - Each patch is a standard .patch file applicable with `git am`
//! - Metadata sidecar files track creation time, author, affected files
//! - Staged patches await user review; applied patches are moved after commit
//!
//! ### artifacts/
//! Auto-maintained project documentation (L3):
//! - Each artifact is a proposed update to README.md, CLAUDE.md, etc.
//! - Pending artifacts await user review; applied artifacts are archived

pub mod artifacts;
pub mod capture;
#[cfg(feature = "tauri")]
pub mod commands;
pub mod commits;
pub mod config;
pub mod events;
pub mod formats;
pub mod processor;
pub mod session;
pub mod state;
pub mod synthesis;

pub use capture::CaptureContext;
#[cfg(feature = "tauri")]
pub use commands::*;
#[allow(unused_imports)]
pub use config::SidecarConfig;
#[allow(unused_imports)]
pub use events::SidecarEvent;
pub use state::SidecarState;

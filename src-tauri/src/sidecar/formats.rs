//! File format utilities for sidecar storage.
//!
//! This module is now minimal since we use:
//! - YAML frontmatter in state.md (parsed via serde_yaml)
//! - Git format-patch files (standard format)

// This module is kept for potential future format utilities
// but most formatting is now handled directly in session.rs and commits.rs

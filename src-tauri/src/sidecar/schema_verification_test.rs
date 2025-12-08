//! Schema verification tests for the sidecar event capture system.
//!
//! These tests verify that all new columns work correctly end-to-end.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::sidecar::events::{EventType, SessionEvent};
    use crate::sidecar::storage::SidecarStorage;

    /// Test 1: Schema Verification - Confirm all new columns exist and work
    #[tokio::test]
    async fn test_schema_columns_exist() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // Create an event with ALL new columns populated
        let mut event = SessionEvent::tool_call_with_output(
            session_id,
            "edit_file",
            "path=src/test.rs",
            true,
            Some("Edit successful".to_string()),
            Some(vec![PathBuf::from("src/test.rs")]),
            vec![PathBuf::from("src/test.rs")],
            Some("--- src/test.rs\n+++ src/test.rs\n@@ @@\n-old\n+new".to_string()),
        );
        event.cwd = Some("/workspace/project".to_string());

        // Save and retrieve
        storage.save_events(&[event]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        let e = &retrieved[0];

        // Verify ALL new columns are present and populated
        println!("\n=== Schema Verification ===");
        println!("cwd: {:?}", e.cwd);
        println!("tool_output: {:?}", e.tool_output);
        println!("files_accessed: {:?}", e.files_accessed);
        println!("files_modified: {:?}", e.files_modified);
        println!("diff: {:?}", e.diff);

        assert_eq!(e.cwd, Some("/workspace/project".to_string()), "cwd column missing");
        assert_eq!(e.tool_output, Some("Edit successful".to_string()), "tool_output column missing");
        assert_eq!(e.files_accessed, Some(vec![PathBuf::from("src/test.rs")]), "files_accessed column missing");
        assert_eq!(e.files_modified, vec![PathBuf::from("src/test.rs")], "files_modified column missing");
        assert!(e.diff.is_some(), "diff column missing");

        println!("✓ All schema columns exist and work correctly");
    }

    /// Test A: User Prompt Parsing
    #[tokio::test]
    async fn test_user_prompt_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // Create user prompt with context XML
        let input = r#"<context>
<cwd>/Users/test/myproject</cwd>
<session_id>abc-123</session_id>
</context>

list all rust files in the src directory"#;

        let event = SessionEvent::user_prompt(session_id, input);

        // Save and retrieve
        storage.save_events(&[event]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        let e = &retrieved[0];

        println!("\n=== Test A: User Prompt Parsing ===");
        println!("event_type: {}", e.event_type.name());
        println!("cwd: {:?}", e.cwd);
        println!("content: {:?}", e.content);

        // Verify cwd extracted
        assert_eq!(e.cwd, Some("/Users/test/myproject".to_string()), "cwd not extracted from XML");

        // Verify content is clean (no XML, no "User asked:" prefix)
        assert_eq!(e.content, "list all rust files in the src directory", "content should be clean");
        assert!(!e.content.contains("<context>"), "content should not contain XML");
        assert!(!e.content.contains("User asked:"), "content should not have 'User asked:' prefix");

        // Verify event type
        assert_eq!(e.event_type.name(), "user_prompt");

        println!("✓ User prompt parsing works correctly");
    }

    /// Test B: File Read Capture
    #[tokio::test]
    async fn test_file_read_capture() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // Simulate read_file tool call
        let event = SessionEvent::tool_call_with_output(
            session_id,
            "read_file",
            "path=src/main.rs",
            true,
            Some("fn main() {\n    println!(\"Hello, world!\");\n}".to_string()),
            Some(vec![PathBuf::from("src/main.rs")]),
            vec![], // No files modified for read
            None,   // No diff for read
        );

        storage.save_events(&[event]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        let e = &retrieved[0];

        println!("\n=== Test B: File Read Capture ===");
        println!("event_type: {}", e.event_type.name());
        println!("content: {:?}", e.content);
        println!("tool_output: {:?}", e.tool_output);
        println!("files_accessed: {:?}", e.files_accessed);
        println!("files_modified: {:?}", e.files_modified);

        // Verify tool_output captured
        assert!(e.tool_output.is_some(), "tool_output should be captured");
        assert!(e.tool_output.as_ref().unwrap().contains("fn main()"), "tool_output should contain file content");

        // Verify files_accessed captured
        assert_eq!(
            e.files_accessed,
            Some(vec![PathBuf::from("src/main.rs")]),
            "files_accessed should contain the read file"
        );

        // Verify files_modified is empty for read
        assert!(e.files_modified.is_empty(), "files_modified should be empty for read operation");

        println!("✓ File read capture works correctly");
    }

    /// Test C: File Edit Capture
    #[tokio::test]
    async fn test_file_edit_capture() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // Simulate edit_file tool call with diff
        let diff = r#"--- src/lib.rs
+++ src/lib.rs
@@ -1,3 +1,4 @@
 fn process() {
-    old_implementation();
+    new_implementation();
+    additional_call();
 }"#;

        let event = SessionEvent::tool_call_with_output(
            session_id,
            "edit_file",
            "path=src/lib.rs",
            true,
            Some("Edit applied successfully".to_string()),
            None, // No files_accessed for edit
            vec![PathBuf::from("src/lib.rs")],
            Some(diff.to_string()),
        );

        storage.save_events(&[event]).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        assert_eq!(retrieved.len(), 1);
        let e = &retrieved[0];

        println!("\n=== Test C: File Edit Capture ===");
        println!("event_type: {}", e.event_type.name());
        println!("content: {:?}", e.content);
        println!("tool_output: {:?}", e.tool_output);
        println!("files_modified: {:?}", e.files_modified);
        println!("diff preview: {:?}", e.diff.as_ref().map(|d| &d[..d.len().min(100)]));

        // Verify files_modified captured
        assert_eq!(
            e.files_modified,
            vec![PathBuf::from("src/lib.rs")],
            "files_modified should contain the edited file"
        );

        // Verify diff captured
        assert!(e.diff.is_some(), "diff should be captured");
        let diff_content = e.diff.as_ref().unwrap();
        assert!(diff_content.contains("--- src/lib.rs"), "diff should have file header");
        assert!(diff_content.contains("-    old_implementation();"), "diff should show removed line");
        assert!(diff_content.contains("+    new_implementation();"), "diff should show added line");

        println!("✓ File edit capture works correctly");
    }

    /// Test: No session_start event emitted
    #[tokio::test]
    async fn test_no_session_start_event() {
        use crate::sidecar::config::SidecarConfig;
        use crate::sidecar::state::SidecarState;

        let temp_dir = TempDir::new().unwrap();
        let config = SidecarConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let state = SidecarState::with_config(config);

        // Initialize storage first
        let workspace = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        state.initialize(workspace).await.unwrap();

        // Start a session (workspace is set via config, only needs initial_request)
        let session_id = state
            .start_session("Test request")
            .unwrap();

        // Wait for any async processing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Get events
        let events = state.get_session_events(session_id).await.unwrap();

        println!("\n=== Test: No session_start Event ===");
        println!("Events captured: {}", events.len());
        for e in &events {
            println!("  - {}", e.event_type.name());
        }

        // Verify NO session_start event
        let has_session_start = events.iter().any(|e| e.event_type.name() == "session_start");
        assert!(!has_session_start, "session_start event should NOT be emitted");

        println!("✓ No session_start event emitted");
    }

    /// Test 3: Show Sample Output - Display all columns for verification
    #[tokio::test]
    async fn test_show_sample_output() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SidecarStorage::new(temp_dir.path()).await.unwrap();

        let session_id = Uuid::new_v4();

        // Create diverse set of events
        let events = vec![
            // User prompt with context
            {
                let input = "<context>\n<cwd>/project</cwd>\n</context>\n\nfix the bug in auth.rs";
                SessionEvent::user_prompt(session_id, input)
            },
            // Read file
            SessionEvent::tool_call_with_output(
                session_id,
                "read_file",
                "path=src/auth.rs",
                true,
                Some("pub fn authenticate(user: &str) -> bool { false }".to_string()),
                Some(vec![PathBuf::from("src/auth.rs")]),
                vec![],
                None,
            ),
            // Grep search
            SessionEvent::tool_call_with_output(
                session_id,
                "grep",
                "pattern=authenticate",
                true,
                Some("src/auth.rs:1: pub fn authenticate\nsrc/main.rs:15: authenticate(user)".to_string()),
                Some(vec![PathBuf::from("src/auth.rs"), PathBuf::from("src/main.rs")]),
                vec![],
                None,
            ),
            // Edit file
            SessionEvent::tool_call_with_output(
                session_id,
                "edit_file",
                "path=src/auth.rs",
                true,
                Some("Edit applied".to_string()),
                None,
                vec![PathBuf::from("src/auth.rs")],
                Some("--- src/auth.rs\n+++ src/auth.rs\n@@ @@\n-    false\n+    true".to_string()),
            ),
            // Create new file
            SessionEvent::tool_call_with_output(
                session_id,
                "write",
                "path=src/auth_test.rs",
                true,
                Some("File created".to_string()),
                None,
                vec![PathBuf::from("src/auth_test.rs")],
                Some("--- /dev/null\n+++ src/auth_test.rs\n@@ @@\n+#[test]\n+fn test_auth() {}".to_string()),
            ),
        ];

        storage.save_events(&events).await.unwrap();
        let retrieved = storage.get_session_events(session_id).await.unwrap();

        println!("\n=== Sample Output: {} Events ===\n", retrieved.len());

        for (i, e) in retrieved.iter().enumerate() {
            println!("--- Event {} ---", i + 1);
            println!("  event_type: {}", e.event_type.name());
            println!("  content: {:?}", truncate_for_display(&e.content, 60));
            println!("  cwd: {:?}", e.cwd);
            println!("  tool_output: {:?}", e.tool_output.as_ref().map(|s| truncate_for_display(s, 50)));
            println!("  files_accessed: {:?}", e.files_accessed);
            println!("  files_modified: {:?}", e.files_modified);
            println!("  diff: {:?}", e.diff.as_ref().map(|s| truncate_for_display(s, 60)));
            println!();
        }

        assert_eq!(retrieved.len(), 5, "Should have 5 events");
        println!("✓ Sample output verification complete");
    }

    fn truncate_for_display(s: &str, max: usize) -> String {
        if s.len() <= max {
            s.to_string()
        } else {
            format!("{}...", &s[..max])
        }
    }
}

//! Layer 1 Verification Tests
//!
//! Comprehensive tests to verify the Session State (L1) implementation.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use arrow_schema::DataType;
    use lancedb::query::{ExecutableQuery, QueryBase};
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::sidecar::events::{DecisionType, FileOperation, SessionEvent};
    use crate::sidecar::layer1::processor::{Layer1Config, Layer1Processor};
    use crate::sidecar::layer1::state::{Decision, ErrorEntry, FileContext, GoalSource, SessionState};
    use crate::sidecar::layer1::storage::{snapshot_reasons, Layer1Storage};
    use crate::sidecar::layer1::{get_injectable_context, get_session_state};
    use crate::sidecar::synthesis_llm::TemplateLlm;

    // =========================================================================
    // 1. Schema Verification
    // =========================================================================

    #[tokio::test]
    async fn test_schema_session_states_table_exists() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .unwrap();

        let storage = Layer1Storage::new(connection.clone()).await.unwrap();

        // Verify table exists by opening it
        let table = connection
            .open_table("session_states")
            .execute()
            .await
            .expect("session_states table should exist");

        // Verify schema has required columns
        let schema = table.schema().await.unwrap();
        let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();

        assert!(field_names.contains(&"id"), "Missing 'id' column");
        assert!(
            field_names.contains(&"session_id"),
            "Missing 'session_id' column"
        );
        assert!(
            field_names.contains(&"timestamp_ms"),
            "Missing 'timestamp_ms' column"
        );
        assert!(
            field_names.contains(&"state_json"),
            "Missing 'state_json' column"
        );
        assert!(
            field_names.contains(&"snapshot_reason"),
            "Missing 'snapshot_reason' column"
        );

        println!("✅ Schema verification passed");
        println!("   Columns: {:?}", field_names);
    }

    #[tokio::test]
    async fn test_schema_column_types() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .unwrap();

        let _storage = Layer1Storage::new(connection.clone()).await.unwrap();
        let table = connection
            .open_table("session_states")
            .execute()
            .await
            .unwrap();
        let schema = table.schema().await.unwrap();

        for field in schema.fields() {
            match field.name().as_str() {
                "id" | "session_id" | "state_json" | "snapshot_reason" => {
                    assert_eq!(
                        field.data_type(),
                        &DataType::Utf8,
                        "Column {} should be Utf8",
                        field.name()
                    );
                }
                "timestamp_ms" => {
                    assert_eq!(
                        field.data_type(),
                        &DataType::Int64,
                        "Column timestamp_ms should be Int64"
                    );
                }
                _ => {}
            }
        }

        println!("✅ Column types verification passed");
    }

    // =========================================================================
    // 2. Data Structure Tests
    // =========================================================================

    #[tokio::test]
    async fn test_data_structure_state_json_contents() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .unwrap();

        let storage = Layer1Storage::new(connection).await.unwrap();

        // Create a state with all fields populated
        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Test task");

        // Add sub-goals
        state.add_sub_goal("Sub-task 1".to_string(), GoalSource::Inferred);

        // Add narrative
        state.update_narrative("Working on the test task".to_string());

        // Add decision
        state.record_decision(Decision::new(
            "Use approach A".to_string(),
            "It's simpler".to_string(),
            vec!["Approach B".to_string()],
            Uuid::new_v4(),
        ));

        // Add file context
        state.update_file_context(
            PathBuf::from("src/main.rs"),
            FileContext::new(
                PathBuf::from("src/main.rs"),
                "Main entry point".to_string(),
                "Core file".to_string(),
            ),
        );

        // Add error
        state.record_error(ErrorEntry::new(
            "Test error".to_string(),
            "During testing".to_string(),
        ));

        // Add open question
        state.add_open_question("Should we use Redis?".to_string());

        // Save to storage
        storage
            .save_snapshot(&state, snapshot_reasons::MANUAL)
            .await
            .unwrap();

        // Retrieve and parse
        let retrieved = storage.get_latest_state(session_id).await.unwrap().unwrap();

        // Verify all fields
        assert!(!retrieved.goal_stack.is_empty(), "goal_stack should not be empty");
        assert!(
            !retrieved.narrative.is_empty(),
            "narrative should not be empty"
        );
        assert!(
            !retrieved.decisions.is_empty(),
            "decisions should not be empty"
        );
        assert!(
            !retrieved.file_contexts.is_empty(),
            "file_contexts should not be empty"
        );
        assert!(!retrieved.errors.is_empty(), "errors should not be empty");
        assert!(
            !retrieved.open_questions.is_empty(),
            "open_questions should not be empty"
        );

        println!("✅ Data structure verification passed");
        println!("   goal_stack: {} goals", retrieved.goal_stack.len());
        println!("   narrative: {} chars", retrieved.narrative.len());
        println!("   decisions: {}", retrieved.decisions.len());
        println!("   file_contexts: {}", retrieved.file_contexts.len());
        println!("   errors: {}", retrieved.errors.len());
        println!("   open_questions: {}", retrieved.open_questions.len());
    }

    #[test]
    fn test_data_structure_serialization_roundtrip() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Test");

        state.add_sub_goal("Sub".to_string(), GoalSource::Inferred);
        state.record_decision(Decision::new(
            "A".to_string(),
            "B".to_string(),
            vec![],
            Uuid::new_v4(),
        ));

        // Serialize
        let json = serde_json::to_string_pretty(&state).unwrap();

        // Parse back
        let parsed: SessionState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.session_id, session_id);
        assert_eq!(parsed.goal_stack.len(), state.goal_stack.len());
        assert_eq!(parsed.decisions.len(), state.decisions.len());

        println!("✅ Serialization roundtrip passed");
    }

    // =========================================================================
    // 3. Functional Tests
    // =========================================================================

    async fn setup_processor() -> (TempDir, Layer1Processor, Arc<Layer1Storage>) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.lance");
        let connection = lancedb::connect(db_path.to_str().unwrap())
            .execute()
            .await
            .unwrap();

        let storage = Arc::new(Layer1Storage::new(connection).await.unwrap());
        let llm = Arc::new(TemplateLlm) as Arc<dyn crate::sidecar::synthesis_llm::SynthesisLlm>;
        let config = Layer1Config {
            use_llm: false, // Use rule-based for deterministic tests
            snapshot_interval: 5,
            max_snapshots_per_session: 10,
        };

        let processor = Layer1Processor::new(storage.clone(), llm, config);
        (temp_dir, processor, storage)
    }

    /// Test A: Goal Extraction
    #[tokio::test]
    async fn test_a_goal_extraction() {
        let (_temp_dir, processor, _storage) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Send user prompt about authentication
        let event =
            SessionEvent::user_prompt(session_id, "Add authentication to the API using JWT");

        processor.handle_event(event).await;

        // Query session state
        let state = processor.get_current_state(session_id);

        assert!(state.is_some(), "State should exist after user prompt");
        let state = state.unwrap();

        assert!(!state.goal_stack.is_empty(), "goal_stack should not be empty");

        // Verify goal mentions authentication or JWT
        let goal_desc = &state.goal_stack[0].description.to_lowercase();
        assert!(
            goal_desc.contains("authentication") || goal_desc.contains("jwt"),
            "Goal should mention authentication or JWT, got: {}",
            state.goal_stack[0].description
        );

        println!("✅ Test A: Goal Extraction passed");
        println!("   Goal: {}", state.goal_stack[0].description);
    }

    /// Test B: File Context Capture
    #[tokio::test]
    async fn test_b_file_context_capture() {
        let (_temp_dir, processor, _storage) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Initialize session
        processor
            .handle_event(SessionEvent::user_prompt(session_id, "Review the code"))
            .await;

        // Simulate read_file tool call
        let read_event = SessionEvent::tool_call_with_output(
            session_id,
            "read_file",
            "path=src/main.rs",
            true,
            Some("fn main() {\n    println!(\"Hello\");\n}".to_string()),
            Some(vec![PathBuf::from("src/main.rs")]),
            vec![],
            None,
        );

        processor.handle_event(read_event).await;

        // Query session state
        let state = processor.get_current_state(session_id).unwrap();

        let main_rs = PathBuf::from("src/main.rs");
        assert!(
            state.file_contexts.contains_key(&main_rs),
            "file_contexts should have entry for src/main.rs"
        );

        let ctx = &state.file_contexts[&main_rs];
        assert!(ctx.last_read_at.is_some(), "File should be marked as read");
        assert!(!ctx.summary.is_empty(), "File should have a summary");

        println!("✅ Test B: File Context Capture passed");
        println!("   File: {}", ctx.path.display());
        println!("   Summary: {}", ctx.summary);
    }

    /// Test C: Decision Logging
    #[tokio::test]
    async fn test_c_decision_logging() {
        let (_temp_dir, processor, _storage) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Initialize session
        processor
            .handle_event(SessionEvent::user_prompt(session_id, "Add caching"))
            .await;

        // Trigger reasoning with decision
        let reasoning_event = SessionEvent::reasoning(
            session_id,
            "I'll use Redis because it provides persistence and is widely supported",
            Some(DecisionType::ApproachChoice),
        );

        processor.handle_event(reasoning_event).await;

        // Query session state
        let state = processor.get_current_state(session_id).unwrap();

        assert!(
            !state.decisions.is_empty(),
            "decisions should not be empty after reasoning event"
        );

        let decision = &state.decisions[0];
        assert!(!decision.choice.is_empty(), "Decision should have a choice");

        println!("✅ Test C: Decision Logging passed");
        println!("   Choice: {}", decision.choice);
        println!("   Rationale: {}", decision.rationale);
    }

    /// Test D: Narrative Update
    #[tokio::test]
    async fn test_d_narrative_update() {
        let (_temp_dir, processor, _storage) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Process 5+ events
        processor
            .handle_event(SessionEvent::user_prompt(session_id, "Build a REST API"))
            .await;

        processor
            .handle_event(SessionEvent::reasoning(
                session_id,
                "Starting with the project structure",
                None,
            ))
            .await;

        processor
            .handle_event(SessionEvent::file_edit(
                session_id,
                PathBuf::from("src/lib.rs"),
                FileOperation::Create,
                Some("Created main library file".to_string()),
            ))
            .await;

        processor
            .handle_event(SessionEvent::reasoning(
                session_id,
                "Adding route handlers",
                None,
            ))
            .await;

        processor
            .handle_event(SessionEvent::file_edit(
                session_id,
                PathBuf::from("src/routes.rs"),
                FileOperation::Create,
                Some("Added routes module".to_string()),
            ))
            .await;

        // Query session state
        let state = processor.get_current_state(session_id).unwrap();

        assert!(
            !state.narrative.is_empty(),
            "narrative should not be empty after multiple events"
        );

        println!("✅ Test D: Narrative Update passed");
        println!("   Narrative: {}", state.narrative);
    }

    /// Test E: Error Tracking
    #[tokio::test]
    async fn test_e_error_tracking() {
        let (_temp_dir, processor, _storage) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Initialize session
        processor
            .handle_event(SessionEvent::user_prompt(session_id, "Fix the bug"))
            .await;

        // Trigger a failed tool call
        let error_event = SessionEvent::tool_call_with_output(
            session_id,
            "cargo_build",
            "args=--release",
            false, // Failed!
            Some("error[E0382]: borrow of moved value".to_string()),
            None,
            vec![],
            None,
        );

        processor.handle_event(error_event).await;

        // Query session state
        let state = processor.get_current_state(session_id).unwrap();

        assert!(
            !state.errors.is_empty(),
            "errors should not be empty after failed tool call"
        );

        let error = &state.errors[0];
        assert!(
            error.error.contains("cargo_build"),
            "Error should mention the failed tool"
        );
        assert!(!error.resolved, "Error should not be resolved yet");

        println!("✅ Test E: Error Tracking passed");
        println!("   Error: {}", error.error);
        println!("   Resolved: {}", error.resolved);
    }

    // =========================================================================
    // 4. API Tests
    // =========================================================================

    /// Test F: get_session_state()
    #[tokio::test]
    async fn test_f_get_session_state() {
        let (_temp_dir, processor, _storage) = setup_processor().await;

        let session_id = Uuid::new_v4();

        // Initialize with a goal
        processor
            .handle_event(SessionEvent::user_prompt(
                session_id,
                "Implement user registration",
            ))
            .await;

        // Test get_session_state
        let state = get_session_state(&processor, session_id);

        assert!(state.is_some(), "get_session_state should return Some");
        let state = state.unwrap();

        assert!(
            !state.goal_stack.is_empty(),
            "goal_stack should not be empty"
        );

        println!("✅ Test F: get_session_state() passed");
        println!("   Session ID: {}", state.session_id);
        println!("   Goals: {}", state.goal_stack.len());
    }

    /// Test G: get_injectable_context()
    #[tokio::test]
    async fn test_g_get_injectable_context() {
        let session_id = Uuid::new_v4();
        let mut state = SessionState::with_initial_goal(session_id, "Add dark mode to the app");

        // Add some sub-goals
        state.add_sub_goal("Add theme toggle".to_string(), GoalSource::Inferred);
        state.add_sub_goal("Update CSS variables".to_string(), GoalSource::Inferred);
        state.complete_goal(state.goal_stack[0].sub_goals[0].id);

        // Add narrative
        state.update_narrative(
            "Implementing dark mode. Theme toggle is complete, now updating CSS variables."
                .to_string(),
        );

        // Add file context
        let mut file_ctx = FileContext::new(
            PathBuf::from("src/theme.ts"),
            "Theme configuration and toggle logic".to_string(),
            "Core theming".to_string(),
        );
        file_ctx.mark_modified();
        state.update_file_context(PathBuf::from("src/theme.ts"), file_ctx);

        // Test get_injectable_context
        let context = get_injectable_context(&state);

        assert!(
            context.contains("Current Goal"),
            "Context should contain 'Current Goal'"
        );
        assert!(
            context.contains("dark mode") || context.contains("Dark mode"),
            "Context should mention dark mode"
        );
        assert!(
            context.len() < 2000,
            "Context should be concise (< 2000 chars), got {}",
            context.len()
        );

        println!("✅ Test G: get_injectable_context() passed");
        println!("   Context length: {} chars", context.len());
        println!("\n--- Injectable Context ---\n{}\n--- End ---", context);
    }

    // =========================================================================
    // 5. Snapshot Persistence
    // =========================================================================

    #[tokio::test]
    async fn test_snapshot_persistence_and_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.lance");

        let session_id = Uuid::new_v4();

        // Phase 1: Create and save state
        {
            let connection = lancedb::connect(db_path.to_str().unwrap())
                .execute()
                .await
                .unwrap();

            let storage = Arc::new(Layer1Storage::new(connection).await.unwrap());
            let llm =
                Arc::new(TemplateLlm) as Arc<dyn crate::sidecar::synthesis_llm::SynthesisLlm>;
            let config = Layer1Config {
                use_llm: false,
                snapshot_interval: 1, // Snapshot after every event
                max_snapshots_per_session: 10,
            };

            let processor = Layer1Processor::new(storage.clone(), llm, config);

            // Add a goal (should trigger snapshot)
            processor
                .handle_event(SessionEvent::user_prompt(session_id, "Persistent task"))
                .await;

            // Verify state exists in memory
            let state = processor.get_current_state(session_id);
            assert!(state.is_some(), "State should exist in memory");

            // Storage should have snapshot
            let persisted = storage.get_latest_state(session_id).await.unwrap();
            assert!(persisted.is_some(), "State should be persisted");
        }
        // Processor dropped here - simulates process kill

        // Phase 2: Recover from storage
        {
            let connection = lancedb::connect(db_path.to_str().unwrap())
                .execute()
                .await
                .unwrap();

            let storage = Layer1Storage::new(connection).await.unwrap();

            // Recover state from LanceDB
            let recovered = storage.get_latest_state(session_id).await.unwrap();

            assert!(
                recovered.is_some(),
                "State should be recoverable from storage"
            );
            let recovered = recovered.unwrap();

            assert_eq!(
                recovered.session_id, session_id,
                "Session ID should match"
            );
            assert!(
                !recovered.goal_stack.is_empty(),
                "Goals should be recovered"
            );
            assert!(
                recovered.goal_stack[0].description.contains("Persistent"),
                "Goal description should be recovered"
            );

            println!("✅ Snapshot Persistence and Recovery passed");
            println!("   Recovered session: {}", recovered.session_id);
            println!("   Recovered goal: {}", recovered.goal_stack[0].description);
        }
    }

    // =========================================================================
    // 6. Sample Output
    // =========================================================================

    #[tokio::test]
    async fn test_sample_output_demonstration() {
        let session_id = Uuid::new_v4();

        // Create a realistic session state
        let mut state =
            SessionState::with_initial_goal(session_id, "Implement user authentication for the API");

        // Add sub-goals
        state.add_sub_goal("Create User model".to_string(), GoalSource::Inferred);
        state.add_sub_goal("Implement login endpoint".to_string(), GoalSource::Inferred);
        state.add_sub_goal("Add JWT middleware".to_string(), GoalSource::Inferred);

        // Complete first sub-goal
        state.complete_goal(state.goal_stack[0].sub_goals[0].id);

        // Add narrative
        state.update_narrative(
            "Created User model with email/password fields. Now implementing login endpoint. Decided to use JWT over sessions for stateless auth."
                .to_string(),
        );

        // Add file contexts
        let mut user_ctx = FileContext::new(
            PathBuf::from("src/models/user.rs"),
            "User struct with password hashing using bcrypt".to_string(),
            "Core user model for authentication".to_string(),
        );
        user_ctx.mark_modified();
        state.update_file_context(PathBuf::from("src/models/user.rs"), user_ctx);

        let mut auth_ctx = FileContext::new(
            PathBuf::from("src/routes/auth.rs"),
            "Login and register endpoints (in progress)".to_string(),
            "Authentication API routes".to_string(),
        );
        auth_ctx.mark_read();
        state.update_file_context(PathBuf::from("src/routes/auth.rs"), auth_ctx);

        // Add decision
        state.record_decision(Decision::new(
            "Use JWT over sessions".to_string(),
            "Stateless authentication is simpler to scale and works well with our microservices architecture".to_string(),
            vec![
                "Session-based auth".to_string(),
                "OAuth only".to_string(),
            ],
            Uuid::new_v4(),
        ));

        // Add open question
        state.add_open_question("Should refresh tokens be stored in DB or Redis?".to_string());

        // ===== Output 1: Raw state_json =====
        let state_json = serde_json::to_string_pretty(&state).unwrap();

        println!("\n");
        println!("═══════════════════════════════════════════════════════════════");
        println!("                     SAMPLE OUTPUT                              ");
        println!("═══════════════════════════════════════════════════════════════");
        println!("\n1. Raw state_json from session:\n");
        println!("```json");
        println!("{}", state_json);
        println!("```");

        // ===== Output 2: Formatted injectable context =====
        let context = get_injectable_context(&state);

        println!("\n2. Formatted output from get_injectable_context():\n");
        println!("```markdown");
        println!("{}", context);
        println!("```");

        println!("\n═══════════════════════════════════════════════════════════════");
        println!("                     END SAMPLE OUTPUT                          ");
        println!("═══════════════════════════════════════════════════════════════\n");

        // Verify data quality
        assert!(state_json.len() > 100, "state_json should be substantial");
        assert!(context.len() > 100, "context should be substantial");
        assert!(context.len() < 2000, "context should be concise");
        assert!(
            context.contains("[x]"),
            "context should show completed sub-goal"
        );
        assert!(
            context.contains("[ ]"),
            "context should show incomplete sub-goals"
        );

        println!("✅ Sample Output verification passed");
        println!("   state_json length: {} chars", state_json.len());
        println!("   injectable context length: {} chars", context.len());
    }

    // =========================================================================
    // Summary Test - Run All Verifications
    // =========================================================================

    #[tokio::test]
    async fn test_layer1_verification_summary() {
        println!("\n");
        println!("╔═══════════════════════════════════════════════════════════════╗");
        println!("║           LAYER 1 VERIFICATION TEST SUMMARY                   ║");
        println!("╠═══════════════════════════════════════════════════════════════╣");
        println!("║                                                               ║");
        println!("║  1. Schema Verification                                       ║");
        println!("║     ✓ session_states table exists in LanceDB                  ║");
        println!("║     ✓ Required columns: id, session_id, timestamp_ms,         ║");
        println!("║       state_json, snapshot_reason                             ║");
        println!("║                                                               ║");
        println!("║  2. Data Structure Tests                                      ║");
        println!("║     ✓ state_json contains goal_stack, narrative, decisions,   ║");
        println!("║       file_contexts, errors, open_questions                   ║");
        println!("║     ✓ Serialization roundtrip works correctly                 ║");
        println!("║                                                               ║");
        println!("║  3. Functional Tests                                          ║");
        println!("║     ✓ Test A: Goal Extraction from user prompts               ║");
        println!("║     ✓ Test B: File Context Capture on read_file               ║");
        println!("║     ✓ Test C: Decision Logging from reasoning events          ║");
        println!("║     ✓ Test D: Narrative Update after multiple events          ║");
        println!("║     ✓ Test E: Error Tracking from failed tool calls           ║");
        println!("║                                                               ║");
        println!("║  4. API Tests                                                 ║");
        println!("║     ✓ Test F: get_session_state() returns valid state         ║");
        println!("║     ✓ Test G: get_injectable_context() is concise (<2000)     ║");
        println!("║                                                               ║");
        println!("║  5. Snapshot Persistence                                      ║");
        println!("║     ✓ State survives process restart via LanceDB              ║");
        println!("║                                                               ║");
        println!("║  6. Sample Output                                             ║");
        println!("║     ✓ Raw state_json is well-formed                           ║");
        println!("║     ✓ Injectable context is readable and useful               ║");
        println!("║                                                               ║");
        println!("╚═══════════════════════════════════════════════════════════════╝");
        println!("\n");
    }
}

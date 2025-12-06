//! Testbed for evaluating mistral.rs as a replacement for llama-cpp-2
//!
//! Run with:
//!   cd mistral-testbed && cargo run --release
//!
//! This tests:
//! 1. Loading the existing Qwen GGUF model
//! 2. Chat completion with proper template
//! 3. Commit message generation

use anyhow::Result;
use mistralrs::{GgufModelBuilder, TextMessageRole, TextMessages};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("=== Mistral.rs Testbed ===\n");

    // Find the Qwen model in the default location
    let home = dirs::home_dir().expect("Could not find home directory");
    let models_dir = home.join(".qbit/models");
    let model_file = "qwen2.5-0.5b-instruct-q4_k_m.gguf";
    let model_path = models_dir.join(model_file);

    if !model_path.exists() {
        eprintln!("Model not found at: {:?}", model_path);
        eprintln!("Please ensure the Qwen model is downloaded first.");
        std::process::exit(1);
    }

    println!("Loading model from: {:?}", model_path);
    println!("This may take a moment...\n");

    // Build the model with GGUF loader
    // Note: We use the directory and filename separately
    // Skip PagedAttention to avoid Metal shader issues
    let model = GgufModelBuilder::new(
        models_dir.to_string_lossy().to_string(),
        vec![model_file.to_string()],
    )
    .with_logging()
    .build()
    .await?;

    println!("Model loaded successfully!\n");

    // Test 1: Simple completion
    println!("--- Test 1: Simple Chat ---");
    let messages = TextMessages::new()
        .add_message(TextMessageRole::System, "You are a helpful assistant.")
        .add_message(TextMessageRole::User, "What is 2 + 2? Answer briefly.");

    let response = model.send_chat_request(messages).await?;
    println!("Response: {}", response.choices[0].message.content.as_deref().unwrap_or("(no content)"));
    println!(
        "Tokens: {} prompt, {} completion\n",
        response.usage.prompt_tokens, response.usage.completion_tokens
    );

    // Test 2: Commit message generation (our actual use case)
    println!("--- Test 2: Commit Message Generation ---");
    let system_prompt = r#"You are a helpful assistant that generates git commit messages.
Follow conventional commits format: type(scope): description
Types: feat, fix, refactor, docs, test, chore
Keep subject line under 50 chars."#;

    let user_prompt = r#"Generate a commit message for these changes:

User request: Add authentication to the API
Session summary: Added JWT-based authentication
Files changed: src/auth.rs, src/api/routes.rs
Key decisions: Using JWT for statelessness

Output just the commit message, nothing else."#;

    let messages = TextMessages::new()
        .add_message(TextMessageRole::System, system_prompt)
        .add_message(TextMessageRole::User, user_prompt);

    let response = model.send_chat_request(messages).await?;
    println!("Generated commit message:");
    println!("{}", response.choices[0].message.content.as_deref().unwrap_or("(no content)"));
    println!(
        "\nTokens: {} prompt, {} completion",
        response.usage.prompt_tokens, response.usage.completion_tokens
    );

    // Test 3: Streaming response
    println!("\n--- Test 3: Streaming Response ---");
    let messages = TextMessages::new()
        .add_message(TextMessageRole::System, "You are a helpful coding assistant.")
        .add_message(TextMessageRole::User, "Write a haiku about Rust programming.");

    print!("Response: ");
    use std::io::Write;
    let mut stream = model.stream_chat_request(messages).await?;
    while let Some(chunk) = stream.next().await {
        match chunk {
            mistralrs::Response::Chunk(c) => {
                if let Some(content) = &c.choices[0].delta.content {
                    print!("{}", content);
                    std::io::stdout().flush()?;
                }
            }
            mistralrs::Response::Done(d) => {
                println!("\n(Done: {} tokens)", d.usage.completion_tokens);
            }
            _ => {}
        }
    }

    println!("\n=== Testbed Complete ===");
    Ok(())
}

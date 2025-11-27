//! Test Vertex AI Anthropic integration
//!
//! Run with: cargo run --example test_vertex

use rig::completion::CompletionModel;
use rig_anthropic_vertex::{models, Client};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("Creating Vertex AI client...");

    // Use your service account credentials
    let client = Client::from_service_account(
        "/Users/xlyk/.keys/vertex-ai.json",
        "futurhealth",
        "us-east5",
    )
    .await?;

    println!("Client created successfully!");
    println!("Project: {}", client.project_id());
    println!("Location: {}", client.location());

    // Get completion model
    let model = client.completion_model(models::CLAUDE_OPUS_4_5);
    println!("Using model: {}", models::CLAUDE_OPUS_4_5);

    // Build a simple request
    println!("\nSending test prompt...");

    let request = rig::completion::CompletionRequest {
        preamble: Some("You are a helpful coding assistant.".to_string()),
        chat_history: rig::one_or_many::OneOrMany::one(rig::completion::Message::User {
            content: rig::one_or_many::OneOrMany::one(rig::message::UserContent::Text(
                rig::message::Text {
                    text: "Write a Rust function that calculates the fibonacci sequence using memoization. Keep it brief."
                        .to_string(),
                },
            )),
        }),
        documents: vec![],
        tools: vec![],
        temperature: Some(0.5),
        max_tokens: Some(500),
        tool_choice: None,
        additional_params: None,
    };

    match model.completion(request).await {
        Ok(response) => {
            println!("\n=== Response ===");
            for content in response.choice.iter() {
                if let rig::completion::AssistantContent::Text(t) = content {
                    println!("{}", t.text);
                }
            }
            println!("\n=== Usage ===");
            println!("Input tokens: {}", response.usage.input_tokens);
            println!("Output tokens: {}", response.usage.output_tokens);
            println!("Total tokens: {}", response.usage.total_tokens);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            return Err(e.into());
        }
    }

    println!("\nTest completed successfully!");
    Ok(())
}

mod config;
mod executor;
mod llm_client;
mod llm_client_trait;

use anyhow::Result;
use clap::Parser;
use llm_client_trait::{LlmClient, SessionConfig, ToolResult};
use std::io::{self, Read};
use std::path::PathBuf;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

use crate::config::ToolDefinition;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// Path to the configuration file
  #[arg(short, long)]
  config: PathBuf,

  /// Model to use (defaults to environment variable LLM_CLI_MODEL or "gpt-4o")
  #[arg(short, long)]
  model: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
  // Initialize tracing
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .init();

  let args = Args::parse();
  let config = config::Config::from_file(&args.config)?;

  // Get API configuration from environment
  let base_url =
    std::env::var("LLM_CLI_ENDPOINT").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
  let api_key = std::env::var("LLM_CLI_TOKEN")?;

  // Create LLM client
  let client = llm_client::OpenAIClient::new(base_url, api_key);

  let model = args
    .model
    .clone()
    .or_else(|| std::env::var("LLM_CLI_MODEL").ok())
    .unwrap_or_else(|| "gpt-4o".to_string());

  let executor = executor::Executor::new();

  info!("Loaded {} tools from config", config.tools.len());

  // Read prompt from stdin
  let mut prompt = String::new();
  io::stdin().read_to_string(&mut prompt)?;

  debug!("Prompt: {}", prompt.trim());

  let tool_definitions: Vec<_> = config.tools.iter().map(|t| t.definition.clone()).collect();

  // Create session
  let session_config = SessionConfig {
    model,
    system_prompt: None,
    tools: tool_definitions,
  };

  let mut session = client.create_session(session_config).await?;

  // Main conversation loop
  let mut result = session.send_message(prompt.clone()).await?;
  loop {
    if let Some(content) = &result.content {
      println!("{}", content);
    }

    let tool_calls = result.tool_calls;

    // If no tool calls, we're done
    if tool_calls.is_empty() {
      debug!("No tool calls made, ending conversation");
      break;
    }

    // Execute tool calls
    println!("\n--- Executing tools ---");
    let mut tool_results = Vec::new();

    for tool_call in &tool_calls {
      println!("Tool: {} ({})", tool_call.name, tool_call.id);
      println!("Arguments: {}", tool_call.arguments);

      // Find the tool in config
      let tool = config
        .tools
        .iter()
        .find(|t| t.definition.name == tool_call.name)
        .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", tool_call.name))?;

      // Execute the tool
      match executor
        .execute_tool(tool, &tool_call.arguments, &config.shell)
        .await
      {
        Ok(output) => {
          println!("Output:\n{}", output);
          tool_results.push(ToolResult {
            tool_call_id: tool_call.id.clone(),
            content: output,
          });
        }
        Err(e) => {
          error!("Tool execution failed: {}", e);
          let error_msg = format!("Error: {}", e);
          tool_results.push(ToolResult {
            tool_call_id: tool_call.id.clone(),
            content: error_msg,
          });
        }
      }
    }

    println!("--- End tool execution ---\n");

    // Send tool results back to the model
    result = session.send_tool_results(tool_results).await?;

    if let Some(content) = &result.content {
      println!("{}", content);
    }

    // Continue looping if there are more tool calls
    if result.tool_calls.is_empty() {
      break;
    }
  }

  Ok(())
}

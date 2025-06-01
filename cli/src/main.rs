mod config;
mod executor;
mod llm_client;

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use std::io::{self, Read};
use std::path::PathBuf;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

use crate::llm_client::{ToolCall, ToolCallFunction};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// Path to the configuration file
  #[arg(short, long)]
  config: PathBuf,

  /// Model to use (defaults to environment variable LLM_CLI_MODEL or "gpt-4")
  #[arg(short, long)]
  model: Option<String>,

  /// Log file path for conversation history
  #[arg(short, long, default_value = None)]
  log_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
  // Initialize tracing
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .init();

  let args = Args::parse();

  // Load configuration
  let config = config::Config::from_file(&args.config)?;
  info!("Loaded {} tools from config", config.tools.len());

  // Initialize LLM client
  let llm_client = llm_client::LlmClient::from_env()?;

  // Initialize executor
  let executor = executor::Executor::new();

  // Read prompt from stdin
  let mut prompt = String::new();
  io::stdin().read_to_string(&mut prompt)?;

  if prompt.trim().is_empty() {
    error!("No prompt provided");
    return Ok(());
  }

  // Initialize conversation log
  let mut conversation_log = ConversationLog::new(&args.log_file);

  // Create initial message
  let mut messages = vec![llm_client::Message::User {
    content: prompt.trim().to_string(),
  }];

  conversation_log.add_message(&messages[0]).await?;

  // Convert tools to LLM format
  let tool_definitions: Vec<_> = config
    .tools
    .iter()
    .map(|tool| tool.to_llm_definition())
    .collect();

  // Main conversation loop
  loop {
    // Create request
    let request = llm_client::LlmRequest {
      messages: messages.clone(),
      stream: true,
      tools: tool_definitions.clone(),
      model: args
        .model
        .clone()
        .or_else(|| std::env::var("LLM_CLI_MODEL").ok())
        .unwrap_or_else(|| "gpt-4".to_string()),
    };

    // Stream response
    let mut stream = llm_client.stream_completion(request).await?;
    let mut accumulated_text = Some(String::new());
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    let mut tool_arguments_jsons: Vec<String> = Vec::new();

    while let Some(event) = stream.next().await {
      match event? {
        llm_client::StreamEvent::Chunk(chunk) => {
          debug!("Received chunk: {:?}", &chunk);

          for choice in chunk.choices {
            if let Some(delta) = choice.delta {
              if let Some(content) = delta.content {
                print!("{}", &content);
                // Initialize accumulated_text if it's None
                if accumulated_text.is_none() {
                  accumulated_text = Some(String::new());
                }
                // Append content to accumulated_text
                if let Some(ref mut text) = accumulated_text {
                  text.push_str(&content);
                }
              }
              // Handle tool calls
              if let Some(calls) = delta.tool_calls {
                calls.into_iter().for_each(|call| {
                  debug!("Received tool call: {:?}", &call);
                  if let Some(id) = call.id {
                    debug!("Tool call function: {:?}", call.function.name);
                    tool_calls.push(ToolCall {
                      id: id,
                      tool_type: "function".to_string(),
                      function: ToolCallFunction {
                        name: call
                          .function
                          .name
                          .expect("Tool call with id must have a name"),
                        arguments: serde_json::Value::Null,
                      },
                    });
                    tool_arguments_jsons.push(call.function.arguments);
                  } else {
                    tool_arguments_jsons[call.index].push_str(&call.function.arguments);
                  }
                });
              }
            }
          }
        }
        llm_client::StreamEvent::Done => {
          debug!("Stream completed");
          break;
        }
      }
    }

    // Deserialize accumulated arguments and update tool calls
    for (i, args_string) in tool_arguments_jsons.into_iter().enumerate() {
      if let Ok(args) = serde_json::from_str::<serde_json::Value>(&args_string) {
        if let Some(tool_call) = tool_calls.get_mut(i) {
          tool_call.function.arguments = args;
        }
      } else {
        error!("Failed to parse tool arguments: {}", args_string);
      }
    }

    // If we got text, add it as assistant message
    if !accumulated_text.is_none() {
      println!(); // New line after streaming
    } else {
      tracing::debug!("No text response received.");
    }

    let assistant_msg = llm_client::Message::Assistant {
      content: accumulated_text,
      tool_calls: if tool_calls.len() > 0 {
        Some(tool_calls.clone())
      } else {
        None
      },
    };
    conversation_log.add_message(&assistant_msg).await?;
    messages.push(assistant_msg);

    // Execute tool calls
    println!("\n--- Executing tools ---");
    for tool_call in &tool_calls {
      println!("Tool: {} ({})", tool_call.function.name, tool_call.id);
      println!("Arguments: {:?}", tool_call.function.arguments);

      // Find the tool in config
      let tool = config
        .tools
        .iter()
        .find(|t| t.name == tool_call.function.name)
        .ok_or_else(|| anyhow::anyhow!("Tool not found: {}", tool_call.function.name))?;

      // Execute the tool
      match executor
        .execute_tool(tool, &tool_call.function.arguments, &config.shell)
        .await
      {
        Ok(output) => {
          println!("Output:\n{}", output);

          // Log first while we still own output
          conversation_log
            .add_tool_result(&tool_call, &output)
            .await?;

          // Convert to message format (simplified for MVP)
          let tool_msg = llm_client::Message::Tool {
            tool_call_id: tool_call.id.clone(),
            content: output,
          };

          messages.push(tool_msg);
        }
        Err(e) => {
          error!("Tool execution failed: {}", e);
          let error_msg = format!("Error: {}", e);

          conversation_log
            .add_tool_result(&tool_call, &error_msg)
            .await?;

          let tool_msg = llm_client::Message::Tool {
            tool_call_id: tool_call.id.clone(),
            content: error_msg.clone(),
          };

          messages.push(tool_msg);
        }
      }
    }

    println!("--- End tool execution ---\n");

    // Log tool calls
    for tool_call in &tool_calls {
      conversation_log.add_tool_call(tool_call).await?;
    }

    if tool_calls.is_empty() {
      println!("--- No tool calls made ---");
      // If no tool calls were made, we can exit the loop
      break;
    }
  }

  Ok(())
}

// Simple conversation logger
struct ConversationLog {
  file_path: Option<PathBuf>,
  entries: Vec<serde_json::Value>,
}

impl ConversationLog {
  fn new(file_path: &Option<PathBuf>) -> Self {
    Self {
      file_path: file_path.clone(),
      entries: Vec::new(),
    }
  }

  async fn add_message(&mut self, message: &llm_client::Message) -> Result<()> {
    self.entries.push(serde_json::json!({
        "type": "message",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "message": message,
    }));
    // TODO: this is slow - appending must be faster. we can use newline delimited JSON or similar
    self.save().await
  }

  async fn add_tool_call(&mut self, tool_call: &llm_client::ToolCall) -> Result<()> {
    self.entries.push(serde_json::json!({
        "type": "tool_call",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "tool_call": tool_call,
    }));
    self.save().await
  }

  async fn add_tool_result(
    &mut self,
    tool_call: &llm_client::ToolCall,
    output: &str,
  ) -> Result<()> {
    self.entries.push(serde_json::json!({
        "type": "tool_result",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "tool_call_id": tool_call.id,
        "tool_name": tool_call.function.name,
        "output": output,
    }));
    self.save().await
  }

  async fn save(&self) -> Result<()> {
    match &self.file_path {
      Some(path) => {
        if let Some(_parent) = path.parent() {
          let json = serde_json::to_string_pretty(&self.entries)?;
          tokio::fs::write(&path, json).await?;
          Ok(())
        } else {
          Err(anyhow::anyhow!("Failed to get parent directory"))
        }
      }
      None => return Ok(()), // No logging file specified
    }
  }
}

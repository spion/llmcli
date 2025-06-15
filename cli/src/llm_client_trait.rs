use anyhow::Result;
use futures::Stream;
use serde::{Deserialize, Serialize};

// Session configuration
#[derive(Debug, Clone)]
pub struct SessionConfig {
  pub model: String,
  pub system_prompt: Option<String>,
  pub tools: Vec<ToolDefinition>,
}

#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
  /// Create a new session with initial configuration
  async fn create_session(&self, config: SessionConfig) -> Result<Box<dyn LlmSession>>;
}

pub type EventStream = dyn Stream<Item = Result<LlmEvent>> + Send + Unpin;

// Core session trait with both streaming and non-streaming methods

#[async_trait::async_trait]
pub trait LlmSession: Send + Sync {
  /// Send a user message and get complete response (non-streaming)
  async fn send_message(&mut self, content: String) -> Result<CompletionResult>;

  /// Send a user message and get response events (streaming)
  async fn send_message_stream(&mut self, content: String) -> Result<Box<EventStream>>;

  /// Send tool call results and get complete response (non-streaming)
  async fn send_tool_results(&mut self, results: Vec<ToolResult>) -> Result<CompletionResult>;

  /// Send tool call results and get response events (streaming)
  async fn send_tool_results_stream(
    &mut self,
    results: Vec<ToolResult>,
  ) -> Result<Box<EventStream>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
  // pub index: usize,
  pub id: String,
  pub name: String,
  pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
  pub name: String,
  pub description: String,
  pub parameters: serde_json::Value,
}

// Events that can be emitted
#[derive(Debug, Clone)]
pub enum LlmEvent {
  /// Text token(s) - for real-time display
  TextDelta(String),

  /// Complete response with text and/or tool calls
  Completion(CompletionResult),
}

#[derive(Debug, Clone)]
pub struct CompletionResult {
  pub content: Option<String>,
  pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
  pub tool_call_id: String,
  pub content: String,
}

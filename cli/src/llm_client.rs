use crate::{
  config::ToolDefinition,
  llm_client_trait::{
    CompletionResult, EventStream, LlmClient, LlmEvent, LlmSession, SessionConfig, ToolCall,
    ToolResult,
  },
};
use anyhow::{Result, anyhow};
use async_openai_types::types::{self as oai, ChatCompletionRequestAssistantMessageContent};
use async_trait::async_trait;
use futures::stream;
use reqwest::Client;

pub struct OpenAIClient {
  base_url: String,
  api_key: String,
  client: Client,
}

impl OpenAIClient {
  pub fn new(base_url: String, api_key: String) -> Self {
    Self {
      base_url,
      api_key,
      client: Client::new(),
    }
  }
}

#[async_trait]
impl LlmClient for OpenAIClient {
  async fn create_session(&self, config: SessionConfig) -> Result<Box<dyn LlmSession>> {
    Ok(Box::new(OpenAISession {
      client: self.client.clone(),
      base_url: self.base_url.clone(),
      api_key: self.api_key.clone(),
      config,
      messages: Vec::new(),
    }))
  }
}

struct OpenAISession {
  client: Client,
  base_url: String,
  api_key: String,
  config: SessionConfig,
  messages: Vec<oai::ChatCompletionRequestMessage>,
}

fn tool_definition_to_params_json(tool: &ToolDefinition) -> serde_json::Value {
  serde_json::json!({
    "type": "object",
    "properties": tool.params.iter().map(|(name, prop)| {
      (
        name.clone(),
        serde_json::json!({
          "type": prop.prop_type,
          "description": prop.description,
          "required": prop.required.unwrap_or(false),
        }),
      )
    }).collect::<serde_json::Map<String, serde_json::Value>>(),
    "required": tool.params.iter().filter_map(|(name, prop)| {
      if prop.required.unwrap_or(false) {
        Some(name.clone())
      } else {
        None
      }
    }).collect::<Vec<String>>(),
  })
}

impl OpenAISession {
  fn add_user_message(&mut self, content: String) {
    // Add system prompt if this is the first message
    if self.messages.is_empty() && self.config.system_prompt.is_some() {
      self
        .messages
        .push(oai::ChatCompletionRequestMessage::System(
          oai::ChatCompletionRequestSystemMessage {
            content: oai::ChatCompletionRequestSystemMessageContent::Text(
              self.config.system_prompt.clone().unwrap(),
            ),
            ..Default::default()
          },
        ));
    }

    self.messages.push(oai::ChatCompletionRequestMessage::User(
      oai::ChatCompletionRequestUserMessage {
        content: oai::ChatCompletionRequestUserMessageContent::Text(content),
        ..Default::default()
      },
    ));
  }

  fn add_tool_results(&mut self, results: Vec<ToolResult>) {
    for result in results {
      self.messages.push(oai::ChatCompletionRequestMessage::Tool(
        oai::ChatCompletionRequestToolMessage {
          content: oai::ChatCompletionRequestToolMessageContent::Text(result.content),
          tool_call_id: result.tool_call_id,
        },
      ));
    }
  }

  async fn complete(&mut self) -> Result<CompletionResult> {
    // Build request

    let tools = self
      .config
      .tools
      .iter()
      .map(|t| oai::ChatCompletionTool {
        r#type: oai::ChatCompletionToolType::Function,
        function: oai::FunctionObject {
          name: t.name.clone(),
          description: Some(t.description.clone()),
          parameters: Some(tool_definition_to_params_json(&t)),
          strict: None,
        },
      })
      .collect();

    let request = oai::CreateChatCompletionRequest {
      model: self.config.model.clone(),
      messages: self.messages.clone(),
      tools: Some(tools),
      parallel_tool_calls: Some(true),
      ..Default::default()
    };

    // Make request
    let response = self
      .client
      .post(format!("{}/chat/completions", self.base_url))
      .header("Authorization", format!("Bearer {}", self.api_key))
      .header("Content-Type", "application/json")
      .json(&request)
      .send()
      .await?;

    if !response.status().is_success() {
      let status = response.status();
      let text = response.text().await?;
      return Err(anyhow!(
        "API request failed with status {}: {}",
        status,
        text
      ));
    }

    // Parse response
    let completion: oai::CreateChatCompletionResponse = response.json().await?;

    let choice = completion
      .choices
      .into_iter()
      .next()
      .ok_or_else(|| anyhow!("No choices in response"))?;

    let message = choice.message;

    let result = CompletionResult {
      content: message.content.clone(),
      tool_calls: message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|tc| ToolCall {
          id: tc.id,
          name: tc.function.name,
          arguments: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
        })
        .collect(),
    };

    // Add assistant message to history
    self
      .messages
      .push(oai::ChatCompletionRequestMessage::Assistant(
        oai::ChatCompletionRequestAssistantMessage {
          content: result
            .content
            .clone()
            .map(ChatCompletionRequestAssistantMessageContent::Text),
          tool_calls: if result.tool_calls.is_empty() {
            None
          } else {
            Some(
              result
                .tool_calls
                .iter()
                .map(|tc| oai::ChatCompletionMessageToolCall {
                  id: tc.id.clone(),
                  r#type: oai::ChatCompletionToolType::Function,
                  function: oai::FunctionCall {
                    name: tc.name.clone(),
                    arguments: tc.arguments.to_string(),
                  },
                })
                .collect(),
            )
          },
          ..Default::default()
        },
      ));

    Ok(result)
  }
}

#[async_trait]
impl LlmSession for OpenAISession {
  async fn send_message(&mut self, content: String) -> Result<CompletionResult> {
    self.add_user_message(content);
    self.complete().await
  }

  async fn send_message_stream(&mut self, content: String) -> Result<Box<EventStream>> {
    self.add_user_message(content);
    let result = self.complete().await?;

    // Just emit the final completion event
    Ok(Box::new(Box::pin(stream::once(async move {
      Ok(LlmEvent::Completion(result))
    }))))
  }

  async fn send_tool_results(&mut self, results: Vec<ToolResult>) -> Result<CompletionResult> {
    self.add_tool_results(results);
    self.complete().await
  }

  async fn send_tool_results_stream(
    &mut self,
    results: Vec<ToolResult>,
  ) -> Result<Box<EventStream>> {
    self.add_tool_results(results);
    let result = self.complete().await?;

    // Just emit the final completion event
    Ok(Box::new(Box::pin(stream::once(async move {
      Ok(LlmEvent::Completion(result))
    }))))
  }
}

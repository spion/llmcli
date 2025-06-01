use anyhow::Result;
use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::{Deserialize, Serialize};
use std::env;
use std::pin::Pin;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
  User {
    content: String,
  },
  Assistant {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
  },
  Tool {
    tool_call_id: String,
    content: String,
  },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
  // pub index: usize,
  pub id: String,
  #[serde(rename = "type", default = "generate_function")]
  pub tool_type: String,
  pub function: ToolCallFunction,
}

fn generate_function() -> String {
  "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
  #[serde(default = "default_call_name")]
  pub name: String,
  #[serde(serialize_with = "serialize_arguments_as_string")]
  pub arguments: serde_json::Value,
}

fn serialize_arguments_as_string<S>(
  value: &serde_json::Value,
  serializer: S,
) -> Result<S::Ok, S::Error>
where
  S: serde::Serializer,
{
  let json_string = serde_json::to_string(value).map_err(serde::ser::Error::custom)?;
  serializer.serialize_str(&json_string)
}

fn default_call_name() -> String {
  "".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
  pub tool_call_id: String,
  pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LlmResponse {
  Text { content: String },
  ToolCall { tool_calls: Vec<ToolCall> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
  pub messages: Vec<Message>,
  pub tools: Vec<ToolDefinition>,
  pub model: String,
  pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
  pub function: FunctionDefinition,
  #[serde(rename = "type")]
  pub tool_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
  pub name: String,
  pub description: String,
  pub parameters: serde_json::Value, // JSON schema for parameters
}

pub struct LlmClient {
  client: reqwest_middleware::ClientWithMiddleware,
  endpoint: String,
  headers: HeaderMap,
}

impl LlmClient {
  pub fn from_env() -> Result<Self> {
    let endpoint =
      env::var("LLM_CLI_ENDPOINT").map_err(|_| anyhow::anyhow!("LLM_CLI_ENDPOINT not set"))?;

    let mut headers = HeaderMap::new();

    // Add custom headers from environment
    for (key, value) in env::vars() {
      if key.starts_with("LLM_CLI_HEADER_") {
        let header_name = key.strip_prefix("LLM_CLI_HEADER_").unwrap();
        let header_name = HeaderName::from_bytes(header_name.as_bytes())?;
        let header_value = HeaderValue::from_str(&value)?;
        headers.insert(header_name, header_value);
      }
    }

    // Add token if provided
    if let Ok(token) = env::var("LLM_CLI_TOKEN") {
      headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", token))?,
      );
    }

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client = ClientBuilder::new(reqwest::Client::new())
      .with(RetryTransientMiddleware::new_with_policy(retry_policy))
      .build();

    Ok(Self {
      client,
      endpoint,
      headers,
    })
  }

  pub async fn stream_completion(
    &self,
    request: LlmRequest,
  ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
    let body = serde_json::to_string(&request)?;
    let response = self
      .client
      .post(&self.endpoint)
      .headers(self.headers.clone())
      .header("Content-Type", "application/json")
      .body(body)
      .send()
      .await?;

    if !response.status().is_success() {
      let status = response.status();
      let body = response.text().await?;
      anyhow::bail!("LLM API error: {} - {}", status, body);
    }

    let stream = response.bytes_stream().eventsource().map(|event| {
      match event {
        Ok(event) => {
          // Parse SSE event data
          let data = event.data;
          if data == "[DONE]" {
            tracing::debug!("Received done event");
            Ok(StreamEvent::Done)
          } else {
            tracing::debug!("Received chunk data: {}", &data);
            match serde_json::from_str::<StreamChunk>(&data) {
              Ok(chunk) => Ok(StreamEvent::Chunk(chunk)),
              Err(e) => Err(anyhow::anyhow!("Failed to parse chunk: {}", e)),
            }
          }
        }
        Err(e) => Err(anyhow::anyhow!("Stream error: {}", e)),
      }
    });

    Ok(Box::pin(stream))
  }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
  Chunk(StreamChunk),
  Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
  pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChoice {
  pub delta: Option<Delta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
  pub content: Option<String>,
  pub tool_calls: Option<Vec<ToolCallChunk>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallChunk {
  pub index: usize,
  pub id: Option<String>,
  pub function: ToolCallFunctionChunk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunctionChunk {
  pub name: Option<String>,
  pub arguments: String, // Partial JSON
}

impl crate::config::Tool {
  pub fn to_llm_definition(&self) -> ToolDefinition {
    let mut parameters = serde_json::json!({});

    for schema in &self.input_schema {
      match schema {
        crate::config::JsonSchema::Object {
          properties,
          required,
        } => {
          let mut schema_props = serde_json::Map::new();

          for (name, prop) in properties {
            let mut prop_def = serde_json::json!({
                "type": prop.prop_type,
                "description": prop.description,
            });

            if let Some(pattern) = &prop.pattern {
              prop_def["pattern"] = serde_json::json!(pattern);
            }

            schema_props.insert(name.clone(), prop_def);
          }

          parameters = serde_json::json!({
              "type": "object",
              "properties": schema_props,
              "required": required,
          });
        }
      }
    }

    ToolDefinition {
      function: FunctionDefinition {
        name: self.name.clone(),
        description: self.description.clone(),
        parameters,
      },
      tool_type: "function".to_string(),
    }
  }
}

#[cfg(test)]
mod tests {

  #[test]
  fn test_tool_to_llm_definition() {
    let tool = crate::config::Tool {
      name: "test".to_string(),
      description: "Test tool".to_string(),
      input_schema: vec![crate::config::JsonSchema::Object {
        properties: vec![(
          "message".to_string(),
          crate::config::Property {
            prop_type: "string".to_string(),
            description: "Test message".to_string(),
            pattern: None,
          },
        )]
        .into_iter()
        .collect(),
        required: vec!["message".to_string()],
      }],
      command: "echo $param_message".to_string(),
      shell: None,
    };

    let def = tool.to_llm_definition();
    assert_eq!(def.function.name, "test");
    assert_eq!(def.function.description, "Test tool");
    let params = def.function.parameters.as_object().unwrap();
    assert_eq!(params.get("type").unwrap(), "object");
    assert!(params.get("properties").is_some());
    assert!(params.get("required").is_some());
  }
}

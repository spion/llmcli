use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  #[serde(default = "default_shell")]
  pub shell: String,
  pub tools: Vec<ToolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
  #[serde(flatten)]
  pub definition: ToolDefinition,
  #[serde(flatten)]
  pub implementation: ToolImplementation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
  pub name: String,
  pub description: String,
  pub params: HashMap<String, Property>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolImplementation {
  pub command: String,
  pub shell: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
  #[serde(rename = "type")]
  pub prop_type: String,
  pub description: String,
  pub required: Option<bool>,
}

fn default_shell() -> String {
  "bash".to_string()
}

impl Config {
  pub fn from_file(path: &Path) -> Result<Self> {
    let contents = std::fs::read_to_string(path)?;
    let config: Config = serde_yaml::from_str(&contents)
      .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
    Ok(config)
  }
}

impl ToolConfig {
  pub fn get_shell(&self, default: &str) -> String {
    self
      .implementation
      .shell
      .clone()
      .unwrap_or_else(|| default.to_string())
  }

  pub fn build_command(&self) -> Result<String> {
    Ok(self.implementation.command.clone())
  }

  pub fn build_env_vars(&self, input: &serde_json::Value) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    if let Some(obj) = input.as_object() {
      for (key, value) in obj {
        let env_name = format!("param_{}", key);
        let value_str = match value {
          serde_json::Value::String(s) => s.clone(),
          _ => serde_json::to_string(value).unwrap_or_default(),
        };
        env_vars.push((env_name, value_str));
      }
    }

    env_vars
  }
}

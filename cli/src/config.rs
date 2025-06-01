use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_shell")]
    pub shell: String,
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Vec<JsonSchema>,
    pub command: String,
    pub shell: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JsonSchema {
    Object {
        properties: HashMap<String, Property>,
        #[serde(default)]
        required: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    #[serde(rename = "type")]
    pub prop_type: String,
    pub description: String,
    pub pattern: Option<String>,
}

fn default_shell() -> String {
    "bash".to_string()
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&contents)?;
        
        // Validate patterns are valid regex
        for tool in &config.tools {
            for schema in &tool.input_schema {
                match schema {
                    JsonSchema::Object { properties, .. } => {
                        for (_, prop) in properties {
                            if let Some(pattern) = &prop.pattern {
                                Regex::new(pattern)?;
                            }
                        }
                    }
                }
            }
        }
        
        Ok(config)
    }
}

impl Tool {
    pub fn get_shell(&self, default: &str) -> String {
        self.shell.clone().unwrap_or_else(|| default.to_string())
    }
    
    pub fn validate_input(&self, input: &serde_json::Value) -> Result<()> {
        for schema in &self.input_schema {
            match schema {
                JsonSchema::Object { properties, required } => {
                    let obj = input.as_object()
                        .ok_or_else(|| anyhow::anyhow!("Input must be an object"))?;
                    
                    // Check required fields
                    for req in required {
                        if !obj.contains_key(req) {
                            anyhow::bail!("Missing required field: {}", req);
                        }
                    }
                    
                    // Validate each property
                    for (name, value) in obj {
                        if let Some(prop) = properties.get(name) {
                            if let Some(pattern) = &prop.pattern {
                                let regex = Regex::new(pattern)?;
                                let str_value = value.as_str()
                                    .ok_or_else(|| anyhow::anyhow!("Property {} must be a string", name))?;
                                if !regex.is_match(str_value) {
                                    anyhow::bail!("Property {} doesn't match pattern {}", name, pattern);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    
    pub fn build_command(&self, input: &serde_json::Value) -> Result<String> {
        self.validate_input(input)?;
        Ok(self.command.clone())
    }
    
    pub fn build_env_vars(&self, input: &serde_json::Value) -> Vec<(String, String)> {
        let mut env_vars = Vec::new();
        
        if let Some(obj) = input.as_object() {
            for (key, value) in obj {
                let env_name = format!("param_{}", key);
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                env_vars.push((env_name, value_str));
            }
        }
        
        env_vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_config() {
        let yaml = r#"
shell: "bash"
tools:
  - name: "echo"
    description: "Echo a message"
    input_schema:
      - type: object
        properties:
          message:
            type: string
            description: "Message to echo"
    command: |
      echo "$param_message"
"#;
        
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.shell, "bash");
        assert_eq!(config.tools.len(), 1);
        assert_eq!(config.tools[0].name, "echo");
    }
    
    #[test]
    fn test_validate_input() {
        let tool = Tool {
            name: "test".to_string(),
            description: "Test tool".to_string(),
            input_schema: vec![JsonSchema::Object {
                properties: vec![
                    ("url".to_string(), Property {
                        prop_type: "string".to_string(),
                        description: "URL".to_string(),
                        pattern: Some(r"^https?://.*".to_string()),
                    })
                ].into_iter().collect(),
                required: vec!["url".to_string()],
            }],
            command: "test".to_string(),
            shell: None,
        };
        
        let valid_input = serde_json::json!({
            "url": "https://example.com"
        });
        assert!(tool.validate_input(&valid_input).is_ok());
        
        let invalid_input = serde_json::json!({
            "url": "not-a-url"
        });
        assert!(tool.validate_input(&invalid_input).is_err());
    }
}
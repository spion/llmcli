use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

pub struct Executor {
    working_dir: std::path::PathBuf,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        }
    }
    
    pub async fn execute_tool(
        &self,
        tool: &crate::config::Tool,
        input: &serde_json::Value,
        default_shell: &str,
    ) -> Result<String> {
        let command = tool.build_command(input)?;
        let env_vars = tool.build_env_vars(input);
        let shell = tool.get_shell(default_shell);
        
        info!("Executing tool '{}' with shell '{}'", tool.name, shell);
        debug!("Command: {}", command);
        debug!("Environment variables: {:?}", env_vars);
        
        let output = match shell.as_str() {
            "bash" => self.execute_bash(&command, &env_vars).await?,
            "sh" => self.execute_sh(&command, &env_vars).await?,
            "zsh" => self.execute_zsh(&command, &env_vars).await?,
            _ => anyhow::bail!("Unsupported shell: {}", shell),
        };
        
        Ok(output)
    }
    
    async fn execute_bash(&self, command: &str, env_vars: &[(String, String)]) -> Result<String> {
        self.execute_with_shell("bash", &["-c", command], env_vars).await
    }
    
    async fn execute_sh(&self, command: &str, env_vars: &[(String, String)]) -> Result<String> {
        self.execute_with_shell("sh", &["-c", command], env_vars).await
    }
    
    async fn execute_zsh(&self, command: &str, env_vars: &[(String, String)]) -> Result<String> {
        self.execute_with_shell("zsh", &["-c", command], env_vars).await
    }
    
    async fn execute_with_shell(&self, shell: &str, args: &[&str], env_vars: &[(String, String)]) -> Result<String> {
        let mut cmd = Command::new(shell);
        cmd.args(args)
            .current_dir(&self.working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        // Add environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
        
        let mut child = cmd.spawn()?;
        
        let status = child.wait().await?;
        
        let mut stdout = String::new();
        let mut stderr = String::new();
        
        if let Some(mut stdout_handle) = child.stdout {
            stdout_handle.read_to_string(&mut stdout).await?;
        }
        
        if let Some(mut stderr_handle) = child.stderr {
            stderr_handle.read_to_string(&mut stderr).await?;
        }
        
        if !status.success() {
            anyhow::bail!(
                "Command failed with exit code {:?}\nstdout: {}\nstderr: {}",
                status.code(),
                stdout,
                stderr
            );
        }
        
        // Combine stdout and stderr for the output
        let output = if stderr.is_empty() {
            stdout
        } else if stdout.is_empty() {
            stderr
        } else {
            format!("{}\n{}", stdout, stderr)
        };
        
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Tool, JsonSchema, Property};
    
    #[tokio::test]
    async fn test_execute_echo() {
        let tool = Tool {
            name: "echo".to_string(),
            description: "Echo a message".to_string(),
            input_schema: vec![JsonSchema::Object {
                properties: vec![
                    ("message".to_string(), Property {
                        prop_type: "string".to_string(),
                        description: "Message to echo".to_string(),
                        pattern: None,
                    })
                ].into_iter().collect(),
                required: vec!["message".to_string()],
            }],
            command: "echo \"$param_message\"".to_string(),
            shell: None,
        };
        
        let input = serde_json::json!({
            "message": "Hello, world!"
        });
        
        let executor = Executor::new();
        let output = executor.execute_tool(&tool, &input, "bash").await.unwrap();
        assert_eq!(output.trim(), "Hello, world!");
    }
}
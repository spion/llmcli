use anyhow::Result;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
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
    tool: &crate::config::ToolConfig,
    input: &serde_json::Value,
    default_shell: &str,
  ) -> Result<String> {
    let command = tool.build_command()?;
    let env_vars = tool.build_env_vars(input);
    let shell = tool.get_shell(default_shell);

    let supported_shells = ["bash", "sh", "zsh", "nu"];
    if !supported_shells.contains(&shell.as_str()) {
      anyhow::bail!("Unsupported shell: {}", shell);
    }
    // Execute the command using the specified shell
    info!("Executing command: {}", command);
    debug!("Using shell: {}", shell);
    let output = self
      .execute_with_shell(&shell, &["-c", &command], &env_vars)
      .await?;

    Ok(output)
  }

  async fn execute_with_shell(
    &self,
    shell: &str,
    args: &[&str],
    env_vars: &[(String, String)],
  ) -> Result<String> {
    let mut cmd = Command::new(shell);
    cmd
      .args(args)
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
  use crate::config::{Property, ToolConfig, ToolDefinition, ToolImplementation};

  #[tokio::test]
  async fn test_execute_echo() {
    let tool = ToolConfig {
      definition: ToolDefinition {
        name: "echo".to_string(),
        description: "Echo a message".to_string(),
        params: vec![(
          "message".to_string(),
          Property {
            prop_type: "string".to_string(),
            description: "Message to echo".to_string(),
            required: Some(true),
          },
        )]
        .into_iter()
        .collect(),
      },
      implementation: ToolImplementation {
        command: "echo \"$param_message\"".to_string(),
        shell: None,
      },
    };

    let input = serde_json::json!({
        "message": "Hello, world!"
    });

    let executor = Executor::new();
    let output = executor.execute_tool(&tool, &input, "bash").await.unwrap();
    assert_eq!(output.trim(), "Hello, world!");
  }
}

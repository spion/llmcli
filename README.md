# llmcli

A command-line tool for using LLMs and making sure other cli tools are made available to them in
a controlled, programmable way.

- Define your tools as scripts
- Pass them the parameters as you see fit
- Restrict the LLM's access to the system

## Premise

Most agentic LLM tools are designed to offer broad capabilities to the LLM, then try to limit
the LLM's access to the system through sandboxing or similar configuration. This approach can lead
to unintended consequences. Furthermore, the configuration provided is often limited and inflexible,
making it difficult to adapt to specific tasks.

`llmcli` takes a different approach. You get full control over the tools through a simple configuration
file. Every tool is an arbitrary shell script, ran with the shell of your choice. You can define both
the input schema and how the command uses those arguments through environment variables.

## Example

Here's an example of a simple `llmcli` configuration file that defines a couple of tools:

```yaml

shell: "bash"  # or "nushell", "zsh", etc - this refers to the shell used to execute commands
tools:
  - name: "git clone"
    description: "Clone a git repository"
    input_schema:
      - type: object
        properties:
          repo_url:
            type: string
            description: "URL of the git repository to clone"
            pattern: "https?://.*"  # Regex to validate the URL
    command: |
      git clone $param_repo_url
    shell: "bash"  # Can override shell for individual commands
  - name: "read file"
    description: "Read a file and print its contents"
    input_schema:
      - type: object
        properties:
          file_path:
            type: string
            description: "Path to the file to read"
            pattern: "^$PWD/.+"  # Regex to ensure the file is within the current working directory
    command: |
      cat "$param_file_path"

    - name: "patcher apply"
      description: "Apply an aider-style search replace patch to a file"
      input_schema:
        - type: object
          properties:
            file_path:
              type: string
              description: "Path to the file to patch"
              pattern: "^$PWD/.+"  # Regex to ensure the file is within the current working directory
            patch:
              type: string
              description: "The patch to apply, in the form of a search and replace string"
      command: |
        patcher apply "$param_file_path" --patch "$param_patch"
```

## Installation

For now, `llmcli` is only available from git. You can install it using `cargo`:

```bash
cargo install --git https://github.com/spion/llmcli.git llmcli
```

## Usage

1. Set environment variables:

```bash
# OpenAI-compatible endpoint
export LLM_CLI_ENDPOINT="https://api.openai.com/v1/chat/completions"
export LLM_CLI_TOKEN="your-api-key"
export LLM_CLI_MODEL="gpt-4.1"
```

2. Run with a config file:

```bash
cat prompt.txt | llmcli --config ../example/config.yaml
```

`llmcli` is non-interactive: Its meant to be used in automated workflows where there are no users
to answer questions or provide input.

## Example Config

See the [example directory](./example) for a sample configuration with basic tools.

## Status

This project is in early development.

## Planned features

- [ ] Support for nushell
- [ ] Command discovery: allow scripts to return JSON with available sub-commands and their schemas
- [ ] mcp server mode: allow llmcli to run as a server that can be used by other tools
- [ ] support for Claude-compatible API endpoints
- [ ] support for response caching

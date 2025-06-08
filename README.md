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
  - name: "list_files"
    description: "List files in a directory"
    input_schema:
      - type: object
        properties:
          path:
            type: string
            description: "Path to the directory (relative to current directory)"
        required:
          - path
    command: |
      real_path=$(realpath "$param_path")
      # Ensure its within the current directory
      if [[ "$real_path/" != $(pwd)/* ]]; then
        echo "Error: Path must be within the current directory."
        exit 1
      fi
      ls -la "$param_path"

  - name: "read_file"
    description: "Read the contents of a file"
    input_schema:
      - type: object
        properties:
          file_path:
            type: string
            description: "Path to the file to read"
        required:
          - file_path
    command: |
      real_path=$(realpath "$param_file_path")
      # Ensure its within the current directory
      if [[ "$real_path/" != $(pwd)/* ]]; then
        echo "Error: File path must be within the current directory."
        exit 1
      fi
      cat "$param_file_path"

  - name: "write_file"
    description: "Write content to a file"
    input_schema:
      - type: object
        properties:
          file_path:
            type: string
            description: "Path to the file to write"
          content:
            type: string
            description: "Content to write to the file"
        required:
          - file_path
          - content
    command: |
      real_path=$(realpath "$param_file_path")
      # Ensure its within the current directory
      if [[ "$real_path/" != $(pwd)/* ]]; then
        echo "Error: File path must be within the current directory."
        exit 1
      fi
      echo "$param_content" > "$param_file_path"
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

This project is in early development / proototype stage. The core functionality as described in
the README has been implemented in the prototype under the `cli` directory, but its highly likely
it will undergo significant restructuring in the future.

## Planned features

- [ ] Support for nushell
- [ ] Command discovery: allow scripts to return JSON with available sub-commands and their schemas
- [ ] mcp server mode: allow llmcli to run as a server that can be used by other tools
- [ ] support for Claude-compatible API endpoints and Google's Gemini API endpoints
  - [ ] support for explicit prompt caching with Claude

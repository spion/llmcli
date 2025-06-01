Initial prototype starts with just simple command config.

Next stage implements tool discovery, and the specific subcommand for nushell tool discovery

Finally we have a mcp server mode which instead of running the LLM will provide a server

# Architecture to complete above plan

- cmd-server: takes a config and creates a "service" class that accepts tool calls,
  dispatches them (possibly in parallel) to the configured tools (validating any arg patterns),
  and returns the results.

- client: reads env vars:
  - LLM_CLI_ENDPOINT
  - LLM_CLI_HEADER_NAME=value
  - LLM_CLI_TOKEN=value
  - provides an event streaming client to the LLM endpoint (deserialized messages)

- cli (the llm loop, and client)
  - reads prompt from stdin
  - reads tools from specified config file
  - displays any llm text output in stdout
  - displays any tool calls in stdout
  - writes every message, tool response and llm response to a JSON log file


- nushell-discover
  - runs the command with --help
  - parses the output to discover available flags and sub-commands
  - condenses them into a tool definition

- mcp-server
  - takes the command server and exposes it as a MCP endpoint


- core
  - defines common types and traits


Libraries to use:


- tokio - we are using async Rust, to support parallel tool calls when needed
- tracing_subscriber - for structured logging
- serde_json - for JSON serialization/deserialization
- clap - for command line argument parsing
- https://github.com/modelcontextprotocol/rust-sdk for MCP
- reqwest-middleware for HTTP client, with retry and backoff middleware
- eventsource_stream for SSE support and realtime text output
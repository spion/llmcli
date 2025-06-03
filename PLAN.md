Initial prototype starts with just simple command config.

Next stage implements tool discovery, and the specific subcommand for nushell tool discovery

Finally we have a mcp server mode which instead of running the LLM will provide a server

# Architecture to complete above plan

- cmd-server: takes a config and creates a "service" class that accepts tool calls,
  dispatches them (possibly in parallel) to the configured tools and returns the results.

- client: reads env vars
  - LLM_CLI_ENDPOINT
  - LLM_CLI_HEADER_NAME=value
  - LLM_CLI_TOKEN=value
  - provides an event streaming client to the LLM endpoint (deserialized messages)
  - TODO: replace with async-openai + a claude client, behind a trait / facade


- cli (the llm loop, and client)
  - reads prompt from stdin
  - reads tools from specified config file
  - displays any llm text output in stdout
  - displays any tool calls in stdout
  - writes every message, tool response and llm response to a JSON log file
    - logging disabled by default

- nushell-discover
  - implements discover protocol for nushell
  - runs the main command with --help
  - parses the output to discover available flags and sub-commands
    - if present, each sub-commands generates its own tool definition
  - condenses into tool definitions

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
- api communication
  - reqwest-middleware for HTTP client, with retry and backoff middleware
  - eventsource_stream for SSE support and realtime text output
  - TODO: replace both with async-openai, which already has all the above features
  - TODO: investigate clust (seems a bit high level)

# vlm-mcp server

MCP server for Vision Language Model integration. Supports all major MCP transport protocols with configurable security (insecure, TLS, mTLS).

## Features

- **Transports**: stdio, Streamable HTTP, legacy SSE
- **Security**: Insecure, TLS, and mTLS for persistent transports
- **VLM backends**: OpenAI-compatible API, Ollama native
- **Streaming**: Token-by-token output where supported
- **DoS protection**: Configurable image size limits
- **Library-first**: Use as an in-process library or standalone binary

## Configuration

Three-tier config with precedence: **CLI flag > env var > YAML config file**.

### Naming Convention

`__` (double underscore) separates struct paths:

| Source | Example |
|--------|---------|
| YAML field | `vlm.endpoint` |
| Env var | `VLM_MCP__VLM__ENDPOINT` |
| CLI flag | `--vlm__endpoint` |

### Example YAML

```yaml
server:
  transport: "stdio"
  bind: "127.0.0.1"
  port: 8080

vlm:
  endpoint: "http://localhost:11434"
  model: "qwen2.5-vl:72b"
  timeout_seconds: 120
```

See `config.example.yaml` for the full schema.

## Building

```bash
# Build all targets
cargo build --release

# Build for static Linux (musl)
cargo build --release --target x86_64-unknown-linux-musl
```

## Running

### stdio (default — for local agents)

```bash
vlm-mcp-server --vlm__endpoint http://localhost:11434 --vlm__model qwen2.5-vl:72b
```

### Streamable HTTP

```bash
vlm-mcp-server --transport http \
    --vlm__endpoint http://localhost:11434 \
    --vlm__model qwen2.5-vl:72b
```

### With TLS

```bash
vlm-mcp-server --transport http \
    --server__tls__cert /path/to/cert.pem \
    --server__tls__key /path/to/key.pem \
    --vlm__endpoint http://localhost:11434 \
    --vlm__model qwen2.5-vl:72b
```

## License

MIT

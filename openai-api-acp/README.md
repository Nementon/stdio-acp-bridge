# openai-api-acp — OpenAI-Compatible ACP Proxy

A lightweight Rust bridge between the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/) JSON-RPC interface and any **OpenAI-compatible HTTP API** — including [vLLM](https://github.com/vllm-project/vllm), [LM Studio](https://lmstudio.ai/), [Ollama](https://ollama.com/), [OpenRouter](https://openrouter.ai/), and OpenAI itself.

It acts as an ACP agent backend: `session/prompt` requests are translated into `POST /v1/chat/completions` calls, and streamed tokens are forwarded back as ACP `session/update` notifications.

## Windows Intelligent Terminal Compatibility

Fully compatible with [**Windows Intelligent Terminal**](https://github.com/microsoft/intelligent-terminal). Register the compiled binary as an agent in Intelligent Terminal's settings to connect any local or remote OpenAI-compatible model to the Agent Pane.

## Features

- **Full ACP session lifecycle** — `session/new`, `session/load`, `session/resume`, `session/fork`, `session/close`, `session/delete`, `session/cancel`
- **Streaming responses** — SSE tokens from the upstream API are forwarded as incremental `session/update` notifications
- **Session persistence** — conversation history saved to disk; sessions survive restarts
- **Model selection** — `session/setModel` and `session/setConfigOption` for runtime model switching
- **Auto-discovery** — queries `GET /v1/models` on startup to populate the model list
- **JSON-RPC Debug Logging** — Capture ingress and egress traffic for debugging.

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- A running OpenAI-compatible API endpoint

## Usage via Orchestrator

While `openai-api-acp` can be compiled independently, it is natively integrated into the `stdio-acp-bridge` orchestrator. The easiest way to run it is to pass the `--bridge openai` parameter or configure it in `stdio-acp-bridge.yml`.

```powershell
# Using the root orchestrator directly pointing to Ollama:
$env:STDIO_ACPB_OPENAI_API_BASE = "http://localhost:11434/v1"
$env:STDIO_ACPB_OPENAI_API_KEY  = "ollama"
cargo run --release -- --bridge openai

# Or run via a configured YAML orchestrator config:
cargo run --release -- --config stdio-acp-bridge.yml
```

## Usage as Standalone Binary

You can also bypass the orchestrator completely and run `openai-api-acp` directly as a standalone binary. This is useful if you only want to deploy this specific proxy.

```powershell
# Build only the standalone binary
cargo build --release -p openai-api-acp

# Windows — point at a local Ollama instance using env vars
$env:OPENAI_API_BASE = "http://localhost:11434/v1"
$env:OPENAI_API_KEY  = "ollama"
.\target\release\openai-api-acp.exe

# Or pass flags directly
.\target\release\openai-api-acp.exe --api-base http://localhost:11434/v1 --api-key ollama
```

## Configuration

If invoked as a standalone CLI or configured via the YAML orchestrator, it supports the following options:

| YAML Key | Env variable | Default | Description |
|---|---|---|---|
| `api_base` | `OPENAI_API_BASE` | `http://localhost:8000/v1` | Base URL of the OpenAI-compatible API |
| `api_key` | `OPENAI_API_KEY` | `dummy-key` | API key sent in the `Authorization` header |
| `state_dir` | `STDIO_ACPB_STATE_DIR` | `~/.stdio-acpb/openai-acp` | Directory for session state (`sessions.json`) |
| `debug_log` | `STDIO_ACPB_DEBUG_LOG` | *(none)* | Path to a file where raw JSON-RPC traffic is logged |

## ACP Methods Supported

| Method | Description |
|---|---|
| `initialize` | Advertise capabilities and available models |
| `session/new` | Create a session (optionally with `cwd`, `title`, `modelId`) |
| `session/load` | Load a persisted session by ID |
| `session/resume` | Resume a session and return its current config |
| `session/fork` | Fork a session, copying its message history |
| `session/list` | List all sessions |
| `session/close` | Remove a session from memory |
| `session/delete` | Permanently delete a session |
| `session/prompt` | Send a prompt; streams `session/update` notifications from the model |
| `session/cancel` | Cancel an in-flight `session/prompt` |
| `session/setModel` | Switch the model for a session |
| `session/setConfigOption` | Set a named config option (currently `model`) |

## Development & Testing

```bash
cargo test        # run unit tests and E2E JSON-RPC bridge tests
```

---

*Part of the [`stdio-acp-bridge`](../README.md) suite.*

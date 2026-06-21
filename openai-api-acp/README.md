# openai-api-acp — OpenAI-Compatible ACP Proxy

A lightweight Rust bridge between the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/) JSON-RPC interface and any **OpenAI-compatible HTTP API** — including [vLLM](https://github.com/vllm-project/vllm), [LM Studio](https://lmstudio.ai/), [Ollama](https://ollama.com/), [OpenRouter](https://openrouter.ai/), and OpenAI itself.

It acts as an ACP agent backend: `session/prompt` requests are translated into `POST /v1/chat/completions` calls, and streamed tokens are forwarded back as ACP `session/update` notifications.

## Windows Intelligent Terminal Compatibility

Fully compatible with [**Windows Intelligent Terminal**](https://github.com/microsoft/intelligent-terminal). Register the compiled binary as an agent in Intelligent Terminal's settings to connect any local or remote OpenAI-compatible model to the Agent Pane.

## Features

- **Full ACP session lifecycle** — `session/new`, `session/load`, `session/resume`, `session/fork`, `session/close`, `session/delete`
- **Streaming responses** — SSE tokens from the upstream API are forwarded as incremental `session/update` notifications
- **Session persistence** — conversation history saved to disk; sessions survive restarts
- **Model selection** — `session/setModel` and `session/setConfigOption` for runtime model switching
- **Auto-discovery** — queries `GET /v1/models` on startup to populate the model list

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- A running OpenAI-compatible API endpoint

## Installation

```powershell
# Windows
cd openai-api-acp
cargo build --release
# Binary: target\release\openai-api-acp.exe
```

```bash
# Linux / macOS
cd openai-api-acp
cargo build --release
# Binary: target/release/openai-api-acp
```

## CLI Flags & Environment Variables

| Flag | Env variable | Default | Description |
|---|---|---|---|
| `--api-base` | `OPENAI_API_BASE` | `http://localhost:8000/v1` | Base URL of the OpenAI-compatible API |
| `--api-key` | `OPENAI_API_KEY` | `dummy-key` | API key sent in the `Authorization` header |
| `--state-dir` | `STDIO_ACPB_STATE_DIR` | `~/.stdio-acpb/openai-acp` | Directory for session state (`sessions.json`) |

## Usage

The proxy is launched by an ACP client (such as Windows Intelligent Terminal) and communicates over stdin/stdout.

```powershell
# Windows — point at a local Ollama instance
$env:OPENAI_API_BASE = "http://localhost:11434/v1"
$env:OPENAI_API_KEY  = "ollama"
.\target\release\openai-api-acp.exe

# Or pass flags directly
.\target\release\openai-api-acp.exe --api-base http://localhost:11434/v1 --api-key ollama
```

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
| `session/setModel` | Switch the model for a session |
| `session/setConfigOption` | Set a named config option (currently `model`) |

---

*Part of the [`stdio-acp-bridge`](../README.md) suite.*

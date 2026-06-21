# agy-acp — Google Antigravity STDIO–ACP Bridge

`agy-acp` is a high-performance Rust bridge between standard I/O (STDIO) and the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/). It wraps the Google Antigravity CLI (`agy`) and exposes it as a fully ACP-compliant agent backend, translating JSON-RPC session messages into `agy` subprocess invocations and streaming Gemini responses back as ACP `session/update` notifications.

## Forked From

- [OpenAB](https://github.com/openabdev/openab/tree/main) — [`agy-acp`](https://github.com/openabdev/openab/tree/59be108013725b363f72a7bfd7925349df56f5a5/agy-acp)

## Windows Intelligent Terminal Compatibility

`agy-acp` is fully compatible with [**Windows Intelligent Terminal**](https://github.com/microsoft/intelligent-terminal). Register the compiled binary as an agent in Intelligent Terminal's settings to bring Google Gemini into the Agent Pane alongside other ACP agents.

## Features

- **Full ACP session lifecycle** — `session/new`, `session/load`, `session/resume`, `session/fork`, `session/close`, `session/delete`, `session/cancel`
- **Real-time streaming** — polls Gemini's SQLite conversation database and emits incremental `session/update` notifications as the model generates output
- **Session persistence** — saves and restores conversation IDs and step indices across restarts
- **Model selection** — `session/setModel` and `session/setConfigOption` for switching Gemini models at runtime
- **Cancellation** — per-session cancellation via `session/cancel` or `Ctrl-C`
- **JSON-RPC Debug Logging** — Built-in support to capture complete `<-` ingress and `->` egress traffic for debugging.
- **Windows-native** — correct path handling, `USERPROFILE`-based home directory resolution

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- [`agy`](https://developers.google.com/gemini) — Google Antigravity CLI, must be on `PATH`
- A valid Gemini account / API key (via `GEMINI_API_KEY` or `~/.gemini/antigravity-cli/settings.json`)

## Usage via Orchestrator

While `agy-acp` can be compiled independently, it is natively integrated into the `stdio-acp-bridge` orchestrator. The easiest way to run it is to pass the `--bridge agy` parameter or configure it in `stdio-acp-bridge.yml`.

```powershell
# Using the root orchestrator
cargo run --release -- --bridge agy

# With explicit state and conversations directories via the unified orchestrator configuration:
cargo run --release -- --config stdio-acp-bridge.yml
```

## Usage as Standalone Binary

You can also bypass the orchestrator completely and run `agy-acp` directly as a standalone binary. This is useful if you only want to deploy this specific bridge without the surrounding orchestrator framework.

```powershell
# Build only the standalone binary
cargo build --release -p agy-acp

# Run directly
.\target\release\agy-acp.exe

# With explicit command line arguments
.\target\release\agy-acp.exe --state-dir "C:\path\to\state" --conversations-dir "C:\path\to\conversations"
```

## CLI Flags & Environment Variables

If invoked as a standalone CLI or configured via the YAML orchestrator, it supports the following options:

| YAML Key | Env variable | Default | Description |
|---|---|---|---|
| `state_dir` | `STDIO_ACPB_STATE_DIR` | `~/.stdio-acpb/agy-acp` | Directory for session state (`sessions.json`) |
| `conversations_dir` | `STDIO_ACPB_CONVERSATIONS_DIR` | `~/.gemini/antigravity-cli/conversations` | Directory containing Gemini conversation `.db` files |
| `debug_log` | `STDIO_ACPB_DEBUG_LOG` | *(none)* | Path to a file where raw JSON-RPC traffic is logged |

## ACP Methods Supported

| Method | Description |
|---|---|
| `initialize` | Advertise agent capabilities and available models |
| `session/new` | Create a new session (optionally with `cwd`, `title`, `modelId`) |
| `session/load` | Load a previously created session by ID |
| `session/resume` | Resume a session and return its current config |
| `session/fork` | Fork a session, copying its conversation database |
| `session/list` | List all sessions (filterable by `cwd` / `workingDirectory`) |
| `session/close` | Remove a session from memory |
| `session/delete` | Permanently delete a session and its persisted state |
| `session/prompt` | Send a prompt; streams `session/update` notifications while `agy` runs |
| `session/cancel` | Cancel an in-flight `session/prompt` |
| `session/setModel` | Switch the Gemini model for a session |
| `session/setConfigOption` | Set a named config option (currently `model`) |

## Development & Testing

The crate supports comprehensive local testing, including End-to-End JSON-RPC tests:

```bash
cargo test        # run unit and end-to-end integration tests (no auth required)
cargo test -- --ignored  # run integration tests (require auth + built binary)
```

---

*Part of the [`stdio-acp-bridge`](../README.md) suite.*

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
- **Windows-native** — correct path handling, `USERPROFILE`-based home directory resolution

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- [`agy`](https://developers.google.com/gemini) — Google Antigravity CLI, must be on `PATH`
- A valid Gemini account / API key (via `GEMINI_API_KEY` or `~/.gemini/antigravity-cli/settings.json`)

## Installation

```powershell
# Windows
cd agy-acp
cargo build --release
# Binary: target\release\agy-acp.exe
```

```bash
# Linux / macOS
cd agy-acp
cargo build --release
# Binary: target/release/agy-acp
```

## Usage

`agy-acp` is an ACP server — it reads JSON-RPC requests from stdin and writes responses and notifications to stdout. It is intended to be launched by an ACP client (such as Windows Intelligent Terminal) rather than used interactively.

```powershell
# Windows — launch directly
.\target\release\agy-acp.exe

# With explicit state and conversations directories
.\target\release\agy-acp.exe --state-dir $env:APPDATA\agy-acp --conversations-dir $env:USERPROFILE\.gemini\antigravity-cli\conversations

# With a debug log
.\target\release\agy-acp.exe --debug-log C:\Temp\agy-acp.log
```

## CLI Flags & Environment Variables

| Flag | Env variable | Default | Description |
|---|---|---|---|
| `--state-dir` | `STDIO_ACPB_STATE_DIR` | `~/.stdio-acpb/agy-acp` | Directory for session state (`sessions.json`) |
| `--conversations-dir` | `STDIO_ACPB_CONVERSATIONS_DIR` | `~/.gemini/antigravity-cli/conversations` | Directory containing Gemini conversation `.db` files |
| `--debug-log` / `--log-file` | — | *(none)* | Path to a file where raw JSON-RPC traffic is logged |

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

## Development

```bash
cargo test        # run unit tests (no auth required)
cargo test -- --ignored  # run integration tests (require auth + built binary)
```

---

*Part of the [`stdio-acp-bridge`](../README.md) suite.*
PS D:\LLMs\tools\stdio-acp-bridge\agy-acp>
PS D:\LLMs\tools\stdio-acp-bridge\agy-acp> Set-Content README.md -Value "# agy-acp (Google Antigravity STDIO ACP Bridge)

 `agy-acp` is a high-performance Rust utility that acts as a bridge between standard I/O (STDIO) and the Antigravity Communication Protocol (ACP). It allows developers to interact with ACP-compliant services or tools directly from the command line without needing to handle complex protocol framing manually.

 ## Features

 - **Transparent Bridging**: Seamlessly maps standard input/output to ACP messages.
 - **High Performance**: Built with Rust to ensure low-latency communication.
 - **STDIO Compatible**: Easily pipeable to other command-line tools.
 - **ACP Compliance**: Handles the heavy lifting of the Antigravity Communication Protocol.

 ## Prerequisites

 - Rust and Cargo installed (Required for building from source).
 - A target ACP-compliant service or tool.

 ## Installation

 To build the project from source, navigate to the project root directory and run:

 ```bash
 cargo build --release
 ```

 The resulting binary will be located in `target/release/agy-acp`.

 ## Usage

 You can use `agy-acp` by piping standard input into it or redirecting its output.

 ### Basic Example
 To send a message to an ACP service:
 ```bash
 echo "Your message here" | ./target/release/agy-acp
 ```

 ### Interactive Use
 To use it in an interactive session:
 ```bash
 ./target/release/agy-acp
 ```

 ## Development

 To contribute to this project:
 1. Clone the repository.
 2. Run `cargo test` to ensure existing functionality is preserved.
 3. Submit a pull request with your changes.

 ---
 *This tool is part of the `stdio-acp-bridge` suite."

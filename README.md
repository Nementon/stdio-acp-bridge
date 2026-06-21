# stdio-acp-bridge

A collection of STDIO-to-ACP (Agent Client Protocol) bridge utilities that let you connect AI agent backends to any [ACP-compatible](https://agentclientprotocol.com/get-started/agents) client — including **Windows Intelligent Terminal**.

## What is ACP?

The [Agent Client Protocol](https://agentclientprotocol.com/) is an open standard for communicating with AI agent CLIs over a simple JSON-RPC interface. Clients speak ACP; this repository provides the bridge adapters that translate STDIO-based agent tools into ACP-compliant backends.

## Windows Intelligent Terminal Compatibility

All bridges in this repository are fully compatible with [**Windows Intelligent Terminal**](https://github.com/microsoft/intelligent-terminal) — Microsoft's experimental AI-native terminal built on Windows Terminal.

Intelligent Terminal natively integrates with any ACP-compliant agent CLI. The bridges in this repository allow you to plug in additional agent backends (Google Antigravity / Gemini, OpenAI-compatible endpoints, and more) directly into Intelligent Terminal's Agent Pane without any custom wiring.

### Quick setup with Intelligent Terminal

1. Install Intelligent Terminal from the [Microsoft Store](https://apps.microsoft.com/detail/9NMQC2SSJX24) or via WinGet:

   ```powershell
   winget install --id Microsoft.IntelligentTerminal -e
   ```

2. Build the bridge you want to use (see per-project instructions below).

3. Register the compiled binary as an agent in Intelligent Terminal's settings, pointing to the bridge executable. Intelligent Terminal will manage the process lifecycle and communicate over STDIO using ACP.

> **Requirements:** Windows 10 2004 (19041) or later. Rust toolchain required to build from source.

---

## Projects

### [`agy-acp`](./agy-acp/)

A high-performance Rust bridge between STDIO and the **Google Antigravity (Gemini)** CLI (`agy`). It wraps the `agy` subprocess, surfaces streaming conversation updates as ACP `session/update` notifications, and persists session state across restarts.

**Key features:**
- Full ACP session lifecycle: `session/new`, `session/load`, `session/resume`, `session/fork`, `session/close`, `session/delete`
- Real-time streaming of Gemini responses via SQLite conversation database polling
- Session state persistence and restoration
- Cancellation support (`session/cancel`)
- Windows-native path handling

```powershell
cd agy-acp
cargo build --release
# Binary: agy-acp\target\release\agy-acp.exe
```

### [`openai-api-acp`](./openai-api-acp/)

A lightweight ACP proxy that bridges ACP `session/prompt` calls to any **OpenAI-compatible HTTP API** — including [vLLM](https://github.com/vllm-project/vllm), [LM Studio](https://lmstudio.ai/), [Ollama](https://ollama.com/), and OpenAI itself.

**Key features:**
- Translates ACP JSON-RPC to `POST /v1/chat/completions`
- Streams responses back as ACP notifications
- Configurable via environment variables

```powershell
cd openai-api-acp
cargo build --release
# Binary: openai-api-acp\target\release\openai-api-acp.exe
```

**Configuration:**

| Variable | Default | Description |
|---|---|---|
| `OPENAI_API_BASE` | `http://localhost:8000/v1` | Base URL of the OpenAI-compatible API |
| `OPENAI_API_KEY` | `dummy-key` | API key |

---

## Submodules

| Submodule | Description |
|---|---|
| [`agent-client-protocol`](./agent-client-protocol/) | ACP specification and schema |
| [`intelligent-terminal`](./intelligent-terminal/) | Windows Intelligent Terminal source (Microsoft) |

Initialize submodules after cloning:

```powershell
git submodule update --init --recursive
```

---

## Building

Prerequisites: [Rust](https://rustup.rs/) (stable toolchain).

```powershell
# Build all workspace members
cargo build --release
```

---

## License

See [LICENSE](./LICENSE).

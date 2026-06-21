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

2. Build the orchestrator bridge using Cargo.

3. Register the compiled binary as an agent in Intelligent Terminal's settings, pointing to the bridge executable and its YAML configuration file. Intelligent Terminal will manage the process lifecycle and communicate over STDIO using ACP.

> **Requirements:** Windows 10 2004 (19041) or later. Rust toolchain required to build from source.

---

## The Orchestrator

The project provides a single unified `stdio-acp-bridge` orchestrator. The root binary manages routing to multiple backend bridges dynamically based on its configuration. It supports multi-layered configurations:

1. **YAML File:** Passed via `-c` / `--config <FILE>`, falling back to `stdio-acp-bridge.yml` in the current working directory.
2. **Environment Variables:** Options prefixed with `STDIO_ACPB_`.
3. **CLI Arguments:** Hard overrides like `--bridge`.

### YAML Configuration Example (`stdio-acp-bridge.yml`)

The orchestrator determines which bridge to execute using the `bridge` field. Each sub-bridge has its own dedicated configuration block.

```yaml
# Specify the bridge to use: "agy" or "openai"
bridge: "openai"

# Configuration for the Google Antigravity bridge
agy:
  state_dir: "C:\\path\\to\\custom\\state"
  conversations_dir: "C:\\path\\to\\conversations"
  debug_log: "C:\\Temp\\agy-bridge-debug.log"

# Configuration for the OpenAI proxy bridge
openai:
  api_base: "http://localhost:11434/v1"
  api_key: "ollama"
  state_dir: "C:\\path\\to\\custom\\state"
  debug_log: "C:\\Temp\\openai-bridge-debug.log"
```

You can then run the orchestrator:

```powershell
cargo run --release -- --config stdio-acp-bridge.yml
```

### JSON-RPC Debug Logging
All bridges support full ingress (`<-`) and egress (`->`) JSON-RPC traffic tracing. You can enable this by passing the `--debug-log <FILE>` argument (or setting `debug_log` in the YAML configuration / `STDIO_ACPB_AGY_DEBUG_LOG` / `STDIO_ACPB_OPENAI_DEBUG_LOG` in the environment).

---

## Available Bridges

### [`agy-acp`](./agy-acp/)

A high-performance Rust bridge between STDIO and the **Google Antigravity (Gemini)** CLI (`agy`). It wraps the `agy` subprocess, surfaces streaming conversation updates as ACP `session/update` notifications, and persists session state across restarts.

### [`openai-api-acp`](./openai-api-acp/)

A lightweight ACP proxy that bridges ACP `session/prompt` calls to any **OpenAI-compatible HTTP API** — including [vLLM](https://github.com/vllm-project/vllm), [LM Studio](https://lmstudio.ai/), [Ollama](https://ollama.com/), and OpenAI itself.

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
# Build the unified orchestrator binary
cargo build --release
# The resulting binary is located at target/release/stdio-acp-bridge.exe
```

---

## License

See [LICENSE](./LICENSE).

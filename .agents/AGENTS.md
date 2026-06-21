# stdio-acp-bridge Agent Instructions

This workspace contains bridge utilities for the Agent Client Protocol (ACP) to connect STDIO-based backends (like the Antigravity CLI and OpenAI endpoints) to the Windows Intelligent Terminal.

## Sub-Project Instructions
This repository heavily relies on submodules which contain their own `AGENTS.md` files. Always consult these when modifying their respective areas:
- **agent-client-protocol**: See [`agent-client-protocol/AGENTS.md`](../agent-client-protocol/AGENTS.md) for instructions on modifying the ACP schema and library methods.
- **intelligent-terminal**: See [`intelligent-terminal/AGENTS.md`](../intelligent-terminal/AGENTS.md) for deep architectural documentation of the Windows Terminal integration.
- **wta (Windows Terminal Agent)**: See [`intelligent-terminal/tools/wta/AGENTS.md`](../intelligent-terminal/tools/wta/AGENTS.md) for specific `wta` binary instructions.

## Workspace Layout
- `agy-acp/`: A Rust bridge for the Google Antigravity (Gemini) CLI.
- `openai-api-acp/`: A proxy bridge for OpenAI-compatible HTTP APIs.

## Build Guidelines
- This is a Rust workspace. Use `cargo build --release` from the workspace root to build all bridges.
- If making changes to Rust code, ensure you use `cargo check` or `cargo build` to verify changes.

## General Rules
- Do not modify submodules without explicit instructions.
- Ensure Windows-native path handling in all Rust bridges.
- Follow the ACP JSON-RPC 2.0 specifications when updating bridge communications.

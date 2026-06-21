use clap::Parser;
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing_subscriber::EnvFilter;

pub mod adapter;
pub mod types;

use adapter::Adapter;
use types::{JsonRpcRequest, JsonRpcResponse};

#[derive(Parser, Debug, Clone, serde::Deserialize)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(
        long,
        env = "OPENAI_API_BASE",
        default_value = "http://localhost:8000/v1"
    )]
    #[serde(default = "default_api_base")]
    pub api_base: String,

    #[arg(long, env = "OPENAI_API_KEY", default_value = "dummy-key")]
    #[serde(default = "default_api_key")]
    pub api_key: String,

    #[arg(long, env = "STDIO_ACPB_STATE_DIR")]
    pub state_dir: Option<String>,

    #[arg(long, visible_alias = "log-file")]
    pub debug_log: Option<String>,
}

fn default_api_base() -> String {
    "http://localhost:8000/v1".to_string()
}
fn default_api_key() -> String {
    "dummy-key".to_string()
}

impl Default for Args {
    fn default() -> Self {
        Self {
            api_base: default_api_base(),
            api_key: default_api_key(),
            state_dir: None,
            debug_log: None,
        }
    }
}

pub async fn run(args: Args) {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let adapter = Arc::new(tokio::sync::Mutex::new(Adapter::new(
        args.api_base,
        args.api_key,
        args.state_dir,
    )));
    let debug_log_path = args.debug_log;

    // Warm up models
    {
        let mut guard = adapter.lock().await;
        guard.get_available_models().await;
    }

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Option<String>>();

    // Writer task
    let debug_log_writer = debug_log_path.clone();
    tokio::spawn(async move {
        while let Some(Some(msg)) = out_rx.recv().await {
            if stdout.write_all(msg.as_bytes()).await.is_err() {
                break;
            }
            if stdout.write_all(b"\n").await.is_err() {
                break;
            }
            if stdout.flush().await.is_err() {
                break;
            }
            if let Some(path) = &debug_log_writer {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                    use std::io::Write;
                    let _ = writeln!(f, "-> {}", msg);
                }
            }
        }
    });

    while let Ok(Some(line)) = reader.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        if let Some(path) = &debug_log_path {
            if let Ok(mut log_file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                use std::io::Write;
                let _ = writeln!(log_file, "<- {}", line);
            }
        }

        let req: Result<JsonRpcRequest, _> = serde_json::from_str(&line);
        match req {
            Ok(r) => {
                let id = r.id.unwrap_or(json!(null));
                let method = r.method.unwrap_or_default();
                let params = r.params.unwrap_or(json!({}));

                let mut adapter_guard = adapter.lock().await;

                let resp = match method.as_str() {
                    "initialize" => Some(adapter_guard.handle_initialize(id.clone()).await),
                    "session/new" => Some(adapter_guard.handle_session_new(id.clone(), &params).await),
                    "session/load" => {
                        Some(adapter_guard.handle_session_load(id.clone(), &params).await)
                    }
                    "session/list" => Some(adapter_guard.handle_session_list(id.clone(), &params)),
                    "session/close" => {
                        Some(adapter_guard.handle_session_close(id.clone(), &params))
                    }
                    "session/delete" => {
                        Some(adapter_guard.handle_session_delete(id.clone(), &params))
                    }
                    "session/resume" => {
                        Some(adapter_guard.handle_session_resume(id.clone(), &params))
                    }
                    "session/fork" => Some(adapter_guard.handle_session_fork(id.clone(), &params)),
                    "session/setModel" => {
                        Some(adapter_guard.handle_session_set_model(id.clone(), &params))
                    }
                    "session/setConfigOption" => Some(
                        adapter_guard
                            .handle_session_set_config_option(id.clone(), &params)
                            .await,
                    ),
                    "session/prompt" => {
                        let session_id = params
                            .get("sessionId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                            
                        let prompt_value = params.get("content").or_else(|| params.get("prompt"));
                        let message = match prompt_value {
                            Some(serde_json::Value::String(s)) => s.trim().to_string(),
                            Some(serde_json::Value::Object(obj)) => {
                                if let Some(t) = obj.get("text").and_then(|t| t.as_str()) {
                                    t.trim().to_string()
                                } else {
                                    String::new()
                                }
                            }
                            Some(serde_json::Value::Array(arr)) => arr
                                .iter()
                                .filter_map(|b| {
                                    b.get("text")
                                        .and_then(|t| t.as_str())
                                        .map(|t| t.to_string())
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                                .trim()
                                .to_string(),
                            _ => String::new(),
                        };

                        let tx = out_tx.clone();
                        let adapter_clone = Arc::clone(&adapter);

                        // We must release the lock before executing
                        drop(adapter_guard);

                        tokio::spawn(async move {
                            let mut g = adapter_clone.lock().await;
                            if let Err(e) =
                                g.execute_prompt(&session_id, &message, tx.clone()).await
                            {
                                let err_resp = JsonRpcResponse {
                                    jsonrpc: "2.0",
                                    id: id.clone(),
                                    result: None,
                                    error: Some(
                                        json!({"code": -32603, "message": format!("OpenAI API execution failed: {}", e)}),
                                    ),
                                };
                                let _ = tx.send(Some(serde_json::to_string(&err_resp).unwrap()));
                            } else {
                                let success_resp = JsonRpcResponse {
                                    jsonrpc: "2.0",
                                    id: id.clone(),
                                    result: Some(json!({"stopReason": "end_turn"})),
                                    error: None,
                                };
                                let _ =
                                    tx.send(Some(serde_json::to_string(&success_resp).unwrap()));
                            }
                        });
                        None
                    }
                    "session/cancel" => {
                        None
                    }
                    _ => Some(JsonRpcResponse {
                        jsonrpc: "2.0",
                        id: id.clone(),
                        result: None,
                        error: Some(json!({ "code": -32601, "message": "Method not found" })),
                    }),
                };

                if let Some(resp) = resp {
                    let _ = out_tx.send(Some(serde_json::to_string(&resp).unwrap()));
                }
            }
            Err(e) => {
                let err_resp = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: json!(null),
                    result: None,
                    error: Some(
                        json!({ "code": -32700, "message": format!("Parse error: {}", e) }),
                    ),
                };
                let _ = out_tx.send(Some(serde_json::to_string(&err_resp).unwrap()));
            }
        }
    }
}

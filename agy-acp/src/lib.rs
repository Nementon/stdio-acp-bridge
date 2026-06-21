pub mod adapter;
pub mod db;
pub mod protobuf;
pub mod streaming;
pub mod types;

use adapter::Adapter;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use types::*;

impl Adapter {
    /// Execute prompt subprocess without holding any adapter lock.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_prompt(
        id: Value,
        session_id: &str,
        args: Vec<String>,
        snapshot: Option<HashSet<String>>,
        initial_conv_id: Option<String>,
        initial_step_idx: i64,
        working_dir: String,
        conversations_dir: PathBuf,
        cancelled: Arc<AtomicBool>,
        out_tx: mpsc::UnboundedSender<Option<String>>,
    ) -> PromptOutput {
        let spawn_result = Command::new(Adapter::agy_bin())
            .args(&args)
            .env("PATH", Adapter::augmented_path())
            .current_dir(&working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let mut child = match spawn_result {
            Ok(child) => child,
            Err(e) => {
                return PromptOutput {
                    response_lines: vec![serde_json::to_string(&JsonRpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(
                            json!({"code":-32000,"message":format!("failed to run agy: {e}")}),
                        ),
                    })
                    .unwrap()],
                    session_update: None,
                };
            }
        };

        let mut stdout_handle = child.stdout.take();
        let stdout_reader = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stdout) = stdout_handle.take() {
                let _ = stdout.read_to_end(&mut buf).await;
            }
            buf
        });

        let mut stderr_handle = child.stderr.take();
        let stderr_reader = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stderr) = stderr_handle.take() {
                let _ = stderr.read_to_end(&mut buf).await;
            }
            buf
        });

        let streaming_state = Arc::new(Mutex::new(StreamingState {
            conversation_id: initial_conv_id,
            base_step_idx: initial_step_idx,
            last_step_idx: initial_step_idx,
            emitted_len: HashMap::new(),
            emitted_tool_steps: HashSet::new(),
            had_updates: false,
        }));

        let stop_polling = Arc::new(AtomicBool::new(false));
        let poll_conversations_dir = conversations_dir.clone();
        let poll_snapshot = snapshot.clone();
        let poll_session_id = session_id.to_string();
        let poll_state = Arc::clone(&streaming_state);
        let poll_stop = Arc::clone(&stop_polling);
        let poll_tx = out_tx.clone();

        let poller = std::thread::spawn(move || {
            while !poll_stop.load(Ordering::SeqCst) {
                let lines = streaming::poll_streaming_delta(
                    &poll_conversations_dir,
                    poll_snapshot.as_ref(),
                    &poll_session_id,
                    &poll_state,
                );
                for line in lines {
                    if poll_tx.send(Some(line)).is_err() {
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });

        let _stop_guard = StopGuard(Arc::clone(&stop_polling));

        let mut was_cancelled = false;
        let result = tokio::select! {
            result = child.wait() => result,
            _ = async {
                while !cancelled.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            } => {
                was_cancelled = true;
                let _ = child.kill().await;
                child.wait().await
            }
        };

        let _ = stdout_reader.await;
        let stderr_bytes = stderr_reader.await.unwrap_or_default();

        stop_polling.store(true, Ordering::SeqCst);
        let _ = poller.join();

        // Final poll
        {
            let lines = streaming::poll_streaming_delta(
                &conversations_dir,
                snapshot.as_ref(),
                session_id,
                &streaming_state,
            );
            for line in lines {
                let _ = out_tx.send(Some(line));
            }
        }

        let (bound_conv_id, new_step_idx, had_updates) = {
            let guard = streaming_state.lock().unwrap();
            (
                guard.conversation_id.clone(),
                guard.last_step_idx,
                guard.had_updates,
            )
        };

        let session_update = Some((bound_conv_id.clone(), new_step_idx));

        let stop_reason = if was_cancelled {
            "cancelled"
        } else if result.as_ref().map(|s| !s.success()).unwrap_or(false) {
            "error"
        } else {
            "end_turn"
        };

        match result {
            Ok(status) => {
                let stderr_text = String::from_utf8_lossy(&stderr_bytes);
                if !stderr_text.is_empty() {
                    eprintln!("[agy-acp] agy stderr: {}", stderr_text.trim_end());
                }
                if !was_cancelled && !status.success() {
                    eprintln!("[agy-acp] WARN: agy exited with status: {}", status);
                    if !had_updates {
                        let msg = if stderr_text.is_empty() {
                            format!("agy exited with status: {}", status)
                        } else {
                            format!("agy failed: {}", stderr_text.trim_end())
                        };
                        return PromptOutput {
                            response_lines: vec![serde_json::to_string(&JsonRpcResponse {
                                jsonrpc: "2.0",
                                id,
                                result: None,
                                error: Some(json!({"code":-32000,"message":msg})),
                            })
                            .unwrap()],
                            session_update,
                        };
                    }
                }
            }
            Err(e) => {
                return PromptOutput {
                    response_lines: vec![serde_json::to_string(&JsonRpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(
                            json!({"code":-32000,"message":format!("failed to wait for agy: {e}")}),
                        ),
                    })
                    .unwrap()],
                    session_update,
                };
            }
        }

        PromptOutput {
            response_lines: vec![serde_json::to_string(&JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({ "stopReason": stop_reason })),
                error: None,
            })
            .unwrap()],
            session_update,
        }
    }
}

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, env = "STDIO_ACPB_STATE_DIR")]
    state_dir: Option<String>,

    #[arg(long, env = "STDIO_ACPB_CONVERSATIONS_DIR")]
    conversations_dir: Option<String>,

    #[arg(long, visible_alias = "log-file")]
    debug_log: Option<String>,
}

pub async fn run() {
    let args = Args::parse();
    let debug_log_path = args.debug_log;
    let state_dir = args.state_dir;
    let conv_dir = args.conversations_dir;

    let adapter = Arc::new(tokio::sync::Mutex::new(Adapter::new(state_dir, conv_dir)));

    // Load cached models or use static fallback immediately to avoid blocking initialization.
    {
        let mut guard = adapter.lock().await;
        if let Some(cached) = guard.load_cached_models() {
            guard.available_models = Some(cached);
        } else {
            guard.available_models = Some(Adapter::static_fallback_models());
        }
    }

    // Spawn a background task to refresh the models list cache asynchronously.
    let adapter_clone = Arc::clone(&adapter);
    tokio::spawn(async move {
        let models = tokio::task::spawn_blocking(Adapter::fetch_available_models)
            .await
            .unwrap_or_default();
        if !models.is_empty() {
            let mut guard = adapter_clone.lock().await;
            eprintln!(
                "[agy-acp] fetched {} models from `agy models`, updating cache",
                models.len()
            );
            guard.save_models_cache(&models);
            guard.available_models = Some(models);
        }
    });

    let active_cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Option<String>>();

    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) if !l.trim().is_empty() => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
                _ => {}
            }
        }
    });

    let mut stdout = io::stdout();
    let mut stdin_open = true;
    let mut pending_prompts = 0usize;

    loop {
        if !stdin_open && pending_prompts == 0 {
            break;
        }

        let line = if stdin_open {
            tokio::select! {
                output = out_rx.recv() => {
                    match output {
                        Some(Some(line)) => {
                            let _ = writeln!(stdout, "{}", line);
                            let _ = stdout.flush();
                            if let Some(path) = &debug_log_path {
                                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                                    use std::io::Write;
                                    let _ = writeln!(f, "-> {}", line);
                                }
                            }
                        }
                        Some(None) => pending_prompts = pending_prompts.saturating_sub(1),
                        None => {}
                    }
                    continue;
                }
                input = rx.recv() => {
                    match input {
                        Some(line) => line,
                        None => { stdin_open = false; continue; }
                    }
                }
            }
        } else {
            match out_rx.recv().await {
                Some(Some(line)) => {
                    let _ = writeln!(stdout, "{}", line);
                    let _ = stdout.flush();
                    if let Some(path) = &debug_log_path {
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(path)
                        {
                            use std::io::Write;
                            let _ = writeln!(f, "-> {}", line);
                        }
                    }
                }
                Some(None) => pending_prompts = pending_prompts.saturating_sub(1),
                None => break,
            }
            continue;
        };

        while let Ok(output) = out_rx.try_recv() {
            match output {
                Some(line) => {
                    let _ = writeln!(stdout, "{}", line);
                    let _ = stdout.flush();
                    if let Some(path) = &debug_log_path {
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(path)
                        {
                            use std::io::Write;
                            let _ = writeln!(f, "-> {}", line);
                        }
                    }
                }
                None => pending_prompts = pending_prompts.saturating_sub(1),
            }
        }

        if let Some(path) = &debug_log_path {
            if let Ok(mut log_file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                use std::io::Write;
                let _ = writeln!(log_file, "<- {}", line);
            }
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let id = match req.id {
            Some(id) => id,
            None => {
                if req.method.as_deref() == Some("session/cancel") {
                    let params = req.params.unwrap_or(json!({}));
                    if let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) {
                        if let Some(cancelled) = active_cancellations
                            .lock()
                            .unwrap()
                            .get(session_id)
                            .cloned()
                        {
                            cancelled.store(true, Ordering::SeqCst);
                        }
                    }
                }
                continue;
            }
        };

        let output = match req.method.as_deref() {
            Some("initialize") => {
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let _ = out_tx.send(Some(
                        serde_json::to_string(&adapter.handle_initialize(id)).unwrap(),
                    ));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/new") => {
                let params = req.params.unwrap_or(json!({}));
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let _ = out_tx.send(Some(
                        serde_json::to_string(&adapter.handle_session_new(id, &params)).unwrap(),
                    ));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/load") => {
                let params = req.params.unwrap_or(json!({}));
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let response = adapter.handle_session_load(id, &params);

                    // Replay existing message history for the loaded session before returning success.
                    if response.error.is_none() {
                        let session_id_opt = params
                            .get("sessionId")
                            .or_else(|| params.get("session_id"))
                            .and_then(|v| v.as_str());
                        if let Some(session_id) = session_id_opt {
                            if let Some(session) = adapter.sessions.get(session_id) {
                                if let Some(conv_id) = &session.conversation_id {
                                    let notifications = streaming::replay_history(
                                        &adapter.conversations_dir,
                                        conv_id,
                                        session_id,
                                    );
                                    for notif in notifications {
                                        let _ = out_tx.send(Some(notif));
                                    }
                                }
                            }
                        }
                    }

                    let _ = out_tx.send(Some(serde_json::to_string(&response).unwrap()));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/prompt") => {
                let params = req.params.unwrap_or(json!({}));
                let session_id = params
                    .get("sessionId")
                    .or_else(|| params.get("session_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let cancelled = Arc::new(AtomicBool::new(false));
                if !session_id.is_empty() {
                    active_cancellations
                        .lock()
                        .unwrap()
                        .insert(session_id.clone(), Arc::clone(&cancelled));
                }
                let adapter = Arc::clone(&adapter);
                let active_cancellations = Arc::clone(&active_cancellations);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let (sid, args, snapshot, init_conv, init_idx, wd, cd) = {
                        let mut adapter = adapter.lock().await;
                        let (sid, _prompt, args, snapshot, init_conv, init_idx, resolved_cwd) =
                            adapter.prepare_prompt_state(&params);
                        let cd = adapter.conversations_dir.clone();
                        (sid, args, snapshot, init_conv, init_idx, resolved_cwd, cd)
                    };
                    let output = Adapter::execute_prompt(
                        id,
                        &sid,
                        args,
                        snapshot,
                        init_conv,
                        init_idx,
                        wd,
                        cd,
                        cancelled,
                        out_tx.clone(),
                    )
                    .await;
                    if let Some((bound_conv_id, new_step_idx)) = output.session_update {
                        let mut adapter = adapter.lock().await;
                        if let Some(session) = adapter.sessions.get_mut(&sid) {
                            if session.conversation_id.is_none() {
                                session.conversation_id = bound_conv_id.clone();
                            }
                            if bound_conv_id.is_some() {
                                session.last_step_idx = new_step_idx;
                            }
                        }
                        if bound_conv_id.is_some() {
                            let model_id =
                                adapter.sessions.get(&sid).and_then(|s| s.model_id.clone());
                            let cwd = adapter.sessions.get(&sid).and_then(|s| s.cwd.clone());
                            let title = adapter.sessions.get(&sid).and_then(|s| s.title.clone());
                            let updated_at = adapter
                                .sessions
                                .get(&sid)
                                .and_then(|s| s.updated_at.clone());
                            adapter.persist_session(
                                &sid,
                                bound_conv_id.as_deref(),
                                new_step_idx,
                                model_id.as_deref(),
                                cwd.as_deref(),
                                title.as_deref(),
                                updated_at.as_deref(),
                            );
                        }
                    }
                    if !session_id.is_empty() {
                        active_cancellations.lock().unwrap().remove(&session_id);
                    }
                    for line in output.response_lines {
                        let _ = out_tx.send(Some(line));
                    }
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/setConfigOption") | Some("session/set_config_option") => {
                let params = req.params.unwrap_or(json!({}));
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let response = adapter.handle_session_set_config_option(id, &params);
                    if response.error.is_none() {
                        let session_id = params
                            .get("sessionId")
                            .or_else(|| params.get("session_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");
                        if !session_id.is_empty() && !value.is_empty() {
                            let notification = JsonRpcNotification {
                                jsonrpc: "2.0",
                                method: "session/update".to_string(),
                                params: json!({
                                    "sessionId": session_id,
                                    "update": {
                                        "sessionUpdate": "config_option_update",
                                        "configOptions": [
                                            {
                                                "id": "model",
                                                "currentValue": value
                                            }
                                        ]
                                    }
                                }),
                            };
                            let _ =
                                out_tx.send(Some(serde_json::to_string(&notification).unwrap()));
                        }
                    }
                    let _ = out_tx.send(Some(serde_json::to_string(&response).unwrap()));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/setModel") | Some("session/set_model") => {
                let params = req.params.unwrap_or(json!({}));
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let response = adapter.handle_session_set_model(id, &params);
                    if response.error.is_none() {
                        let session_id = params
                            .get("sessionId")
                            .or_else(|| params.get("session_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let value = params
                            .get("modelId")
                            .or_else(|| params.get("model_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if !session_id.is_empty() && !value.is_empty() {
                            let notification = JsonRpcNotification {
                                jsonrpc: "2.0",
                                method: "session/update".to_string(),
                                params: json!({
                                    "sessionId": session_id,
                                    "update": {
                                        "sessionUpdate": "config_option_update",
                                        "configOptions": [
                                            {
                                                "id": "model",
                                                "currentValue": value
                                            }
                                        ]
                                    }
                                }),
                            };
                            let _ =
                                out_tx.send(Some(serde_json::to_string(&notification).unwrap()));
                        }
                    }
                    let _ = out_tx.send(Some(serde_json::to_string(&response).unwrap()));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/list") => {
                let params = req.params.unwrap_or(json!({}));
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let adapter = adapter.lock().await;
                    let _ = out_tx.send(Some(
                        serde_json::to_string(&adapter.handle_session_list(id, &params)).unwrap(),
                    ));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/close") => {
                let params = req.params.unwrap_or(json!({}));
                let session_id = params
                    .get("sessionId")
                    .or_else(|| params.get("session_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !session_id.is_empty() {
                    if let Some(cancelled) = active_cancellations
                        .lock()
                        .unwrap()
                        .get(&session_id)
                        .cloned()
                    {
                        cancelled.store(true, Ordering::SeqCst);
                    }
                }
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let _ = out_tx.send(Some(
                        serde_json::to_string(&adapter.handle_session_close(id, &params)).unwrap(),
                    ));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/delete") => {
                let params = req.params.unwrap_or(json!({}));
                let session_id = params
                    .get("sessionId")
                    .or_else(|| params.get("session_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !session_id.is_empty() {
                    if let Some(cancelled) = active_cancellations
                        .lock()
                        .unwrap()
                        .get(&session_id)
                        .cloned()
                    {
                        cancelled.store(true, Ordering::SeqCst);
                    }
                }
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let _ = out_tx.send(Some(
                        serde_json::to_string(&adapter.handle_session_delete(id, &params)).unwrap(),
                    ));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/resume") => {
                let params = req.params.unwrap_or(json!({}));
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let _ = out_tx.send(Some(
                        serde_json::to_string(&adapter.handle_session_resume(id, &params)).unwrap(),
                    ));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/fork") => {
                let params = req.params.unwrap_or(json!({}));
                let adapter = Arc::clone(&adapter);
                let out_tx = out_tx.clone();
                pending_prompts += 1;
                tokio::spawn(async move {
                    let mut adapter = adapter.lock().await;
                    let _ = out_tx.send(Some(
                        serde_json::to_string(&adapter.handle_session_fork(id, &params)).unwrap(),
                    ));
                    let _ = out_tx.send(None);
                });
                Vec::new()
            }
            Some("session/cancel") => {
                let params = req.params.unwrap_or(json!({}));
                let session_id_opt = params
                    .get("sessionId")
                    .or_else(|| params.get("session_id"))
                    .and_then(|v| v.as_str());
                if let Some(session_id) = session_id_opt {
                    if let Some(cancelled) = active_cancellations
                        .lock()
                        .unwrap()
                        .get(session_id)
                        .cloned()
                    {
                        cancelled.store(true, Ordering::SeqCst);
                    }
                }
                vec![serde_json::to_string(&JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({})),
                    error: None,
                })
                .unwrap()]
            }
            Some(method) => {
                vec![serde_json::to_string(&JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(
                        json!({"code":-32601,"message":format!("method not found: {method}")}),
                    ),
                })
                .unwrap()]
            }
            None => continue,
        };

        for line in output {
            let _ = writeln!(stdout, "{}", line);
        }
        let _ = stdout.flush();
    }
}


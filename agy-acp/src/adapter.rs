use fs2::FileExt;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::types::*;

pub fn paths_are_equal(p1: &str, p2: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        let p1_norm = p1.replace('/', "\\").to_lowercase();
        let p2_norm = p2.replace('/', "\\").to_lowercase();
        p1_norm == p2_norm
    }
    #[cfg(not(target_os = "windows"))]
    {
        p1 == p2
    }
}

pub struct Adapter {
    pub sessions: HashMap<String, Session>,
    pub working_dir: String,
    pub conversations_dir: PathBuf,
    pub state_file: PathBuf,
    pub available_models: Option<Vec<String>>,
}

pub fn get_home_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| "C:\\".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").unwrap_or_else(|_| "/home/agent".to_string())
    }
}

impl Adapter {
    pub fn new(state_dir: Option<String>, conversations_dir: Option<String>) -> Self {
        let home = get_home_dir();
        let state_dir = match state_dir {
            Some(d) => PathBuf::from(d),
            None => PathBuf::from(&home).join(".sdtio-acpb/agy-acp"),
        };
        let conversations_dir = match conversations_dir {
            Some(d) => PathBuf::from(d),
            None => PathBuf::from(&home).join(".gemini/antigravity-cli/conversations"),
        };
        let default_working = if cfg!(target_os = "windows") {
            "C:\\".to_string()
        } else {
            "/tmp".to_string()
        };
        Self {
            sessions: HashMap::new(),
            working_dir: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(default_working),
            conversations_dir,
            state_file: state_dir.join("sessions.json"),
            available_models: None,
        }
    }

    // --- Model cache ---

    pub fn models_cache_path(&self) -> PathBuf {
        self.state_file.with_file_name("models_cache.json")
    }

    pub fn load_cached_models(&self) -> Option<Vec<String>> {
        let path = self.models_cache_path();
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str::<Vec<String>>(&content)
            .ok()
            .filter(|v| !v.is_empty())
    }

    pub fn save_models_cache(&self, models: &[String]) {
        if let Some(parent) = self.models_cache_path().parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(models) {
            let tmp = self.models_cache_path().with_extension("tmp");
            if fs::write(&tmp, &json).is_ok() {
                let _ = fs::rename(&tmp, self.models_cache_path());
            }
        }
    }

    pub fn static_fallback_models() -> Vec<String> {
        vec![
            "Gemini 3.5 Flash (Medium)".to_string(),
            "Gemini 3.5 Flash (High)".to_string(),
            "Gemini 3.5 Flash (Low)".to_string(),
            "Gemini 3.1 Pro (Low)".to_string(),
            "Gemini 3.1 Pro (High)".to_string(),
            "Claude Sonnet 4.6 (Thinking)".to_string(),
            "Claude Opus 4.6 (Thinking)".to_string(),
            "GPT-OSS 120B (Medium)".to_string(),
        ]
    }

    /// Resolve the `agy` binary path.
    pub fn agy_bin() -> &'static str {
        "agy"
    }

    /// Build PATH with common agent binary locations prepended.
    pub fn augmented_path() -> String {
        let home = get_home_dir();
        let base = std::env::var("PATH").unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                String::new()
            } else {
                "/usr/local/bin:/usr/bin:/bin".to_string()
            }
        });

        #[cfg(target_os = "windows")]
        {
            let system_root =
                std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
            let local_app_data = std::env::var("LOCALAPPDATA")
                .unwrap_or_else(|_| format!("{}\\AppData\\Local", home));
            format!(
                "{home}\\bin;{home}\\.local\\bin;{local_app_data}\\agy\\bin;{local_app_data}\\fnm;{system_root}\\system32;{system_root};{base}"
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            format!(
                "{home}/bin:{home}/.local/bin:{home}/.local/share/fnm/aliases/default/bin:{base}"
            )
        }
    }

    pub fn fetch_available_models() -> Vec<String> {
        std::process::Command::new(Self::agy_bin())
            .arg("models")
            .env("PATH", Self::augmented_path())
            .output()
            .ok()
            .map(|o| {
                // First try reading from stdout
                let mut text = String::from_utf8_lossy(&o.stdout).into_owned();

                // If stdout is empty, the CLI might be writing the list to stderr
                if text.trim().is_empty() {
                    text = String::from_utf8_lossy(&o.stderr).into_owned();
                }

                text.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .filter(|v: &Vec<String>| !v.is_empty()) // Ensure we don't pass an empty array up the chain
            .unwrap_or_default()
    }

    pub fn get_available_models(&mut self) -> &[String] {
        if self.available_models.is_none() {
            let models = Self::fetch_available_models();
            if !models.is_empty() {
                eprintln!(
                    "[agy-acp] fetched {} models from `agy models`, updating cache",
                    models.len()
                );
                self.save_models_cache(&models);
                self.available_models = Some(models);
            } else if let Some(cached) = self.load_cached_models() {
                eprintln!(
                    "[agy-acp] `agy models` failed, using cached model list ({} models)",
                    cached.len()
                );
                self.available_models = Some(cached);
            } else {
                eprintln!(
                    "[agy-acp] `agy models` failed and no cache found, using hardcoded fallback"
                );
                self.available_models = Some(Self::static_fallback_models());
            }
        }
        self.available_models.as_ref().unwrap()
    }

    pub fn config_options_json(&mut self, model_id: Option<&str>) -> Value {
        let models = self.get_available_models();
        if models.is_empty() {
            return json!([]);
        }
        let current = model_id
            .or_else(|| models.first().map(|s| s.as_str()))
            .unwrap_or("");
        let options: Vec<Value> = models
            .iter()
            .map(|name| json!({ "value": name, "name": name }))
            .collect();
        json!([{
            "id": "model",
            "name": "Model",
            "category": "model",
            "type": "select",
            "currentValue": current,
            "options": options,
        }])
    }

    // --- State persistence ---

    fn lock_state_file(&self) -> Option<fs::File> {
        if let Some(parent) = self.state_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let lock_path = self.state_file.with_extension("lock");
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .ok()?;
        lock_file.lock_exclusive().ok()?;
        Some(lock_file)
    }

    fn load_store_inner(&self) -> SessionStore {
        let Some(file) = fs::File::open(&self.state_file).ok() else {
            return SessionStore::default();
        };
        serde_json::from_reader(&file).unwrap_or_default()
    }

    pub fn load_store(&self) -> SessionStore {
        let _lock = self.lock_state_file();
        self.load_store_inner()
    }

    #[allow(clippy::type_complexity)]
    pub fn restore_session(
        &self,
        session_id: &str,
    ) -> Option<(
        Option<String>,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        let store = self.load_store();
        store.sessions.get(session_id).map(|s| {
            (
                s.conversation_id.clone(),
                s.last_step_idx,
                s.model_id.clone(),
                s.cwd.clone(),
                s.title.clone(),
                s.updated_at.clone(),
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn persist_session(
        &self,
        session_id: &str,
        conversation_id: Option<&str>,
        last_step_idx: i64,
        model_id: Option<&str>,
        cwd: Option<&str>,
        title: Option<&str>,
        updated_at: Option<&str>,
    ) {
        let Some(_lock) = self.lock_state_file() else {
            return;
        };
        let mut store = self.load_store_inner();
        store.sessions.insert(
            session_id.to_string(),
            StoredSession {
                conversation_id: conversation_id.map(String::from),
                last_step_idx,
                model_id: model_id.map(String::from),
                cwd: cwd.map(String::from),
                title: title.map(String::from),
                updated_at: updated_at.map(String::from),
            },
        );
        let tmp = self.state_file.with_extension("tmp");
        if let Ok(file) = fs::File::create(&tmp) {
            if serde_json::to_writer_pretty(&file, &store).is_ok() {
                let _ = fs::rename(&tmp, &self.state_file);
            }
        }
    }

    // --- Conversation snapshot ---

    pub fn conversation_snapshot(&self) -> HashSet<String> {
        let Ok(entries) = fs::read_dir(&self.conversations_dir) else {
            return HashSet::new();
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().map(|x| x == "db").unwrap_or(false) {
                    path.file_stem().map(|s| s.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn new_conversation_id(&self, before: &HashSet<String>) -> Option<String> {
        let after = self.conversation_snapshot();
        let mut created: Vec<_> = after.difference(before).collect();
        if created.is_empty() {
            return None;
        }
        if created.len() > 1 {
            eprintln!(
                "[agy-acp] WARN: multiple new agy conversation files appeared; refusing to bind"
            );
            return None;
        }
        Some(created.remove(0).clone())
    }

    // --- Session management ---

    pub fn evict_if_needed(&mut self) {
        const MAX_SESSIONS: usize = 64;
        while self.sessions.len() >= MAX_SESSIONS {
            if let Some(key) = self.sessions.keys().next().cloned() {
                self.sessions.remove(&key);
            }
        }
    }

    pub fn restore_session_state(&mut self, session_id: &str) -> bool {
        let Some((conversation_id, last_step_idx, model_id, cwd, title, updated_at)) =
            self.restore_session(session_id)
        else {
            return false;
        };
        if !self.sessions.contains_key(session_id) {
            self.evict_if_needed();
        }
        self.sessions.insert(
            session_id.to_string(),
            Session {
                conversation_id,
                last_step_idx,
                model_id,
                cwd,
                title,
                updated_at,
            },
        );
        true
    }

    // --- JSON-RPC handlers ---

    pub fn handle_initialize(&mut self, id: Value) -> JsonRpcResponse {
        // Pre-warm the models cache so session/new is fast.
        let models = self.get_available_models();
        let models_strings: Vec<Value> = models.iter().map(|n| json!(n)).collect();
        let models_objects: Vec<Value> = models
            .iter()
            .map(|n| {
                json!({
                    "id": n,
                    "modelId": n,
                    "name": n,
                    "description": format!("Model {}", n)
                })
            })
            .collect();

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "protocolVersion": 1,
                "agentInfo": {
                    "name": "agy",
                    "title": "Google Antigravity",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "agentCapabilities": {
                    "loadSession": true,
                    "sessionCapabilities": {
                        "list": {},
                        "close": {},
                        "delete": {},
                        "resume": {},
                        "fork": {}
                    },
                    "models": models_strings.clone(),
                    "availableModels": models_objects.clone(),
                    "selectableModels": models_objects.clone()
                },
                "models": models_strings,
                "availableModels": models_objects.clone(),
                "selectableModels": models_objects
            })),
            error: None,
        }
    }

    pub fn handle_session_new(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = Uuid::new_v4().to_string();
        self.evict_if_needed();

        let cwd = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let title = params
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let updated_at = Some(chrono::Utc::now().to_rfc3339());

        self.sessions.insert(
            session_id.clone(),
            Session {
                conversation_id: None,
                last_step_idx: -1,
                model_id: None,
                cwd: cwd.clone(),
                title: title.clone(),
                updated_at: updated_at.clone(),
            },
        );

        self.persist_session(
            &session_id,
            None,
            -1,
            None,
            cwd.as_deref(),
            title.as_deref(),
            updated_at.as_deref(),
        );

        let config_options = self.config_options_json(None);
        let models = self.get_available_models();

        let models_objects: Vec<Value> = models
            .iter()
            .map(|n| {
                json!({
                    "id": n,
                    "modelId": n,
                    "name": n,
                    "description": format!("Model {}", n)
                })
            })
            .collect();

        let current_model_id = models.first().cloned().unwrap_or_default();

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "sessionId": session_id,
                "configOptions": config_options,
                "models": {
                    "availableModels": models_objects,
                    "currentModelId": current_model_id
                }
            })),
            error: None,
        }
    }

    pub fn handle_session_load(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if session_id.is_empty() {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code":-32602,"message":"missing sessionId"})),
            };
        }
        if self.restore_session_state(session_id) {
            let updated_at = chrono::Utc::now().to_rfc3339();
            if let Some(session) = self.sessions.get_mut(session_id) {
                if let Some(cwd) = params.get("cwd").and_then(|v| v.as_str()) {
                    if !cwd.is_empty() {
                        session.cwd = Some(cwd.to_string());
                    }
                }
                session.updated_at = Some(updated_at.clone());
                let conv_id = session.conversation_id.clone();
                let last_step_idx = session.last_step_idx;
                let model_id = session.model_id.clone();
                let cwd = session.cwd.clone();
                let title = session.title.clone();
                self.persist_session(
                    session_id,
                    conv_id.as_deref(),
                    last_step_idx,
                    model_id.as_deref(),
                    cwd.as_deref(),
                    title.as_deref(),
                    Some(&updated_at),
                );
            }

            // ACP spec: session/load returns null after history replay.
            // configOptions are included for client convenience (many clients
            // expect them here to populate model selectors).
            let model_id = self
                .sessions
                .get(session_id)
                .and_then(|s| s.model_id.clone());
            let config_options = self.config_options_json(model_id.as_deref());
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({
                    "sessionId": session_id,
                    "configOptions": config_options,
                })),
                error: None,
            };
        }
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(
                json!({"code":-32000,"message":format!("unknown sessionId: {session_id}")}),
            ),
        }
    }

    pub fn handle_session_set_config_option(
        &mut self,
        id: Value,
        params: &Value,
    ) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let config_id = params
            .get("configId")
            .or_else(|| params.get("config_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let value = params.get("value").and_then(|v| v.as_str()).unwrap_or("");

        if session_id.is_empty() || config_id != "model" || value.is_empty() {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(
                    json!({"code":-32602,"message":"missing sessionId, configId, or value"}),
                ),
            };
        }
        if !self.sessions.contains_key(session_id) {
            let _ = self.restore_session_state(session_id);
        }
        let Some(session) = self.sessions.get_mut(session_id) else {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(
                    json!({"code":-32000,"message":format!("unknown sessionId: {session_id}")}),
                ),
            };
        };
        session.model_id = Some(value.to_string());

        let updated_at = chrono::Utc::now().to_rfc3339();
        session.updated_at = Some(updated_at.clone());

        let conv_id = session.conversation_id.clone();
        let last_step_idx = session.last_step_idx;
        let cwd = session.cwd.clone();
        let title = session.title.clone();
        self.persist_session(
            session_id,
            conv_id.as_deref(),
            last_step_idx,
            Some(value),
            cwd.as_deref(),
            title.as_deref(),
            Some(&updated_at),
        );
        let config_options = self.config_options_json(Some(value));
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ "configOptions": config_options })),
            error: None,
        }
    }

    pub fn handle_session_list(&self, id: Value, params: &Value) -> JsonRpcResponse {
        let filter_cwd = params
            .get("workingDirectory")
            .or_else(|| params.get("cwd"))
            .and_then(|v| v.as_str());

        let filter_dirs: HashSet<&str> = params
            .get("additionalDirectories")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|val| val.as_str()).collect())
            .unwrap_or_default();

        let store = self.load_store();
        let mut list = Vec::new();
        for (sid, s) in &store.sessions {
            let cwd = s.cwd.clone().unwrap_or_else(|| self.working_dir.clone());

            if let Some(target_cwd) = filter_cwd {
                let matches_cwd = paths_are_equal(&cwd, target_cwd);
                let matches_additional = filter_dirs.iter().any(|d| paths_are_equal(&cwd, d));
                if !matches_cwd && !matches_additional {
                    continue;
                }
            }

            let title = s.title.clone().unwrap_or_else(|| {
                if let Some(ref model) = s.model_id {
                    format!("Session on {}", model)
                } else {
                    format!("Session {}", &sid[..sid.len().min(8)])
                }
            });
            let updated_at = s
                .updated_at
                .clone()
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

            list.push(json!({
                "sessionId": sid,
                "cwd": cwd,
                "title": title,
                "updatedAt": updated_at,
            }));
        }

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "sessions": list
            })),
            error: None,
        }
    }

    pub fn handle_session_close(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if session_id.is_empty() {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code":-32602,"message":"missing sessionId"})),
            };
        }

        self.sessions.remove(session_id);

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        }
    }

    pub fn handle_session_delete(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if session_id.is_empty() {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code":-32602,"message":"missing sessionId"})),
            };
        }

        self.sessions.remove(session_id);

        let Some(_lock) = self.lock_state_file() else {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code":-32000,"message":"failed to lock state file"})),
            };
        };

        let mut store = self.load_store_inner();
        store.sessions.remove(session_id);

        let tmp = self.state_file.with_extension("tmp");
        if let Ok(file) = fs::File::create(&tmp) {
            if serde_json::to_writer_pretty(&file, &store).is_ok() {
                let _ = fs::rename(&tmp, &self.state_file);
            }
        }

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        }
    }

    pub fn handle_session_resume(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        self.handle_session_load(id, params)
    }

    pub fn handle_session_fork(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let parent_session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if parent_session_id.is_empty() {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code":-32602,"message":"missing sessionId"})),
            };
        }

        if !self.sessions.contains_key(parent_session_id) {
            let _ = self.restore_session_state(parent_session_id);
        }

        let (parent_conv_id, parent_last_step_idx, parent_model_id, parent_cwd, parent_title) = {
            let Some(parent_session) = self.sessions.get(parent_session_id) else {
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(
                        json!({"code":-32000,"message":format!("unknown parent sessionId: {parent_session_id}")}),
                    ),
                };
            };
            (
                parent_session.conversation_id.clone(),
                parent_session.last_step_idx,
                parent_session.model_id.clone(),
                parent_session.cwd.clone(),
                parent_session.title.clone(),
            )
        };

        let new_session_id = Uuid::new_v4().to_string();

        let mut new_conv_id = None;
        if let Some(ref p_conv_id) = parent_conv_id {
            let next_conv_id = format!("agy-fork-{}", Uuid::new_v4());
            let parent_db_path = self.conversations_dir.join(format!("{}.db", p_conv_id));
            let new_db_path = self.conversations_dir.join(format!("{}.db", next_conv_id));

            if parent_db_path.exists() {
                if let Err(e) = fs::copy(&parent_db_path, &new_db_path) {
                    eprintln!("[agy-acp] WARN: failed to copy DB file for fork: {}", e);
                } else {
                    new_conv_id = Some(next_conv_id);
                }
            }
        }

        let new_cwd = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| parent_cwd.clone());

        let new_title = Some(format!(
            "Fork of {}",
            parent_title.as_deref().unwrap_or(parent_session_id)
        ));

        let updated_at = Some(chrono::Utc::now().to_rfc3339());

        self.sessions.insert(
            new_session_id.clone(),
            Session {
                conversation_id: new_conv_id.clone(),
                last_step_idx: parent_last_step_idx,
                model_id: parent_model_id.clone(),
                cwd: new_cwd.clone(),
                title: new_title.clone(),
                updated_at: updated_at.clone(),
            },
        );

        self.persist_session(
            &new_session_id,
            new_conv_id.as_deref(),
            parent_last_step_idx,
            parent_model_id.as_deref(),
            new_cwd.as_deref(),
            new_title.as_deref(),
            updated_at.as_deref(),
        );

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ "sessionId": new_session_id })),
            error: None,
        }
    }

    pub fn handle_session_set_model(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let model_id = params
            .get("modelId")
            .or_else(|| params.get("model_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if session_id.is_empty() || model_id.is_empty() {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code": -32602, "message": "missing sessionId or modelId"})),
            };
        }

        if !self.sessions.contains_key(session_id) {
            let _ = self.restore_session_state(session_id);
        }

        let Some(session) = self.sessions.get_mut(session_id) else {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(
                    json!({"code": -32000, "message": format!("unknown sessionId: {session_id}")}),
                ),
            };
        };

        session.model_id = Some(model_id.to_string());
        let updated_at = chrono::Utc::now().to_rfc3339();
        session.updated_at = Some(updated_at.clone());

        let conv_id = session.conversation_id.clone();
        let last_step_idx = session.last_step_idx;
        let cwd = session.cwd.clone();
        let title = session.title.clone();

        self.persist_session(
            session_id,
            conv_id.as_deref(),
            last_step_idx,
            Some(model_id),
            cwd.as_deref(),
            title.as_deref(),
            Some(&updated_at),
        );

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ "modelId": model_id })),
            error: None,
        }
    }

    /// Gather session state needed for prompt execution (under lock).
    #[allow(clippy::type_complexity)]
    pub fn prepare_prompt_state(
        &mut self,
        params: &Value,
    ) -> (
        String,
        String,
        Vec<String>,
        Option<HashSet<String>>,
        Option<String>,
        i64,
        String,
    ) {
        let session_id = params
            .get("sessionId")
            .or_else(|| params.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if !session_id.is_empty() && !self.sessions.contains_key(&session_id) {
            let _ = self.restore_session_state(&session_id);
        }

        let updated_at = chrono::Utc::now().to_rfc3339();
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.updated_at = Some(updated_at.clone());
            let conv_id = session.conversation_id.clone();
            let last_step_idx = session.last_step_idx;
            let model_id = session.model_id.clone();
            let cwd = session.cwd.clone();
            let title = session.title.clone();
            self.persist_session(
                &session_id,
                conv_id.as_deref(),
                last_step_idx,
                model_id.as_deref(),
                cwd.as_deref(),
                title.as_deref(),
                Some(&updated_at),
            );
        }

        let prompt_value = params.get("content").or_else(|| params.get("prompt"));
        let prompt_text = match prompt_value {
            Some(Value::String(s)) => s.trim().to_string(),
            Some(Value::Object(obj)) => {
                if let Some(t) = obj.get("text").and_then(|t| t.as_str()) {
                    t.trim().to_string()
                } else {
                    String::new()
                }
            }
            Some(Value::Array(arr)) => arr
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

        let clean_prompt = prompt_text;

        let snapshot = if self
            .sessions
            .get(&session_id)
            .map(|s| s.conversation_id.is_none())
            .unwrap_or(false)
        {
            Some(self.conversation_snapshot())
        } else {
            None
        };

        let current_cwd = self
            .sessions
            .get(&session_id)
            .and_then(|s| s.cwd.clone())
            .unwrap_or_else(|| self.working_dir.clone());

        let mut args: Vec<String> = Vec::new();
        args.push("--add-dir".to_string());
        args.push(current_cwd.clone());
        if let Ok(extra) = std::env::var("AGY_EXTRA_ARGS") {
            if let Ok(parsed) = shell_words::split(&extra) {
                args.extend(parsed);
            } else {
                eprintln!("[agy-acp] WARN: failed to parse AGY_EXTRA_ARGS, ignoring");
            }
        }
        if let Some(session) = self.sessions.get(&session_id) {
            if let Some(conv_id) = &session.conversation_id {
                args.push("--conversation".to_string());
                args.push(conv_id.clone());
            }
            if let Some(model_id) = &session.model_id {
                args.push("--model".to_string());
                args.push(model_id.clone());
            }
        }
        args.push("-p".to_string());
        args.push(clean_prompt.clone());

        let initial_conv_id = self
            .sessions
            .get(&session_id)
            .and_then(|s| s.conversation_id.clone());
        let initial_step_idx = self
            .sessions
            .get(&session_id)
            .map(|s| s.last_step_idx)
            .unwrap_or(-1);

        (
            session_id,
            clean_prompt,
            args,
            snapshot,
            initial_conv_id,
            initial_step_idx,
            current_cwd,
        )
    }
}

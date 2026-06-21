use fs2::FileExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::types::*;

pub struct Adapter {
    pub sessions: HashMap<String, Session>,
    pub state_file: PathBuf,
    pub available_models: Option<Vec<String>>,
    pub api_base: String,
    pub api_key: String,
    pub client: Client,
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
    pub fn new(api_base: String, api_key: String, state_dir: Option<String>) -> Self {
        let state_dir = match state_dir {
            Some(d) => PathBuf::from(d),
            None => {
                let home = get_home_dir();
                PathBuf::from(&home).join(".stdio-acpb/openai-acp")
            }
        };
        let api_base = api_base.replace('"', "").replace('\'', "");
        let api_base = api_base.trim_end_matches('/').to_string();
        let api_key = api_key.replace('"', "").replace('\'', "").to_string();

        Self {
            sessions: HashMap::new(),
            state_file: state_dir.join("sessions.json"),
            available_models: None,
            api_base,
            api_key,
            client: Client::new(),
        }
    }

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
        vec!["gpt-3.5-turbo".to_string()]
    }

    pub async fn fetch_available_models(&self) -> Vec<String> {
        let url = format!("{}/models", self.api_base);
        match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
        {
            Ok(resp) => {
                match resp.json::<Value>().await {
                    Ok(json) => {
                        if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                            let mut models = Vec::new();
                            for item in data {
                                if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                                    models.push(id.to_string());
                                }
                            }
                            if !models.is_empty() {
                                return models;
                            } else {
                                tracing::error!("No models found in data array: {:?}", data);
                            }
                        } else {
                            tracing::error!("Invalid JSON response: missing or invalid 'data' field: {}", json);
                        }
                    }
                    Err(e) => tracing::error!("Failed to parse models JSON: {}", e),
                }
            }
            Err(e) => tracing::error!("Failed to fetch models from {}: {}", url, e),
        }
        Vec::new()
    }

    pub async fn get_available_models(&mut self) -> &[String] {
        if self.available_models.is_none() {
            let models = self.fetch_available_models().await;
            if !models.is_empty() {
                self.save_models_cache(&models);
                self.available_models = Some(models);
            } else if let Some(cached) = self.load_cached_models() {
                self.available_models = Some(cached);
            } else {
                self.available_models = Some(Self::static_fallback_models());
            }
        }
        self.available_models.as_ref().unwrap()
    }

    pub async fn config_options_json(&mut self, model_id: Option<&str>) -> Value {
        let models = self.get_available_models().await;
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

    pub fn restore_session(&self, session_id: &str) -> Option<StoredSession> {
        let store = self.load_store();
        store.sessions.get(session_id).cloned()
    }

    pub fn persist_session(
        &self,
        session_id: &str,
        messages: Vec<Message>,
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
                messages,
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

    pub fn delete_session(&self, session_id: &str) -> bool {
        let Some(_lock) = self.lock_state_file() else {
            return false;
        };
        let mut store = self.load_store_inner();
        let removed = store.sessions.remove(session_id).is_some();
        if removed {
            let tmp = self.state_file.with_extension("tmp");
            if let Ok(file) = fs::File::create(&tmp) {
                if serde_json::to_writer_pretty(&file, &store).is_ok() {
                    let _ = fs::rename(&tmp, &self.state_file);
                }
            }
        }
        removed
    }

    pub fn evict_if_needed(&mut self) {
        const MAX_SESSIONS: usize = 64;
        while self.sessions.len() >= MAX_SESSIONS {
            if let Some(key) = self.sessions.keys().next().cloned() {
                self.sessions.remove(&key);
            }
        }
    }

    pub fn restore_session_state(&mut self, session_id: &str) -> bool {
        let Some(stored) = self.restore_session(session_id)
        else {
            return false;
        };
        if !self.sessions.contains_key(session_id) {
            self.evict_if_needed();
        }
        self.sessions.insert(
            session_id.to_string(),
            Session {
                messages: stored.messages,
                model_id: stored.model_id,
                cwd: stored.cwd,
                title: stored.title,
                updated_at: stored.updated_at,
            },
        );
        true
    }

    pub async fn handle_initialize(&mut self, id: Value) -> JsonRpcResponse {
        let models = self.get_available_models().await;
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
                    "name": "openai-proxy",
                    "title": "OpenAI API Proxy",
                    "version": "0.1.0"
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

    pub async fn handle_session_new(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
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
                messages: Vec::new(),
                model_id: None,
                cwd: cwd.clone(),
                title: title.clone(),
                updated_at: updated_at.clone(),
            },
        );

        self.persist_session(
            &session_id,
            Vec::new(),
            None,
            cwd.as_deref(),
            title.as_deref(),
            updated_at.as_deref(),
        );

        let models = self.get_available_models().await.to_vec();
        let models_objects: Vec<Value> = models.iter().map(|n| {
            json!({
                "id": n,
                "modelId": n,
                "name": n,
                "description": format!("Model {}", n)
            })
        }).collect();
        let current_model_id = models.first().cloned().unwrap_or_default();
        let config_options = self.config_options_json(None).await;

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

    pub async fn handle_session_load(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = match params.get("sessionId").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({"code": -32602, "message": "Missing sessionId parameter"})),
                };
            }
        };

        if !self.restore_session_state(session_id) {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code": -32603, "message": "Session not found"})),
            };
        }

        let models = self.get_available_models().await.to_vec();
        let models_objects: Vec<Value> = models.iter().map(|n| {
            json!({
                "id": n,
                "modelId": n,
                "name": n,
                "description": format!("Model {}", n)
            })
        }).collect();
        
        let model_id = self.sessions.get(session_id).unwrap().model_id.clone();
        let current_model_id = model_id.clone().unwrap_or_else(|| models.first().cloned().unwrap_or_default());
        let config_options = self.config_options_json(model_id.as_deref()).await;

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ 
                "configOptions": config_options,
                "models": {
                    "availableModels": models_objects,
                    "currentModelId": current_model_id
                }
            })),
            error: None,
        }
    }

    pub fn handle_session_list(&self, id: Value, params: &Value) -> JsonRpcResponse {
        let filter_cwd = params
            .get("workingDirectory")
            .or_else(|| params.get("cwd"))
            .and_then(|v| v.as_str());

        let store = self.load_store();
        let mut session_list = Vec::new();

        for (sid, session) in &store.sessions {
            if let Some(f_cwd) = filter_cwd {
                if let Some(s_cwd) = &session.cwd {
                    let f_cwd_norm = f_cwd.replace('/', "\\").to_lowercase();
                    let s_cwd_norm = s_cwd.replace('/', "\\").to_lowercase();
                    if f_cwd_norm != s_cwd_norm {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            session_list.push(json!({
                "id": sid,
                "title": session.title.clone().unwrap_or_else(|| "Unknown".to_string()),
                "updatedAt": session.updated_at.clone().unwrap_or_else(|| "".to_string()),
                "cwd": session.cwd.clone().unwrap_or_default(),
            }));
        }

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ "sessions": session_list })),
            error: None,
        }
    }

    pub fn handle_session_close(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
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
            .and_then(|v| v.as_str())
            .unwrap_or("");
        self.sessions.remove(session_id);
        let deleted = self.delete_session(session_id);
        if deleted {
            JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({})),
                error: None,
            }
        } else {
            JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(
                    json!({"code": -32603, "message": "Session not found or failed to delete"}),
                ),
            }
        }
    }

    pub fn handle_session_resume(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !self.restore_session_state(session_id) {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code": -32603, "message": "Session not found"})),
            };
        }
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ "sessionId": session_id })),
            error: None,
        }
    }

    pub fn handle_session_fork(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = params
            .get("sessionId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new_session_id = Uuid::new_v4().to_string();

        let Some(stored) = self.restore_session(session_id) else {
            return JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(json!({"code": -32603, "message": "Session not found"})),
            };
        };

        let updated_at = chrono::Utc::now().to_rfc3339();
        self.persist_session(
            &new_session_id,
            stored.messages,
            stored.model_id.as_deref(),
            stored.cwd.as_deref(),
            stored.title.as_deref(),
            Some(&updated_at),
        );

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({ "sessionId": new_session_id })),
            error: None,
        }
    }

    pub fn handle_session_set_model(&mut self, id: Value, params: &Value) -> JsonRpcResponse {
        let session_id = match params.get("sessionId").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({"code": -32602, "message": "Missing sessionId"})),
                };
            }
        };

        let model_id = match params.get("modelId").and_then(|v| v.as_str()) {
            Some(m) => m.to_string(),
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({"code": -32602, "message": "Missing modelId"})),
                };
            }
        };

        if let Some(session) = self.sessions.get_mut(session_id) {
            session.model_id = Some(model_id.clone());
        } else {
            let Some(stored) = self.restore_session(session_id)
            else {
                return JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({"code": -32603, "message": "Session not found"})),
                };
            };
            self.sessions.insert(
                session_id.to_string(),
                Session {
                    messages: stored.messages,
                    model_id: Some(model_id.clone()),
                    cwd: stored.cwd,
                    title: stored.title,
                    updated_at: stored.updated_at,
                },
            );
        }

        let session = self.sessions.get(session_id).unwrap();
        self.persist_session(
            session_id,
            session.messages.clone(),
            session.model_id.as_deref(),
            session.cwd.as_deref(),
            session.title.as_deref(),
            session.updated_at.as_deref(),
        );

        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        }
    }

    pub async fn handle_session_set_config_option(
        &mut self,
        id: Value,
        params: &Value,
    ) -> JsonRpcResponse {
        let config_id = params
            .get("config_id")
            .or_else(|| params.get("configId"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if config_id == "model" {
            let mut set_model_params = params.clone();
            if let Some(val) = params.get("value") {
                set_model_params["modelId"] = val.clone();
            }
            return self.handle_session_set_model(id, &set_model_params);
        }
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({})),
            error: None,
        }
    }

    pub async fn execute_prompt(
        &mut self,
        session_id: &str,
        prompt: &str,
        out_tx: tokio::sync::mpsc::UnboundedSender<Option<String>>,
    ) -> Result<(), anyhow::Error> {
        let stored_session = match self.restore_session(session_id) {
            Some(s) => s,
            None => {
                StoredSession {
                    messages: Vec::new(),
                    model_id: None,
                    cwd: None,
                    title: None,
                    updated_at: None,
                }
            }
        };

        let mut current_messages = stored_session.messages.clone();
        current_messages.push(Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        let model = match stored_session.model_id {
            Some(id) => id,
            None => {
                let models = self.get_available_models().await;
                models.first().cloned().unwrap_or_else(|| "gpt-3.5-turbo".to_string())
            }
        };

        let mut oai_messages = Vec::new();
        for m in &current_messages {
            oai_messages.push(json!({
                "role": m.role,
                "content": m.content
            }));
        }

        let body = json!({
            "model": model,
            "messages": oai_messages,
            "stream": true
        });

        let url = format!("{}/chat/completions", self.api_base);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let text = response.text().await?;
            anyhow::bail!("OpenAI API error: {}", text);
        }

        let mut full_response = String::new();

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            if let Ok(bytes) = chunk {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            break;
                        }
                        if let Ok(json) = serde_json::from_str::<Value>(data) {
                            if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                                if let Some(delta) = choices.first().and_then(|c| c.get("delta")) {
                                    if let Some(content) =
                                        delta.get("content").and_then(|c| c.as_str())
                                    {
                                        full_response.push_str(content);
                                        // Emit streaming text
                                        let rpc = JsonRpcNotification {
                                            jsonrpc: "2.0",
                                            method: "session/update".to_string(),
                                            params: json!({
                                                "sessionId": session_id,
                                                "update": {
                                                    "sessionUpdate": "agent_message_chunk",
                                                    "content": {
                                                        "type": "text",
                                                        "text": content
                                                    }
                                                }
                                            }),
                                        };
                                        let _ = out_tx.send(Some(serde_json::to_string(&rpc).unwrap()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        current_messages.push(Message {
            role: "assistant".to_string(),
            content: full_response,
        });

        self.persist_session(
            session_id,
            current_messages,
            Some(&model),
            stored_session.cwd.as_deref(),
            stored_session.title.as_deref(),
            Some(&chrono::Utc::now().to_rfc3339()),
        );

        Ok(())
    }
}


use agy_acp::adapter::*;
use agy_acp::db;
use agy_acp::protobuf;
use agy_acp::streaming;
use agy_acp::types::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::collections::HashSet;
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn test_extract_text_from_step_payload_field20_field1() {
        let mut inner = Vec::new();
        inner.push(0x0A);
        inner.push(0x05);
        inner.extend_from_slice(b"hello");
        let mut blob = Vec::new();
        blob.push(0x08);
        blob.push(0x0F);
        blob.push(0xA2);
        blob.push(0x01);
        blob.push(inner.len() as u8);
        blob.extend_from_slice(&inner);
        assert_eq!(
            protobuf::extract_text_from_step_payload(&blob),
            Some("hello".to_string())
        );
    }

    #[test]
    fn test_extract_text_returns_none_without_field20() {
        let blob = vec![0x08, 0x03];
        assert_eq!(protobuf::extract_text_from_step_payload(&blob), None);
    }

    #[test]
    fn test_read_varint() {
        assert_eq!(protobuf::read_varint(&[0x05]), Some((5, 1)));
        assert_eq!(protobuf::read_varint(&[0xAC, 0x02]), Some((300, 2)));
        assert_eq!(protobuf::read_varint(&[]), None);
    }

    #[test]
    fn test_initialize_advertises_load_session_support() {
        let mut adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: "/tmp".to_string(),
            conversations_dir: PathBuf::from("/tmp"),
            state_file: PathBuf::from("/tmp/sessions.json"),
            available_models: Some(vec![]),
        };
        let response = adapter.handle_initialize(json!(1));
        assert_eq!(
            response
                .result
                .as_ref()
                .and_then(|r| r.get("agentCapabilities"))
                .and_then(|c| c.get("loadSession"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_initialize_advertises_all_session_capabilities() {
        let mut adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: "/tmp".to_string(),
            conversations_dir: PathBuf::from("/tmp"),
            state_file: PathBuf::from("/tmp/sessions.json"),
            available_models: Some(vec![]),
        };
        let response = adapter.handle_initialize(json!(1));
        let result = response.result.as_ref().unwrap();
        let capabilities = &result["agentCapabilities"];
        assert_eq!(capabilities["loadSession"], json!(true));
        // Per ACP spec, `streaming` is NOT an agentCapability field.
        assert!(capabilities.get("streaming").is_none() || capabilities["streaming"].is_null());
        assert!(capabilities["sessionCapabilities"]["list"].is_object());
        assert!(capabilities["sessionCapabilities"]["close"].is_object());
        assert!(capabilities["sessionCapabilities"]["delete"].is_object());

        let agent_info = &result["agentInfo"];
        assert_eq!(agent_info["name"], json!("agy"));
        assert_eq!(agent_info["title"], json!("Google Antigravity"));
    }

    #[test]
    fn test_session_list_close_delete() {
        let root = std::env::temp_dir().join(format!("agy-acp-test-list-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&root);
        let mut adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: root.to_string_lossy().to_string(),
            conversations_dir: root.join("conversations"),
            state_file: root.join("sessions.json"),
            available_models: Some(vec![]),
        };

        // Create new session
        let new_params = json!({
            "cwd": "/some/cwd",
            "title": "My Custom Title"
        });
        let res_new = adapter.handle_session_new(json!(1), &new_params);
        let result_new = res_new.result.as_ref().unwrap();
        let session_id = result_new["sessionId"].as_str().unwrap().to_string();

        // List sessions
        let list_params = json!({});
        let res_list = adapter.handle_session_list(json!(2), &list_params);
        let result_list = res_list.result.as_ref().unwrap();
        let sessions = result_list["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["sessionId"], json!(session_id));
        assert_eq!(sessions[0]["cwd"], json!("/some/cwd"));
        assert_eq!(sessions[0]["title"], json!("My Custom Title"));

        // Close session
        let close_params = json!({ "sessionId": session_id });
        let res_close = adapter.handle_session_close(json!(3), &close_params);
        assert!(res_close.error.is_none());
        assert!(!adapter.sessions.contains_key(&session_id));

        // Delete session
        let delete_params = json!({ "sessionId": session_id });
        let res_delete = adapter.handle_session_delete(json!(4), &delete_params);
        assert!(res_delete.error.is_none());

        let store = adapter.load_store();
        assert!(!store.sessions.contains_key(&session_id));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_parameter_casing_and_config_option_update() {
        let root = std::env::temp_dir().join(format!("agy-acp-test-casing-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&root);
        let mut adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: root.to_string_lossy().to_string(),
            conversations_dir: root.join("conversations"),
            state_file: root.join("sessions.json"),
            available_models: Some(vec!["Gemini 3.5 Flash (Medium)".to_string()]),
        };

        // 1. Create session (snake_case cwd)
        let new_params = json!({
            "cwd": "/casing/test/dir",
            "title": "Casing Session"
        });
        let res_new = adapter.handle_session_new(json!(1), &new_params);
        let result_new = res_new.result.unwrap();
        let session_id = result_new["sessionId"].as_str().unwrap().to_string();

        assert!(result_new.get("configOptions").is_some());
        let config_options = result_new["configOptions"].as_array().unwrap();
        assert_eq!(config_options.len(), 1);
        assert_eq!(config_options[0]["id"], "model");
        assert_eq!(config_options[0]["category"], "model");
        assert_eq!(
            config_options[0]["currentValue"],
            "Gemini 3.5 Flash (Medium)"
        );

        // Check legacy `models` field for clients without unstable_session_config_options
        assert!(
            result_new.get("models").is_some(),
            "session/new MUST include legacy models object"
        );
        let legacy_models = &result_new["models"];
        assert!(legacy_models["availableModels"].is_array());
        assert_eq!(legacy_models["currentModelId"], "Gemini 3.5 Flash (Medium)");

        // 2. Set config option using snake_case parameter names: session_id and config_id
        let set_params_snake = json!({
            "session_id": session_id,
            "config_id": "model",
            "value": "Gemini 3.5 Flash (Medium)"
        });
        let res_set_snake = adapter.handle_session_set_config_option(json!(2), &set_params_snake);
        assert!(res_set_snake.error.is_none());

        // 2b. Set model using legacy `session/setModel` method
        let set_model_params = json!({
            "sessionId": session_id,
            "modelId": "Gemini 3.5 Flash (Medium)"
        });
        let res_set_model = adapter.handle_session_set_model(json!(99), &set_model_params);
        assert!(res_set_model.error.is_none());
        assert_eq!(
            res_set_model.result.unwrap()["modelId"],
            "Gemini 3.5 Flash (Medium)"
        );
        assert_eq!(
            adapter
                .sessions
                .get(&session_id)
                .unwrap()
                .model_id
                .as_deref(),
            Some("Gemini 3.5 Flash (Medium)")
        );

        // 3. Load session using snake_case session_id
        let load_params_snake = json!({
            "session_id": session_id
        });
        let res_load_snake = adapter.handle_session_load(json!(3), &load_params_snake);
        assert!(res_load_snake.error.is_none());

        // 4. List sessions filtering with workingDirectory (camelCase)
        let list_params_camel = json!({
            "workingDirectory": "/casing/test/dir"
        });
        let res_list_camel = adapter.handle_session_list(json!(4), &list_params_camel);
        let sessions_camel = res_list_camel.result.unwrap()["sessions"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(sessions_camel.len(), 1);
        assert_eq!(sessions_camel[0]["sessionId"], json!(session_id));

        // 5. List sessions filtering with cwd (snake_case)
        let list_params_snake = json!({
            "cwd": "/casing/test/dir"
        });
        let res_list_snake = adapter.handle_session_list(json!(5), &list_params_snake);
        let sessions_snake = res_list_snake.result.unwrap()["sessions"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(sessions_snake.len(), 1);
        assert_eq!(sessions_snake[0]["sessionId"], json!(session_id));

        // 6. List sessions filtering with Windows path variations
        #[cfg(target_os = "windows")]
        {
            let list_params_win = json!({
                "cwd": "\\casing\\TEST\\dir"
            });
            let res_list_win = adapter.handle_session_list(json!(6), &list_params_win);
            let sessions_win = res_list_win.result.unwrap()["sessions"]
                .as_array()
                .unwrap()
                .clone();
            assert_eq!(sessions_win.len(), 1);
            assert_eq!(sessions_win[0]["sessionId"], json!(session_id));
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_session_resume_and_fork() {
        let root = std::env::temp_dir().join(format!("agy-acp-test-fork-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&root);
        let conv_dir = root.join("conversations");
        let _ = fs::create_dir_all(&conv_dir);

        let mut adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: root.to_string_lossy().to_string(),
            conversations_dir: conv_dir.clone(),
            state_file: root.join("sessions.json"),
            available_models: Some(vec!["Gemini 3.5 Flash (Medium)".to_string()]),
        };

        // 1. Create a session
        let res_new = adapter.handle_session_new(json!(1), &json!({}));
        let session_id = res_new.result.unwrap()["sessionId"]
            .as_str()
            .unwrap()
            .to_string();

        // 2. Set config option
        let res_set = adapter.handle_session_set_config_option(
            json!(2),
            &json!({
                "sessionId": session_id,
                "configId": "model",
                "value": "Gemini 3.5 Flash (Medium)"
            }),
        );
        assert!(res_set.error.is_none());

        // 3. Create a mock conversation database file
        let conv_id = "test-conv-123";
        let db_path = conv_dir.join(format!("{}.db", conv_id));
        fs::write(&db_path, b"mock sqlite database content").unwrap();

        // Bind conversation ID to the session
        if let Some(session) = adapter.sessions.get_mut(&session_id) {
            session.conversation_id = Some(conv_id.to_string());
        }
        adapter.persist_session(
            &session_id,
            Some(conv_id),
            -1,
            Some("Gemini 3.5 Flash (Medium)"),
            None,
            None,
            None,
        );

        // 4. Resume the session
        let res_resume =
            adapter.handle_session_resume(json!(3), &json!({ "sessionId": session_id }));
        let result_resume = res_resume.result.unwrap();
        assert_eq!(result_resume["sessionId"], json!(session_id));
        assert_eq!(
            result_resume["configOptions"][0]["currentValue"],
            json!("Gemini 3.5 Flash (Medium)")
        );

        // 5. Fork the session
        let res_fork = adapter.handle_session_fork(json!(4), &json!({ "sessionId": session_id }));
        assert!(res_fork.error.is_none());
        let forked_session_id = res_fork.result.unwrap()["sessionId"]
            .as_str()
            .unwrap()
            .to_string();

        // Verify the forked session details
        let store = adapter.load_store();
        let forked_session = store.sessions.get(&forked_session_id).unwrap();
        assert_eq!(
            forked_session.model_id.as_deref(),
            Some("Gemini 3.5 Flash (Medium)")
        );
        assert!(forked_session.conversation_id.is_some());

        let forked_conv_id = forked_session.conversation_id.as_ref().unwrap();
        assert_ne!(forked_conv_id, conv_id);

        // Verify the database file was successfully copied
        let forked_db_path = conv_dir.join(format!("{}.db", forked_conv_id));
        assert!(forked_db_path.exists());
        assert_eq!(
            fs::read(&forked_db_path).unwrap(),
            b"mock sqlite database content"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_is_narration_true() {
        assert!(db::is_narration("I will fetch the latest commits."));
        assert!(db::is_narration(
            "I will fetch the latest commits.\nI will check the diff."
        ));
    }

    #[test]
    fn test_is_narration_false() {
        assert!(!db::is_narration("Here is the result."));
        assert!(!db::is_narration(
            "I will fetch the commits.\nHere is the result."
        ));
        assert!(!db::is_narration(""));
    }

    #[test]
    fn test_filter_narration_drops_leading_narration() {
        std::env::remove_var("STDIO_ACPB_SHOW_NARRATION");
        let parts = vec![
            "I will fetch the latest commits.\nI will check the diff.".to_string(),
            "I will read the file.".to_string(),
            "The fix is confirmed! LGTM ✅".to_string(),
        ];
        assert_eq!(
            db::filter_narration(&parts),
            "The fix is confirmed! LGTM ✅"
        );
    }

    #[test]
    fn test_filter_narration_single_part_unchanged() {
        let parts = vec!["I will do something.".to_string()];
        assert_eq!(db::filter_narration(&parts), "I will do something.");
    }

    #[test]
    fn test_json_rpc_id_as_string() {
        let req: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":"abc-123","method":"initialize"}"#)
                .unwrap();
        assert_eq!(req.id, Some(json!("abc-123")));
    }

    #[test]
    fn test_json_rpc_id_as_number() {
        let req: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":42,"method":"initialize"}"#).unwrap();
        assert_eq!(req.id, Some(json!(42)));
    }

    #[test]
    #[ignore]
    fn test_session_load_restores_persisted_session() {
        let root = std::env::temp_dir().join(format!("agy-acp-load-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&root);
        let mut adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: root.to_string_lossy().to_string(),
            conversations_dir: root.join("conversations"),
            state_file: root.join("sessions.json"),
            available_models: Some(vec![]),
        };
        adapter.persist_session("sess-1", Some("conv-abc"), 5, None, None, None, None);
        let response = adapter.handle_session_load(json!(7), &json!({"sessionId": "sess-1"}));
        assert!(response.error.is_none());
        assert_eq!(
            adapter
                .sessions
                .get("sess-1")
                .and_then(|s| s.conversation_id.as_deref()),
            Some("conv-abc")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore]
    fn test_session_load_returns_config_options_for_models() {
        let root = std::env::temp_dir().join(format!("agy-acp-load-models-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&root);
        let selected_model = "Gemini 3.1 Pro (High)";
        let mut adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: root.to_string_lossy().to_string(),
            conversations_dir: root.join("conversations"),
            state_file: root.join("sessions.json"),
            available_models: Some(vec![
                "Gemini 3.5 Flash (Low)".to_string(),
                selected_model.to_string(),
            ]),
        };
        adapter.persist_session(
            "sess-1",
            Some("conv-abc"),
            5,
            Some(selected_model),
            None,
            None,
            None,
        );
        let response = adapter.handle_session_load(json!(7), &json!({"sessionId": "sess-1"}));
        assert!(response.error.is_none());

        let result = response
            .result
            .expect("session/load should return a result");
        assert_eq!(result["sessionId"], json!("sess-1"));
        let config_options = result["configOptions"]
            .as_array()
            .expect("session/load should include configOptions");
        assert_eq!(config_options.len(), 1);
        assert_eq!(config_options[0]["id"], json!("model"));
        assert_eq!(config_options[0]["currentValue"], json!(selected_model));

        let options = config_options[0]["options"]
            .as_array()
            .expect("model config option should include options");
        assert_eq!(options.len(), 2);
        assert!(options
            .iter()
            .any(|option| option["value"] == json!(selected_model)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore]
    fn test_persist_and_restore_session() {
        let root = std::env::temp_dir().join(format!("agy-acp-state-{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&root);
        let adapter = Adapter {
            sessions: HashMap::new(),
            working_dir: root.to_string_lossy().to_string(),
            conversations_dir: root.join("conversations"),
            state_file: root.join("sessions.json"),
            available_models: Some(vec![]),
        };
        adapter.persist_session("sess-1", Some("conv-abc"), 7, None, None, None, None);
        assert_eq!(
            adapter.restore_session("sess-1"),
            Some((Some("conv-abc".to_string()), 7, None, None, None, None))
        );
        assert_eq!(adapter.restore_session("sess-unknown"), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore]
    fn test_read_response_from_db() {
        let root = std::env::temp_dir().join(format!("agy-acp-sqlite-{}", Uuid::new_v4()));
        let conv_dir = root.join("conversations");
        fs::create_dir_all(&conv_dir).unwrap();
        let db_path = conv_dir.join("test-conv.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0, has_subtrajectory NUMERIC NOT NULL DEFAULT 0, metadata BLOB, error_details BLOB, permissions BLOB, task_details BLOB, render_info BLOB, step_payload BLOB, step_format INTEGER NOT NULL DEFAULT 0)").unwrap();
        let mut inner = Vec::new();
        inner.push(0x0A);
        inner.push(11);
        inner.extend_from_slice(b"hello world");
        let mut payload = Vec::new();
        payload.push(0x08);
        payload.push(0x0F);
        payload.push(0xA2);
        payload.push(0x01);
        payload.push(inner.len() as u8);
        payload.extend_from_slice(&inner);
        conn.execute(
            "INSERT INTO steps (idx, step_type, step_payload) VALUES (?1, 15, ?2)",
            rusqlite::params![1i64, payload],
        )
        .unwrap();
        drop(conn);
        let result = db::read_response_from_db(&conv_dir, "test-conv", -1);
        assert_eq!(result, Some(("hello world".to_string(), 1)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore]
    fn test_streaming_poll_emits_delta() {
        let root = std::env::temp_dir().join(format!("agy-acp-stream-{}", Uuid::new_v4()));
        let conv_dir = root.join("conversations");
        fs::create_dir_all(&conv_dir).unwrap();
        let db_path = conv_dir.join("stream-conv.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0, has_subtrajectory NUMERIC NOT NULL DEFAULT 0, metadata BLOB, error_details BLOB, permissions BLOB, task_details BLOB, render_info BLOB, step_payload BLOB, step_format INTEGER NOT NULL DEFAULT 0)").unwrap();
        fn make_payload(text: &str) -> Vec<u8> {
            let text_bytes = text.as_bytes();
            let mut inner = vec![0x0A];
            let mut len = text_bytes.len();
            loop {
                if len < 128 {
                    inner.push(len as u8);
                    break;
                }
                inner.push((len as u8 & 0x7F) | 0x80);
                len >>= 7;
            }
            inner.extend_from_slice(text_bytes);
            let mut outer = vec![0xA2, 0x01];
            let mut ilen = inner.len();
            loop {
                if ilen < 128 {
                    outer.push(ilen as u8);
                    break;
                }
                outer.push((ilen as u8 & 0x7F) | 0x80);
                ilen >>= 7;
            }
            outer.extend(inner);
            outer
        }
        conn.execute(
            "INSERT INTO steps (idx, step_type, step_payload) VALUES (?1, 15, ?2)",
            rusqlite::params![1i64, make_payload("hello")],
        )
        .unwrap();
        let state = Arc::new(Mutex::new(StreamingState {
            conversation_id: Some("stream-conv".to_string()),
            base_step_idx: -1,
            last_step_idx: -1,
            emitted_len: HashMap::new(),
            emitted_tool_steps: HashSet::new(),
            had_updates: false,
        }));
        let lines = streaming::poll_streaming_delta(&conv_dir, None, "sess-1", &state);
        assert_eq!(lines.len(), 1);
        let msg: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(msg["params"]["update"]["content"]["text"], "hello");
        let lines = streaming::poll_streaming_delta(&conv_dir, None, "sess-1", &state);
        assert!(lines.is_empty());
        conn.execute(
            "UPDATE steps SET step_payload = ?1 WHERE idx = 1",
            rusqlite::params![make_payload("hello world")],
        )
        .unwrap();
        let lines = streaming::poll_streaming_delta(&conv_dir, None, "sess-1", &state);
        assert_eq!(lines.len(), 1);
        let msg: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(msg["params"]["update"]["content"]["text"], " world");
        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore]
    fn test_streaming_replay_history() {
        let root = std::env::temp_dir().join(format!("agy-acp-replay-{}", Uuid::new_v4()));
        let conv_dir = root.join("conversations");
        fs::create_dir_all(&conv_dir).unwrap();
        let db_path = conv_dir.join("replay-conv.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER NOT NULL DEFAULT 0, status INTEGER NOT NULL DEFAULT 0, has_subtrajectory NUMERIC NOT NULL DEFAULT 0, metadata BLOB, error_details BLOB, permissions BLOB, task_details BLOB, render_info BLOB, step_payload BLOB, step_format INTEGER NOT NULL DEFAULT 0)").unwrap();
        fn make_payload(text: &str) -> Vec<u8> {
            let text_bytes = text.as_bytes();
            let mut inner = vec![0x0A];
            let mut len = text_bytes.len();
            loop {
                if len < 128 {
                    inner.push(len as u8);
                    break;
                }
                inner.push((len as u8 & 0x7F) | 0x80);
                len >>= 7;
            }
            inner.extend_from_slice(text_bytes);
            let mut outer = vec![0xA2, 0x01];
            let mut ilen = inner.len();
            loop {
                if ilen < 128 {
                    outer.push(ilen as u8);
                    break;
                }
                outer.push((ilen as u8 & 0x7F) | 0x80);
                ilen >>= 7;
            }
            outer.extend(inner);
            outer
        }
        conn.execute(
            "INSERT INTO steps (idx, step_type, step_payload) VALUES (?1, 15, ?2)",
            rusqlite::params![1i64, make_payload("first line")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO steps (idx, step_type, step_payload) VALUES (?1, 15, ?2)",
            rusqlite::params![2i64, make_payload("second line")],
        )
        .unwrap();

        let lines = streaming::replay_history(&conv_dir, "replay-conv", "sess-1");
        assert_eq!(lines.len(), 2);
        let msg1: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(msg1["params"]["update"]["content"]["text"], "first line");
        let msg2: Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(msg2["params"]["update"]["content"]["text"], "second line");

        drop(conn);
        let _ = fs::remove_dir_all(root);
    }

    fn prepare_auth() -> bool {
        if std::env::var("GEMINI_API_KEY")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
        {
            return true;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        if std::path::Path::new(&format!("{}/.gemini/antigravity-cli/settings.json", home)).exists()
        {
            return true;
        }
        eprintln!("SKIP: No auth found");
        false
    }

    #[test]
    #[ignore]
    fn test_e2e_agy_acp_full_round_trip() {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};
        if !prepare_auth() {
            return;
        }
        if std::process::Command::new("agy")
            .arg("--help")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            return;
        }
        let binary = std::env::current_dir()
            .unwrap()
            .join("target/release/agy-acp");
        if !binary.exists() {
            panic!("Run `cargo build --release` first");
        }
        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut send_recv = |msg: &str| -> String {
            writeln!(stdin, "{}", msg).unwrap();
            stdin.flush().unwrap();
            let mut l = String::new();
            reader.read_line(&mut l).unwrap();
            l
        };
        let resp = send_recv(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
        let init: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(init["result"]["protocolVersion"], 1);
        let resp = send_recv(r#"{"jsonrpc":"2.0","id":2,"method":"session/new","params":{}}"#);
        let session: Value = serde_json::from_str(&resp).unwrap();
        let sid = session["result"]["sessionId"].as_str().unwrap();
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{{"sessionId":"{}","prompt":[{{"type":"text","text":"Reply with exactly one word: PONG"}}]}}}}"#, sid).unwrap();
        stdin.flush().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        let mut got_notif = false;
        let mut text = String::new();
        loop {
            if std::time::Instant::now() > deadline {
                panic!("Timed out");
            }
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line.is_empty() {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            let msg: Value = serde_json::from_str(line.trim()).unwrap();
            if msg.get("method") == Some(&json!("session/update")) {
                got_notif = true;
                if let Some(t) = msg["params"]["update"]["content"]["text"].as_str() {
                    text.push_str(t);
                }
            }
            if msg.get("id") == Some(&json!(3)) {
                break;
            }
        }
        drop(stdin);
        let _ = child.wait();
        assert!(got_notif);
        assert!(text.to_lowercase().contains("pong"));
    }

use agy_acp::adapter::*;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn test_handle_session_new_with_missing_cwd() {
    let root = std::env::temp_dir().join(format!("agy-acp-test-new-{}", Uuid::new_v4()));
    let mut adapter = Adapter {
        sessions: HashMap::new(),
        working_dir: root.to_string_lossy().to_string(),
        conversations_dir: root.join("conversations"),
        state_file: root.join("sessions.json"),
        available_models: Some(vec![]),
    };
    
    let new_params = json!({});
    let res_new = adapter.handle_session_new(json!(1), &new_params);
    assert!(res_new.error.is_none());
    
    let result = res_new.result.unwrap();
    assert!(result.get("sessionId").is_some());
}

use openai_api_acp::adapter::*;
use openai_api_acp::types::*;
use serde_json::json;

#[test]
fn test_adapter_new_sanitization() {
    let adapter = Adapter::new(
        "\"https://example.com/v1\"/".to_string(),
        "\"my-key\"".to_string(),
        None,
    );
    assert_eq!(adapter.api_base, "https://example.com/v1");
    assert_eq!(adapter.api_key, "my-key");

    let adapter2 = Adapter::new(
        "'http://localhost:8000'".to_string(),
        "'secret'".to_string(),
        None,
    );
    assert_eq!(adapter2.api_base, "http://localhost:8000");
    assert_eq!(adapter2.api_key, "secret");
}

#[test]
fn test_get_home_dir() {
    let home = get_home_dir();
    assert!(!home.is_empty());
}

#[test]
fn test_static_fallback_models() {
    let models = Adapter::static_fallback_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0], "gpt-3.5-turbo");
}

#[test]
fn test_session_lifecycle() {
    let mut adapter = Adapter::new("http://example.com".to_string(), "key".to_string(), None);
    // Use a unique file to prevent interfering with real state
    adapter.state_file = std::env::temp_dir().join(format!("test_session_{}.json", uuid::Uuid::new_v4()));
    
    let session_id = "test-session";
    let stored = adapter.restore_session(session_id);
    assert!(stored.is_none());

    adapter.persist_session(
        session_id,
        vec![Message { role: "user".to_string(), content: "hello".to_string() }],
        Some("my-model"),
        Some("/tmp"),
        Some("My Title"),
        Some("2024-01-01T00:00:00Z"),
    );

    let restored = adapter.restore_session(session_id).unwrap();
    assert_eq!(restored.messages.len(), 1);
    assert_eq!(restored.model_id.unwrap(), "my-model");
    assert_eq!(restored.cwd.unwrap(), "/tmp");
    assert_eq!(restored.title.unwrap(), "My Title");
    
    let _ = std::fs::remove_file(adapter.state_file);
}

#[tokio::test]
async fn test_handle_session_new_missing_cwd() {
    let mut adapter = Adapter::new("http://example.com".to_string(), "key".to_string(), None);
    adapter.state_file = std::env::temp_dir().join(format!("test_session_new_{}.json", uuid::Uuid::new_v4()));
    
    let req = JsonRpcRequest {
        id: Some(json!(1)),
        method: Some("session/new".to_string()),
        params: Some(json!({})),
    };
    
    let res = adapter.handle_session_new(req.id.unwrap(), &req.params.unwrap()).await;
    assert!(res.error.is_none());
    assert!(res.result.is_some());
    let result = res.result.unwrap();
    assert!(result.get("sessionId").is_some());
    
    let _ = std::fs::remove_file(adapter.state_file);
}

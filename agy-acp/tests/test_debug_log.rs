use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::fs;
use uuid::Uuid;

#[test]
fn test_debug_log_command_line_argument() {
    let log_file = env::temp_dir().join(format!("debug_log_test_agy_{}.log", Uuid::new_v4()));
    let cargo_bin = env!("CARGO_BIN_EXE_agy-acp");

    let mut child = Command::new(cargo_bin)
        .arg("--debug-log")
        .arg(&log_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn process");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");

    let request_json = r#"{"jsonrpc": "2.0", "id": 1, "method": "session/close", "params": {"sessionId": "test"}}"#;
    writeln!(stdin, "{}", request_json).expect("Failed to write to stdin");
    stdin.flush().expect("Failed to flush stdin");

    let mut log_contents = String::new();
    for _ in 0..150 {
        if let Ok(contents) = fs::read_to_string(&log_file) {
            log_contents = contents;
            if log_contents.contains("-> {\"jsonrpc\":\"2.0\",\"id\":1") || log_contents.contains("-> {\"jsonrpc\": \"2.0\", \"id\": 1") || log_contents.contains("-> {\"error\"") || log_contents.contains("-> {\"result\"") {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();

    assert!(log_file.exists(), "Log file was not created");

    assert!(log_contents.contains("<- {\"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \"session/close\", \"params\": {\"sessionId\": \"test\"}}"), "Missing ingress log: {}", log_contents);
    assert!(log_contents.contains("-> {\"jsonrpc\":\"2.0\",\"id\":1") || log_contents.contains("-> {\"jsonrpc\": \"2.0\", \"id\": 1") || log_contents.contains("-> {\"error\"") || log_contents.contains("-> {\"result\""), "Missing egress log: {}", log_contents);

    let _ = fs::remove_file(log_file);
}

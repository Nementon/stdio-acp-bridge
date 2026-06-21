use reqwest;

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    
    // Invalid header
    let res = client.post("http://example.com")
        .header("Auth", "bearer\nfoo")
        .send().await;
    println!("Invalid header: {:?}", res);
    
    // Invalid URL
    let res = client.post("http://[::1]:8080/foo bar")
        .send().await;
    println!("Invalid URL: {:?}", res);
    
    // Invalid body
    // Not testing since serde_json::Value never fails
}

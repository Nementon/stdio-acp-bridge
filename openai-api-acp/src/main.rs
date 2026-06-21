#[tokio::main]
async fn main() {
    use clap::Parser;
    openai_api_acp::run(openai_api_acp::Args::parse()).await;
}

#[tokio::main]
async fn main() {
    use clap::Parser;
    agy_acp::run(agy_acp::Args::parse()).await;
}

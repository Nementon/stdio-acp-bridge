use clap::Parser;
use config::{Config, Environment, File};
use serde::Deserialize;
use std::process::exit;

#[derive(Parser, Debug)]
#[command(author, version, about = "STDIO ACP Bridge Orchestrator", long_about = None)]
struct CliArgs {
    /// Path to a YAML configuration file
    #[arg(long, short)]
    config: Option<String>,

    /// Which bridge to use: 'agy' or 'openai'
    #[arg(long)]
    bridge: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OrchestratorConfig {
    bridge: Option<String>,
    agy: Option<agy_acp::Args>,
    openai: Option<openai_api_acp::Args>,
}

#[tokio::main]
async fn main() {
    let cli = CliArgs::parse();

    let mut builder = Config::builder()
        // 1. Optional configuration file
        .add_source(
            File::with_name(&cli.config.clone().unwrap_or_else(|| "stdio-acp-bridge".to_string()))
                .required(false),
        )
        // 2. Environment variables prefixed with STDIO_ACPB_
        // e.g. STDIO_ACPB_BRIDGE=agy
        // STDIO_ACPB_OPENAI_API_KEY=xxx
        .add_source(Environment::with_prefix("STDIO_ACPB").separator("_"));

    // 3. Command line overrides
    if let Some(bridge) = cli.bridge {
        builder = builder.set_override("bridge", bridge).unwrap();
    }

    let config_res = builder.build();
    let config = match config_res {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to build configuration: {}", e);
            exit(1);
        }
    };

    let orch_config: OrchestratorConfig = match config.try_deserialize() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to parse configuration: {}", e);
            exit(1);
        }
    };

    let bridge_name = orch_config.bridge.unwrap_or_else(|| {
        eprintln!("No bridge specified. Please specify --bridge 'agy' or 'openai', or set it in the configuration.");
        exit(1);
    });

    match bridge_name.as_str() {
        "agy" => {
            let agy_args = orch_config.agy.unwrap_or_default();
            agy_acp::run(agy_args).await;
        }
        "openai" => {
            let openai_args = orch_config.openai.unwrap_or_default();
            openai_api_acp::run(openai_args).await;
        }
        _ => {
            eprintln!("Unknown bridge type: {}. Use 'agy' or 'openai'.", bridge_name);
            exit(1);
        }
    }
}

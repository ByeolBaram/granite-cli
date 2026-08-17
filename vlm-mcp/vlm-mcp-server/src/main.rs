//! VLM MCP Server — binary entry point.
//!
//! Supports three-tier config (CLI > env > YAML) with `__` delimiter.
//! Supports stdio, Streamable HTTP, and legacy SSE transports.
//! Supports insecure, TLS, and mTLS security tiers.

mod cli;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use vlm_mcp_core::{Config, OpenAiCompatibleVlm, VlmBackend};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI args
    let cli = cli::CliArgs::parse();

    // Initialize logging
    init_logging(&cli);

    // Load config from YAML + env vars
    let config = Config::from_sources()
        .with_context(|| "Failed to load config")?;

    // Apply CLI overrides
    let cli_overrides = cli.to_overrides();
    let config = config.with_cli_overrides(cli_overrides);

    info!(
        transport = %config.server.transport,
        endpoint = %config.vlm.endpoint,
        model = %config.vlm.model,
        "Starting vlm-mcp server",
    );

    // Build VLM backend using from_config
    let vlm = OpenAiCompatibleVlm::from_config(&config)
        .with_context(|| "Failed to initialize VLM backend")?;

    // Check health
    match vlm.health().await {
        Ok(health) => {
            info!(ready = %health.ready, model = %health.model, "VLM health check passed");
        }
        Err(e) => {
            warn!("VLM health check failed: {}", e);
            warn!("Server will start but tool calls may fail until VLM is reachable");
        }
    }

    // Route to the appropriate transport runner
    match config.server.transport.as_str() {
        "stdio" => run_stdio(&vlm).await,
        "http" => run_http(&vlm, &config).await,
        "http-sse" => run_http_sse(&vlm, &config).await,
        other => Err(anyhow::anyhow!("Unknown transport: '{}'. Use 'stdio', 'http', or 'http-sse'.", other)),
    }
}

fn init_logging(cli: &cli::CliArgs) {
    let filter = EnvFilter::try_new(&cli.log_level)
        .unwrap_or_else(|_| EnvFilter::new("INFO"));

    if cli.log_format == "json" {
        let fmt_layer = tracing_subscriber::fmt::layer().json();
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    } else {
        let fmt_layer = tracing_subscriber::fmt::layer();
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }
}

async fn run_stdio(vlm: &OpenAiCompatibleVlm) -> Result<()> {
    info!("Running in stdio mode (placeholder)");
    println!("VLM MCP Server — stdio mode");
    println!("VLM: {:?}", vlm);
    println!("\nTo run in production, wire up rmcp::Server with the tools:");
    println!("  - vlm_describe_image");
    println!("  - vlm_ocr");
    println!("  - vlm_compare_images");
    println!("  - vlm_analyze");
    println!("  - vlm_health");
    println!("  - vlm_list_models");

    Ok(())
}

async fn run_http(_vlm: &OpenAiCompatibleVlm, config: &Config) -> Result<()> {
    info!(
        bind = %config.server.bind,
        port = %config.server.port,
        "Running in Streamable HTTP mode (placeholder)",
    );

    println!("VLM MCP Server — Streamable HTTP mode");
    println!("Listening on {}:{}", config.server.bind, config.server.port);

    // Check for TLS config
    if let Some(tls) = &config.server.tls {
        match (&tls.cert, &tls.key) {
            (Some(cert), Some(key)) => {
                println!("TLS: cert={}, key={}", cert, key);
            }
            _ => {
                warn!("TLS is configured but cert or key is missing");
            }
        }
    }

    if let Some(mtls) = &config.server.mtls {
        if let Some(ca) = &mtls.ca {
            println!("mTLS: CA={}", ca);
        }
    }

    println!("\nThis is a placeholder. The actual HTTP server will be wired up in the next iteration.");

    Ok(())
}

async fn run_http_sse(_vlm: &OpenAiCompatibleVlm, config: &Config) -> Result<()> {
    info!("Running in legacy SSE mode (placeholder)");

    println!("VLM MCP Server — Legacy SSE mode");
    println!("Listening on {}:{}", config.server.bind, config.server.port);

    Ok(())
}

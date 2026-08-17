//! CLI argument parsing with clap.
//!
//! All arguments map directly to config fields.
//! Environment variables use VLM_MCP__ prefix with __ delimiter.

use clap::Parser;
use vlm_mcp_core::app_config::CliOverrides;

#[derive(Parser, Debug)]
#[command(
    name = "vlm-mcp-server",
    about = "MCP server for Vision Language Model integration",
    version,
)]
pub struct CliArgs {
    /// Transport type: "stdio", "http", or "http-sse"
    #[arg(long, env = "VLM_MCP__SERVER__TRANSPORT")]
    pub transport: Option<String>,

    /// Bind address for HTTP transport
    #[arg(long, env = "VLM_MCP__SERVER__BIND")]
    pub bind: Option<String>,

    /// Port for HTTP transport
    #[arg(long, env = "VLM_MCP__SERVER__PORT")]
    pub port: Option<u16>,

    /// VLM API endpoint (e.g., http://localhost:11434/v1)
    #[arg(long, env = "VLM_MCP__VLM__ENDPOINT")]
    pub vlm_endpoint: Option<String>,

    /// VLM model name
    #[arg(long, env = "VLM_MCP__VLM__MODEL")]
    pub vlm_model: Option<String>,

    /// VLM API key
    #[arg(long, env = "VLM_MCP__VLM__API_KEY")]
    pub vlm_api_key: Option<String>,

    /// VLM request timeout in seconds
    #[arg(long, env = "VLM_MCP__VLM__TIMEOUT_SECONDS")]
    pub vlm_timeout: Option<u64>,

    /// Skip hostname verification for VLM TLS (use with self-signed certs)
    #[arg(long, env = "VLM_MCP__VLM__TLS__VERIFY_HOSTNAME")]
    pub vlm_verify_hostname: Option<bool>,

    /// TLS server certificate path
    #[arg(long, env = "VLM_MCP__SERVER__TLS__CERT")]
    pub tls_cert: Option<String>,

    /// TLS server key path
    #[arg(long, env = "VLM_MCP__SERVER__TLS__KEY")]
    pub tls_key: Option<String>,

    /// DoS protection: max image size in bytes
    #[arg(long, env = "VLM_MCP__SERVER__DOS_PROTECTION__MAX_IMAGE_BYTES")]
    pub dos_max_image_bytes: Option<u64>,

    /// Log level: TRACE, DEBUG, INFO, WARN, ERROR
    #[arg(long, default_value = "INFO")]
    pub log_level: String,

    /// Log format: "plain" or "json"
    #[arg(long, default_value = "plain")]
    pub log_format: String,
}

impl CliArgs {
    /// Convert CLI args to config overrides.
    pub fn to_overrides(self) -> CliOverrides {
        CliOverrides {
            transport: self.transport,
            bind: self.bind,
            port: self.port,
            vlm_endpoint: self.vlm_endpoint,
            vlm_model: self.vlm_model,
            vlm_api_key: self.vlm_api_key,
            vlm_timeout: self.vlm_timeout,
            vlm_verify_hostname: self.vlm_verify_hostname,
            dos_max_image_bytes: self.dos_max_image_bytes,
            log_level: Some(self.log_level),
            log_format: Some(self.log_format),
        }
    }
}

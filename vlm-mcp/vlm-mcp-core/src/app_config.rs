//! Typed configuration structs with YAML + env var + CLI resolution.
//!
//! All config fields are accessible at three levels:
//! - YAML config file (nested structure)
//! - Environment variable (`VLM_MCP__<FIELD_PATH>` with `__` delimiter)
//! - CLI flag (`--<field-path>` with `__` delimiter)
//!
//! Precedence: CLI > env var > YAML file > defaults.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Root configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub vlm: VlmConfig,
    pub logging: LoggingConfig,
}

/// Server transport and security settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Transport type: "stdio", "http", "http-sse".
    pub transport: String,
    /// Bind address for HTTP transport.
    pub bind: String,
    /// Port for HTTP transport.
    pub port: u16,
    /// TLS configuration.
    pub tls: Option<TlsServerConfig>,
    /// mTLS configuration.
    pub mtls: Option<MtlsConfig>,
    /// DoS protection settings.
    pub dos_protection: DoSProtection,
}

/// Server TLS settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsServerConfig {
    /// Path to server certificate (PEM).
    pub cert: Option<String>,
    /// Path to server private key (PEM).
    pub key: Option<String>,
}

/// mTLS settings (overrides TLS when present).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtlsConfig {
    /// CA certificate for verifying client certificates (PEM).
    pub ca: Option<String>,
}

/// DoS protection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoSProtection {
    /// Max image upload size in bytes.
    pub max_image_bytes: u64,
}

/// VLM backend connection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmConfig {
    /// API endpoint URL.
    pub endpoint: String,
    /// Model name.
    pub model: String,
    /// API key (optional).
    pub api_key: String,
    /// Request timeout in seconds.
    pub timeout_seconds: u64,
    /// TLS settings for the VLM client connection.
    pub tls: Option<VlmClientTlsConfig>,
    /// Per-adapter DoS protection.
    pub dos_protection: Option<DoSProtection>,
    /// Extra headers to include in VLM requests.
    pub extra_headers: std::collections::HashMap<String, String>,
}

/// TLS configuration for the VLM client connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmClientTlsConfig {
    /// CA certificate to verify the VLM's server cert (PEM).
    pub ca: Option<String>,
    /// Client certificate for mTLS to VLM (PEM).
    pub cert: Option<String>,
    /// Client private key for mTLS to VLM (PEM).
    pub key: Option<String>,
    /// When false, skip hostname verification but still check chain, expiry, key usage.
    pub verify_hostname: bool,
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level: TRACE, DEBUG, INFO, WARN, ERROR.
    pub level: String,
    /// Log format: "plain" or "json".
    pub format: String,
}

// ─── Defaults ───────────────────────────────────────────────────────

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            vlm: VlmConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            transport: "stdio".to_string(),
            bind: "127.0.0.1".to_string(),
            port: 8080,
            tls: None,
            mtls: None,
            dos_protection: DoSProtection::default(),
        }
    }
}

impl Default for DoSProtection {
    fn default() -> Self {
        Self {
            max_image_bytes: 50 * 1024 * 1024, // 50 MiB
        }
    }
}

impl Default for VlmConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:11434/v1".to_string(),
            model: "qwen2.5-vl:72b".to_string(),
            api_key: String::new(),
            timeout_seconds: 120,
            tls: None,
            dos_protection: None,
            extra_headers: std::collections::HashMap::new(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "INFO".to_string(),
            format: "plain".to_string(),
        }
    }
}

// ─── 3-Tier Resolution ─────────────────────────────────────────────

impl Config {
    /// Load config from YAML file + env vars.
    ///
    /// Precedence: env vars > YAML file > built-in defaults.
    /// CLI overrides are applied separately via `with_cli_overrides`.
    pub fn from_sources() -> Result<Self> {
        let builder = config::Config::builder()
            .add_source(config::File::with_name("config").required(false));

        let builder = builder.add_source(
            config::Environment::with_prefix("VLM_MCP")
                .separator("__")
                .try_parsing(true),
        );

        let config: Config = builder
            .build()
            .with_context(|| "Failed to build config")?
            .try_deserialize()
            .with_context(|| "Failed to deserialize config")?;

        Ok(config)
    }

    /// Apply CLI overrides to a base config.
    pub fn with_cli_overrides(mut self, overrides: CliOverrides) -> Self {
        if let Some(transport) = overrides.transport {
            self.server.transport = transport;
        }
        if let Some(bind) = overrides.bind {
            self.server.bind = bind;
        }
        if let Some(port) = overrides.port {
            self.server.port = port;
        }
        if let Some(endpoint) = overrides.vlm_endpoint {
            self.vlm.endpoint = endpoint;
        }
        if let Some(model) = overrides.vlm_model {
            self.vlm.model = model;
        }
        if let Some(api_key) = &overrides.vlm_api_key {
            self.vlm.api_key = api_key.clone();
        }
        if let Some(timeout) = overrides.vlm_timeout {
            self.vlm.timeout_seconds = timeout;
        }
        if let Some(verify_hostname) = overrides.vlm_verify_hostname {
            if let Some(tls) = &mut self.vlm.tls {
                tls.verify_hostname = verify_hostname;
            }
        }
        if let Some(max_bytes) = overrides.dos_max_image_bytes {
            self.server.dos_protection.max_image_bytes = max_bytes;
        }
        if let Some(level) = &overrides.log_level {
            self.logging.level = level.clone();
        }
        if let Some(fmt) = &overrides.log_format {
            self.logging.format = fmt.clone();
        }
        self
    }
}

/// CLI-provided overrides.
#[derive(Debug, Clone)]
pub struct CliOverrides {
    pub transport: Option<String>,
    pub bind: Option<String>,
    pub port: Option<u16>,
    pub vlm_endpoint: Option<String>,
    pub vlm_model: Option<String>,
    pub vlm_api_key: Option<String>,
    pub vlm_timeout: Option<u64>,
    pub vlm_verify_hostname: Option<bool>,
    pub dos_max_image_bytes: Option<u64>,
    pub log_level: Option<String>,
    pub log_format: Option<String>,
}

impl Default for CliOverrides {
    fn default() -> Self {
        Self {
            transport: None,
            bind: None,
            port: None,
            vlm_endpoint: None,
            vlm_model: None,
            vlm_api_key: None,
            vlm_timeout: None,
            vlm_verify_hostname: None,
            dos_max_image_bytes: None,
            log_level: None,
            log_format: None,
        }
    }
}

impl CliOverrides {
    /// Apply overrides to the server config.
    pub fn apply_to_server(&self, config: &mut ServerConfig) {
        if let Some(transport) = &self.transport {
            config.transport = transport.clone();
        }
        if let Some(bind) = &self.bind {
            config.bind = bind.clone();
        }
        if let Some(port) = self.port {
            config.port = port;
        }
        if let Some(max_bytes) = self.dos_max_image_bytes {
            config.dos_protection.max_image_bytes = max_bytes;
        }
    }

    /// Apply overrides to the VLM config.
    pub fn apply_to_vlm(&self, config: &mut VlmConfig) {
        if let Some(endpoint) = &self.vlm_endpoint {
            config.endpoint = endpoint.clone();
        }
        if let Some(model) = &self.vlm_model {
            config.model = model.clone();
        }
        if let Some(api_key) = &self.vlm_api_key {
            config.api_key = api_key.clone();
        }
        if let Some(timeout) = self.vlm_timeout {
            config.timeout_seconds = timeout;
        }
        if let Some(verify_hostname) = self.vlm_verify_hostname {
            if let Some(tls) = &mut config.tls {
                tls.verify_hostname = verify_hostname;
            }
        }
    }

    /// Apply overrides to the logging config.
    pub fn apply_to_logging(&self, config: &mut LoggingConfig) {
        if let Some(level) = &self.log_level {
            config.level = level.clone();
        }
        if let Some(fmt) = &self.log_format {
            config.format = fmt.clone();
        }
    }
}

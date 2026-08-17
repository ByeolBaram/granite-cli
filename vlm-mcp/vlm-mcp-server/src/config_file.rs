//! Configuration file handling.

use anyhow::Result;
use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigFile {
    pub server: ServerFileConfig,
    pub vlm: VlmFileConfig,
    pub logging: LoggingFileConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerFileConfig {
    pub transport: String,
    pub bind: String,
    pub port: u16,
    pub tls: Option<TlsFileConfig>,
    pub mtls: Option<MtlsFileConfig>,
    pub dos_protection: DoSFileConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TlsFileConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MtlsFileConfig {
    pub ca: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DoSFileConfig {
    pub max_image_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VlmFileConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub timeout_seconds: u64,
    pub tls: Option<VlmTlsFileConfig>,
    pub dos_protection: Option<DoSFileConfig>,
    pub extra_headers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VlmTlsFileConfig {
    pub ca: Option<String>,
    pub cert: Option<String>,
    pub key: Option<String>,
    pub verify_hostname: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggingFileConfig {
    pub level: String,
    pub format: String,
}

impl ConfigFile {
    /// Load configuration from a YAML file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a YAML file.
    pub fn to_file(&self, path: &Path) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

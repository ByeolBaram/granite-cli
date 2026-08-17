//! vlm-mcp-core — transport-agnostic VLM MCP library.
//!
//! This crate provides:
//! - The `VlmBackend` trait for VLM backends
//! - `OpenAiCompatibleVlm` and `OllamaVlm` implementations
//! - MCP tool definitions via `#[tool]`/`#[tool_router]` macros
//! - Typed configuration with 3-tier precedence

pub mod app_config;
pub mod tools;
pub mod vlm;

pub use app_config::Config;
pub use app_config::CliOverrides;
pub use tools::VlmToolRegistry;
pub use vlm::{ImageSource, AnalysisType, VlmBackend, VlmError, VlmHealth};

// Re-export default implementations for library users
pub use vlm::OpenAiCompatibleVlm;
pub use vlm::OllamaVlm;

/// Builder for an MCP server — wires up config + VLM backend.
///
/// Library users construct a `ServerBuilder`, optionally providing
/// their own VLM backend, then call `build()` to get a configured
/// server that can be run with any transport layer.
pub struct ServerBuilder {
    config: Config,
}

impl ServerBuilder {
    /// Create a new builder with default config.
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Create a builder from a loaded config.
    pub fn from_config(config: Config) -> Self {
        Self { config }
    }

    /// Load config from YAML file + env vars.
    pub fn load_config() -> Result<Self, anyhow::Error> {
        let config = Config::from_sources()?;
        Ok(Self { config })
    }

    /// Build a configured server handle.
    pub fn build(self) -> Result<ServerHandle, anyhow::Error> {
        let vlm = OpenAiCompatibleVlm::from_config(&self.config)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(ServerHandle {
            config: self.config,
            vlm,
        })
    }
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to a built server — contains config + VLM backend.
///
/// The actual rmcp::Server is constructed in the binary crate.
pub struct ServerHandle {
    pub config: Config,
    pub vlm: OpenAiCompatibleVlm,
}

impl ServerHandle {
    /// Get the VLM backend reference.
    pub fn vlm(&self) -> &OpenAiCompatibleVlm {
        &self.vlm
    }

    /// Get the config.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

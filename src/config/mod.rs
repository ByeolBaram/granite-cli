pub mod exports;
pub mod shell;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub models: HashMap<String, ModelConfig>,
    pub providers: HashMap<String, ProviderConfig>,
    pub capabilities: HashMap<String, CapabilityConfig>,
    pub routing: RoutingConfig,
    pub shell: ShellConfig,
    pub tools: HashMap<String, ToolConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            models: HashMap::new(),
            providers: HashMap::new(),
            capabilities: HashMap::new(),
            routing: RoutingConfig::default(),
            shell: ShellConfig::default(),
            tools: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_id: String,
    pub provider_id: Option<String>,
    pub variant: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub enabled: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            provider_id: None,
            variant: None,
            endpoint: None,
            api_key: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub enabled: bool,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            name: String::new(),
            provider_type: String::new(),
            endpoint: String::new(),
            api_key: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityConfig {
    pub capability_id: String,
    pub enabled: bool,
    pub config: HashMap<String, String>,
}

impl Default for CapabilityConfig {
    fn default() -> Self {
        Self {
            capability_id: String::new(),
            enabled: false,
            config: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub model_routes: HashMap<String, Vec<ProviderRoute>>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            model_routes: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRoute {
    pub provider_id: String,
    pub priority: u8,
    pub health_check: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    pub shell: String,
    pub export_file: PathBuf,
    pub export_format: String,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let detected = shell::detect_shell();
        Self {
            shell: detected.0,
            export_file: detected.1,
            export_format: detected.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub tool_id: String,
    pub tool_version: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub env_vars: HashMap<String, String>,
    pub capabilities: Vec<ConfiguredCapability>,
    pub export_to_shell: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfiguredCapability {
    pub capability_id: String,
    pub enabled: bool,
    pub config: HashMap<String, String>,
}

impl Config {

    fn config_dir() -> Result<PathBuf> {
        let val_res = std::env::var("GRANITE_CLI_HOME");

        if let Ok(val) = val_res {
            if !val.is_empty() {
                let path = PathBuf::from(&val);

                // Check: Does the parent exist (or is there no parent, e.g. root)?
                // map_or(true, ...) means: If None -> true; Else check inner exists
                let has_valid_parent = path.parent().map_or(true, |p| p.exists());

                let valid_dir =
                    (!path.exists() && has_valid_parent) ||  // Can create new dir in existing parent (or root)
                    (path.exists() && path.is_dir());        // Already exists as directory

                if !valid_dir {
                    anyhow::bail!("Invalid GRANITE_CLI_HOME: '{}' parent does not exist or is not a directory.", val);
                }

                return Ok(path);
            }
        }

        let default_dir = dirs::config_dir().ok_or_else(|| anyhow::Error::msg("Could not determine system configuration directory"))?;

        Ok(default_dir.join("granite-cli"))
    }

    fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.yaml"))
    }

    fn models_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("models"))
    }

    fn providers_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("providers"))
    }

    fn capabilities_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("capabilities"))
    }

    fn tools_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("tools"))
    }

    fn ensure_directories() -> Result<()> {
        let config_dir = Self::config_dir()?;
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
        }
        fs::create_dir_all(Self::models_dir()?)?;
        fs::create_dir_all(Self::providers_dir()?)?;
        fs::create_dir_all(Self::capabilities_dir()?)?;
        fs::create_dir_all(Self::tools_dir()?)?;
        Ok(())
    }

    /*-- pub -- */

    pub fn load() -> Result<Self> {
        Self::ensure_directories()?;
        let path = Self::config_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            let config: Config =
                serde_yaml::from_str(&content).with_context(|| "Failed to parse config file")?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        Self::ensure_directories()?;
        let path = Self::config_path()?;
        let content = serde_yaml::to_string(self)
            .with_context(|| "Failed to serialize config")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;
        Ok(())
    }

    pub fn save_model(&self, model_id: &str, model_config: &ModelConfig) -> Result<()> {
        Self::ensure_directories()?;
        let path = Self::models_dir()?.join(format!("{}.yaml", model_id));
        let content = serde_yaml::to_string(model_config)
            .with_context(|| "Failed to serialize model config")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write model config: {}", path.display()))?;
        Ok(())
    }

    pub fn load_model(model_id: &str) -> Result<Option<ModelConfig>> {
        let path = Self::models_dir()?.join(format!("{}.yaml", model_id));
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read model config: {}", path.display()))?;
            let config: ModelConfig =
                serde_yaml::from_str(&content).with_context(|| "Failed to parse model config")?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    pub fn save_provider(&self, provider_id: &str, provider_config: &ProviderConfig) -> Result<()> {
        Self::ensure_directories()?;
        let path = Self::providers_dir()?.join(format!("{}.yaml", provider_id));
        let content = serde_yaml::to_string(provider_config)
            .with_context(|| "Failed to serialize provider config")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write provider config: {}", path.display()))?;
        Ok(())
    }

    pub fn load_provider(provider_id: &str) -> Result<Option<ProviderConfig>> {
        let path = Self::providers_dir()?.join(format!("{}.yaml", provider_id));
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read provider config: {}", path.display()))?;
            let config: ProviderConfig =
                serde_yaml::from_str(&content).with_context(|| "Failed to parse provider config")?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    pub fn save_capability(&self, capability_id: &str, capability_config: &CapabilityConfig) -> Result<()> {
        Self::ensure_directories()?;
        let path = Self::capabilities_dir()?.join(format!("{}.yaml", capability_id));
        let content = serde_yaml::to_string(capability_config)
            .with_context(|| "Failed to serialize capability config")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write capability config: {}", path.display()))?;
        Ok(())
    }

    pub fn load_capability(capability_id: &str) -> Result<Option<CapabilityConfig>> {
        let path = Self::capabilities_dir()?.join(format!("{}.yaml", capability_id));
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read capability config: {}", path.display()))?;
            let config: CapabilityConfig =
                serde_yaml::from_str(&content).with_context(|| "Failed to parse capability config")?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    pub fn save_tool(&self, tool_id: &str, tool_config: &ToolConfig) -> Result<()> {
        Self::ensure_directories()?;
        let path = Self::tools_dir()?.join(format!("{}.yaml", tool_id));
        let content = serde_yaml::to_string(tool_config)
            .with_context(|| "Failed to serialize tool config")?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write tool config: {}", path.display()))?;
        Ok(())
    }

    pub fn load_tool(tool_id: &str) -> Result<Option<ToolConfig>> {
        let path = Self::tools_dir()?.join(format!("{}.yaml", tool_id));
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read tool config: {}", path.display()))?;
            let config: ToolConfig =
                serde_yaml::from_str(&content).with_context(|| "Failed to parse tool config")?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }
}

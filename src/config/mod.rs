pub mod exports;
pub mod shell;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct TopLevelConfig {
    pub routing: RoutingConfig,
    pub shell: ShellConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct Config {
    pub models: HashMap<String, ModelConfig>,
    pub providers: HashMap<String, ProviderConfig>,
    pub capabilities: HashMap<String, CapabilityConfig>,
    pub routing: RoutingConfig,
    pub shell: ShellConfig,
    pub tools: HashMap<String, ToolConfig>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_id: String,
    pub provider_id: Option<String>,
    pub variant: Option<String>,
    pub enabled: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            provider_id: None,
            variant: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_id: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            provider_type: String::new(),
            config: serde_json::Value::Object(serde_json::Map::new()),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct CapabilityConfig {
    pub capability_id: String,
    pub enabled: bool,
    pub config: HashMap<String, String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct RoutingConfig {
    pub model_routes: HashMap<String, Vec<ProviderRoute>>,
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

        if let Ok(val) = val_res
            && !val.is_empty() {
                let path = PathBuf::from(&val);

                let has_valid_parent = path.parent().is_none_or(|p| p.exists());

                let valid_dir =
                    (!path.exists() && has_valid_parent) || (path.exists() && path.is_dir());

                if !valid_dir {
                    anyhow::bail!(
                        "Invalid GRANITE_CLI_HOME: '{}' parent does not exist or is not a directory.",
                        val
                    );
                }

                return Ok(path);
            }

        let default_dir = dirs::config_dir().ok_or_else(|| {
            anyhow::Error::msg("Could not determine system configuration directory")
        })?;

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

    fn load_yaml_from_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: T = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        Ok(config)
    }

    fn save_yaml_to_file<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
        let content = serde_yaml::to_string(data).with_context(|| "Failed to serialize config")?;
        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;
        Ok(())
    }

    fn load_dir<K: std::hash::Hash + Eq + ToString, V: serde::de::DeserializeOwned>(
        dir: &Path,
        into_key: impl Fn(&str) -> K + Copy,
    ) -> Result<HashMap<K, V>> {
        let mut map = HashMap::new();
        if !dir.exists() {
            return Ok(map);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "yaml") {
                let file_name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Ok(config) = Self::load_yaml_from_file::<V>(&path) {
                    map.insert(into_key(&file_name), config);
                }
            }
        }
        Ok(map)
    }

    pub fn new() -> Result<Self> {
        Self::ensure_directories()?;

        let mut config = Config::default();

        // Load top-level config.yaml for shell and routing
        let top_level_path = Self::config_path()?;
        if top_level_path.exists() {
            if let Ok(top_config) = Self::load_yaml_from_file::<TopLevelConfig>(&top_level_path) {
                config.shell = top_config.shell;
                config.routing = top_config.routing;
            }
        } else {
            config.save()?;
        }

        // Load component files
        config.models = Self::load_dir(&Self::models_dir()?, |s| s.to_string())?;
        config.providers = Self::load_dir(&Self::providers_dir()?, |s| s.to_string())?;
        config.capabilities = Self::load_dir(&Self::capabilities_dir()?, |s| s.to_string())?;
        config.tools = Self::load_dir(&Self::tools_dir()?, |s| s.to_string())?;

        Ok(config)
    }

    fn save(&self) -> Result<()> {
        // Save top-level config.yaml with shell and routing
        let top_level_path = Self::config_path()?;
        let top_config = TopLevelConfig {
            routing: self.routing.clone(),
            shell: self.shell.clone(),
        };
        Self::save_yaml_to_file(&top_level_path, &top_config)?;

        // Save individual model files
        for (id, model) in &self.models {
            let path = Self::models_dir()?.join(format!("{}.yaml", id));
            Self::save_yaml_to_file(&path, model)?;
        }

        // Save individual provider files
        for (id, provider) in &self.providers {
            let path = Self::providers_dir()?.join(format!("{}.yaml", id));
            Self::save_yaml_to_file(&path, provider)?;
        }

        // Save individual capability files
        for (id, capability) in &self.capabilities {
            let path = Self::capabilities_dir()?.join(format!("{}.yaml", id));
            Self::save_yaml_to_file(&path, capability)?;
        }

        // Save individual tool files
        for (id, tool) in &self.tools {
            let path = Self::tools_dir()?.join(format!("{}.yaml", id));
            Self::save_yaml_to_file(&path, tool)?;
        }

        Ok(())
    }

    // -- Model --

    pub fn get_model(&self, id: &str) -> Option<&ModelConfig> {
        self.models.get(id)
    }

    pub fn insert_model(&mut self, id: &str, config: ModelConfig) -> Result<()> {
        self.models.insert(id.to_string(), config);
        self.save()
    }

    pub fn remove_model(&mut self, id: &str) -> Result<()> {
        self.models.remove(id);
        let path = Self::models_dir().ok().and_then(|d| {
            let p = d.join(format!("{}.yaml", id));
            if p.exists() { Some(p) } else { None }
        });
        if let Some(p) = path {
            let _ = fs::remove_file(&p);
        }
        self.save()
    }

    pub fn update_model(&mut self, id: &str, f: impl FnOnce(&mut ModelConfig)) -> Result<()> {
        if let Some(model) = self.models.get_mut(id) {
            f(model);
            self.save()
        } else {
            Ok(())
        }
    }

    // -- Provider --

    pub fn get_provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.get(id)
    }

    pub fn insert_provider(&mut self, id: &str, config: ProviderConfig) -> Result<()> {
        self.providers.insert(id.to_string(), config);
        self.save()
    }

    pub fn remove_provider(&mut self, id: &str) -> Result<()> {
        self.providers.remove(id);
        let path = Self::providers_dir().ok().and_then(|d| {
            let p = d.join(format!("{}.yaml", id));
            if p.exists() { Some(p) } else { None }
        });
        if let Some(p) = path {
            let _ = fs::remove_file(&p);
        }
        self.save()
    }

    pub fn update_provider(&mut self, id: &str, f: impl FnOnce(&mut ProviderConfig)) -> Result<()> {
        if let Some(provider) = self.providers.get_mut(id) {
            f(provider);
            self.save()
        } else {
            Ok(())
        }
    }

    // -- Capability --

    pub fn get_capability(&self, id: &str) -> Option<&CapabilityConfig> {
        self.capabilities.get(id)
    }

    pub fn insert_capability(&mut self, id: &str, config: CapabilityConfig) -> Result<()> {
        self.capabilities.insert(id.to_string(), config);
        self.save()
    }

    pub fn remove_capability(&mut self, id: &str) -> Result<()> {
        self.capabilities.remove(id);
        let path = Self::capabilities_dir().ok().and_then(|d| {
            let p = d.join(format!("{}.yaml", id));
            if p.exists() { Some(p) } else { None }
        });
        if let Some(p) = path {
            let _ = fs::remove_file(&p);
        }
        self.save()
    }

    pub fn update_capability(
        &mut self,
        id: &str,
        f: impl FnOnce(&mut CapabilityConfig),
    ) -> Result<()> {
        if let Some(capability) = self.capabilities.get_mut(id) {
            f(capability);
            self.save()
        } else {
            Ok(())
        }
    }

    // -- Tool --

    pub fn get_tool(&self, id: &str) -> Option<&ToolConfig> {
        self.tools.get(id)
    }

    pub fn insert_tool(&mut self, id: &str, config: ToolConfig) -> Result<()> {
        self.tools.insert(id.to_string(), config);
        self.save()
    }

    pub fn remove_tool(&mut self, id: &str) -> Result<()> {
        self.tools.remove(id);
        let path = Self::tools_dir().ok().and_then(|d| {
            let p = d.join(format!("{}.yaml", id));
            if p.exists() { Some(p) } else { None }
        });
        if let Some(p) = path {
            let _ = fs::remove_file(&p);
        }
        self.save()
    }

    pub fn update_tool(&mut self, id: &str, f: impl FnOnce(&mut ToolConfig)) -> Result<()> {
        if let Some(tool) = self.tools.get_mut(id) {
            f(tool);
            self.save()
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn test_config_default_shell_detection() {
        let config = Config::default();
        assert!(!config.shell.shell.is_empty());
        assert!(!config.shell.export_format.is_empty());
    }
}

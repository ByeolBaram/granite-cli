use anyhow::Result;

use crate::capabilities::{Capability, ConfigureResult, Dependency, EnvBinding, Factory};

/// Mellea skills compiler capability.
/// Compiles raw skills into tool-specific formats with Granite Guardian integration.
/// Note: Full implementation deferred — this is an abstraction placeholder.
pub struct CompilerCapability {
    id: String,
    name: String,
    description: String,
    dependencies: Vec<Dependency>,
}

impl CompilerCapability {
    pub fn new() -> Self {
        Self {
            id: "compiler".to_string(),
            name: "Skills Compiler".to_string(),
            description: "Mellea skills compiler - compile raw skills into tool-specific formats with Granite Guardian integration.".to_string(),
            dependencies: vec![
                Dependency::Model {
                    id: "granite-guardian-3.1-8b".to_string(),
                    required: true,
                },
                Dependency::ExternalTool {
                    name: "mellea-compiler".to_string(),
                    check_command: "mellea --version".to_string(),
                },
            ],
        }
    }

    fn check_compiler_available() -> bool {
        let result = std::process::Command::new("mellea")
            .args(["--version"])
            .output();

        match result {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

impl Default for CompilerCapability {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Capability for CompilerCapability {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn dependencies(&self) -> Vec<Dependency> {
        self.dependencies.clone()
    }

    async fn on_setup(&self, factory: &dyn Factory) -> Result<()> {
        println!("Checking Granite Guardian model...");
        let _has_model = factory.resolve_model("granite-guardian-3.1-8b").await.is_ok();

        if Self::check_compiler_available() {
            println!("Mellea compiler is available.");
        } else {
            println!(
                "\nMellea compiler is not installed. Run:\n  pip install mellea\n\n\
                 Skills compilation will be available once installed."
            );
        }

        Ok(())
    }

    async fn on_configure(&self, _tool: &crate::capabilities::ToolConfig) -> Result<ConfigureResult> {
        let artifacts = vec![];
        let messages = vec!["Skills compiler capability configured.".to_string()];
        Ok(ConfigureResult {
            success: true,
            artifacts,
            messages,
        })
    }

    fn runtime_bindings(&self) -> Vec<EnvBinding> {
        if Self::check_compiler_available() {
            vec![
                EnvBinding {
                    key: "GRANITE_COMPILER_ENABLED".to_string(),
                    value: "true".to_string(),
                },
                EnvBinding {
                    key: "GRANITE_COMPILER_SKILL_PATH".to_string(),
                    value: "/usr/local/bin/mellea".to_string(),
                },
            ]
        } else {
            vec![
                EnvBinding {
                    key: "GRANITE_COMPILER_ENABLED".to_string(),
                    value: "false".to_string(),
                },
            ]
        }
    }
}

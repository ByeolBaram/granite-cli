use anyhow::Result;

use crate::capabilities::{Capability, Dependency, EnvBinding, Factory, LaunchContext};

/// Visual analysis capability using Granite Vision models.
pub struct VisionCapability {
    id: String,
    name: String,
    description: String,
    dependencies: Vec<Dependency>,
}

impl VisionCapability {
    pub fn new() -> Self {
        Self {
            id: "vision".to_string(),
            name: "Visual Analysis".to_string(),
            description: "Enable visual analysis capabilities using Granite Vision models for image understanding.".to_string(),
            dependencies: vec![
                Dependency::Model {
                    id: "granite-vision-3.1-8b".to_string(),
                    required: true,
                },
            ],
        }
    }
}

impl Default for VisionCapability {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Capability for VisionCapability {
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
        println!("Checking for Granite Vision model...");

        let has_model = factory.resolve_model("granite-vision-3.1-8b").await.is_ok();
        if !has_model {
            println!(
                "\nGranite Vision model is not configured. Run:\n  granite-cli model setup granite-vision-3.1-8b\n\n\
                 Vision capability requires the Granite Vision 3.1 8B model."
            );
        } else {
            println!("Granite Vision model is configured.");
        }

        Ok(())
    }

    async fn on_pre_launch(&self, _context: &LaunchContext) -> Result<()> {
        println!("Vision capability: ensuring vision model runtime is available (stub).");
        Ok(())
    }

    async fn on_shutdown(&self, _context: &LaunchContext) -> Result<()> {
        println!("Vision capability: stopping vision runtime (stub).");
        Ok(())
    }

    fn runtime_bindings(&self) -> Vec<EnvBinding> {
        vec![
            EnvBinding {
                key: "GRANITE_VISION_ENABLED".to_string(),
                value: "true".to_string(),
            },
            EnvBinding {
                key: "GRANITE_VISION_MODEL".to_string(),
                value: "granite-vision-3.1-8b".to_string(),
            },
        ]
    }
}

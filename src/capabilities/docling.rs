use anyhow::Result;

use crate::capabilities::{Capability, ConfigureResult, Dependency, EnvBinding, Factory};

/// Docling document conversion capability.
/// Integrates IBM's Docling library for converting PDF, DOCX, PPTX, XLSX to markdown.
/// Note: Docling is invoked by the tool via skills, not directly by granite-cli.
pub struct DoclingCapability {
    id: String,
    name: String,
    description: String,
    dependencies: Vec<Dependency>,
    _enabled: bool,
}

impl DoclingCapability {
    pub fn new() -> Self {
        Self {
            id: "docling".to_string(),
            name: "Document Conversion".to_string(),
            description: "Convert various document formats (PDF, DOCX, PPTX, XLSX) to markdown using IBM Docling.".to_string(),
            dependencies: vec![
                Dependency::ExternalTool {
                    name: "docling".to_string(),
                    check_command: "python -c \"import docling\"".to_string(),
                },
            ],
            _enabled: true,
        }
    }

    fn check_docling_available() -> bool {
        let result = std::process::Command::new("python3")
            .args(["-c", "import docling"])
            .output();

        match result {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

impl Default for DoclingCapability {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Capability for DoclingCapability {
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

    async fn on_setup(&self, _factory: &dyn Factory) -> Result<()> {
        if !Self::check_docling_available() {
            println!(
                "\nDocling is not installed. Install with:\n  pip install docling\n\n\
                 Docling will be available once installed."
            );
        } else {
            println!("Docling is available.");
        }
        Ok(())
    }

    async fn on_configure(&self, _tool: &crate::capabilities::ToolConfig) -> Result<ConfigureResult> {
        let artifacts = vec![];
        let messages = vec!["Docling capability configured.".to_string()];
        Ok(ConfigureResult {
            success: true,
            artifacts,
            messages,
        })
    }

    fn runtime_bindings(&self) -> Vec<EnvBinding> {
        if Self::check_docling_available() {
            vec![
                EnvBinding {
                    key: "GRANITE_DOCLING_ENABLED".to_string(),
                    value: "true".to_string(),
                },
                EnvBinding {
                    key: "GRANITE_DOCLING_SKILL_PATH".to_string(),
                    value: "/usr/local/bin/docling".to_string(),
                },
            ]
        } else {
            vec![
                EnvBinding {
                    key: "GRANITE_DOCLING_ENABLED".to_string(),
                    value: "false".to_string(),
                },
            ]
        }
    }
}

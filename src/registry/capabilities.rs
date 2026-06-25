use crate::registry::Registry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub dependencies: Vec<Dependency>,
    pub hooks: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Dependency {
    Model { id: String, required: bool },
    Provider { id: String, required: bool },
    ExternalTool { name: String, check_command: String },
    Capability { id: String, required: bool },
}

impl std::fmt::Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dependency::Model { id, required } => {
                write!(f, "Model: {}{}", id, if *required { " (required)" } else { "" })
            }
            Dependency::Provider { id, required } => {
                write!(f, "Provider: {}{}", id, if *required { " (required)" } else { "" })
            }
            Dependency::ExternalTool { name, check_command } => {
                write!(f, "ExternalTool: {} ({})", name, check_command)
            }
            Dependency::Capability { id, required } => {
                write!(f, "Capability: {}{}", id, if *required { " (required)" } else { "" })
            }
        }
    }
}

impl std::fmt::Display for CapabilityDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} v{} - {}", self.id, self.version, self.description)
    }
}

pub struct CapabilityRegistry {
    capabilities: Vec<CapabilityDefinition>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        let capabilities = Self::bundled_capabilities();
        Self { capabilities }
    }

    fn bundled_capabilities() -> Vec<CapabilityDefinition> {
        vec![
            CapabilityDefinition {
                id: "docling".to_string(),
                name: "Document Conversion".to_string(),
                description: "Convert various document formats (PDF, DOCX, PPTX, XLSX) to markdown using IBM Docling.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![
                    Dependency::ExternalTool {
                        name: "docling".to_string(),
                        check_command: "python -c \"import docling\"".to_string(),
                    },
                ],
                hooks: vec!["on_setup".to_string(), "runtime_bindings".to_string()],
                tags: vec!["document".to_string(), "conversion".to_string(), "markdown".to_string()],
            },
            CapabilityDefinition {
                id: "vision".to_string(),
                name: "Visual Analysis".to_string(),
                description: "Enable visual analysis capabilities using Granite Vision models for image understanding.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![
                    Dependency::Model {
                        id: "granite-vision-3.1-8b".to_string(),
                        required: true,
                    },
                ],
                hooks: vec!["on_setup".to_string(), "on_pre_launch".to_string(), "on_shutdown".to_string(), "runtime_bindings".to_string()],
                tags: vec!["vision".to_string(), "image".to_string(), "multimodal".to_string()],
            },
            CapabilityDefinition {
                id: "speech".to_string(),
                name: "Audio Transcription".to_string(),
                description: "Transcribe and translate audio files using Granite Speech models.".to_string(),
                version: "0.1.0".to_string(),
                dependencies: vec![
                    Dependency::Model {
                        id: "granite-speech-1.0".to_string(),
                        required: true,
                    },
                ],
                hooks: vec!["on_setup".to_string(), "runtime_bindings".to_string()],
                tags: vec!["speech".to_string(), "audio".to_string(), "transcription".to_string(), "translation".to_string()],
            },
            CapabilityDefinition {
                id: "compiler".to_string(),
                name: "Skills Compiler".to_string(),
                description: "Mellea skills compiler - compile raw skills into tool-specific formats with Granite Guardian integration.".to_string(),
                version: "0.1.0".to_string(),
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
                hooks: vec!["on_setup".to_string(), "on_configure".to_string(), "runtime_bindings".to_string()],
                tags: vec!["compiler".to_string(), "skills".to_string(), "mellea".to_string()],
            },
        ]
    }
}

impl Registry<CapabilityDefinition> for CapabilityRegistry {
    fn list(&self) -> Vec<&CapabilityDefinition> {
        self.capabilities.iter().collect()
    }

    fn get(&self, id: &str) -> Option<&CapabilityDefinition> {
        self.capabilities.iter().find(|c| c.id == id)
    }

    fn search(&self, query: &str) -> Vec<&CapabilityDefinition> {
        let query_lower = query.to_lowercase();
        self.capabilities
            .iter()
            .filter(|c| {
                c.id.to_lowercase().contains(&query_lower)
                    || c.name.to_lowercase().contains(&query_lower)
                    || c.description.to_lowercase().contains(&query_lower)
                    || c.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    #[test]
    fn test_capability_registry_has_bundled_capabilities() {
        let registry = CapabilityRegistry::new();
        let capabilities = registry.list();
        assert_eq!(capabilities.len(), 4);
    }

    #[test]
    fn test_capability_registry_get_by_id() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("docling");
        assert!(cap.is_some());
        let cap = cap.unwrap();
        assert_eq!(cap.name, "Document Conversion");
    }

    #[test]
    fn test_capability_registry_get_vision() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("vision");
        assert!(cap.is_some());
        let cap = cap.unwrap();
        assert!(cap.dependencies.iter().any(|d| matches!(d, Dependency::Model { id, required: true } if id == "granite-vision-3.1-8b")));
    }

    #[test]
    fn test_capability_registry_get_speech() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("speech");
        assert!(cap.is_some());
        let cap = cap.unwrap();
        assert!(cap.dependencies.iter().any(|d| matches!(d, Dependency::Model { id, required: true } if id == "granite-speech-1.0")));
    }

    #[test]
    fn test_capability_registry_get_compiler() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("compiler");
        assert!(cap.is_some());
        let cap = cap.unwrap();
        assert!(cap.dependencies.iter().any(|d| matches!(d, Dependency::ExternalTool { name, .. } if name == "mellea-compiler")));
    }

    #[test]
    fn test_capability_registry_get_not_found() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("nonexistent-capability");
        assert!(cap.is_none());
    }

    #[test]
    fn test_capability_registry_search() {
        let registry = CapabilityRegistry::new();
        let results = registry.search("document");
        assert!(results.len() > 0);
    }

    #[test]
    fn test_capability_registry_search_tag() {
        let registry = CapabilityRegistry::new();
        let results = registry.search("skills");
        assert!(results.len() > 0);
    }

    #[test]
    fn test_capability_dependencies() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("docling").unwrap();
        assert_eq!(cap.dependencies.len(), 1);
        match &cap.dependencies[0] {
            Dependency::ExternalTool { name, check_command } => {
                assert_eq!(name, "docling");
                assert!(check_command.contains("import docling"));
            }
            _ => panic!("Expected ExternalTool dependency"),
        }
    }

    #[test]
    fn test_capability_hooks() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("vision").unwrap();
        assert!(cap.hooks.contains(&"on_setup".to_string()));
        assert!(cap.hooks.contains(&"on_pre_launch".to_string()));
        assert!(cap.hooks.contains(&"on_shutdown".to_string()));
    }

    #[test]
    fn test_capability_display() {
        let registry = CapabilityRegistry::new();
        let cap = registry.get("docling").unwrap();
        let display = cap.to_string();
        assert!(display.contains("docling"));
        assert!(display.contains("Convert various document formats"));
    }
}

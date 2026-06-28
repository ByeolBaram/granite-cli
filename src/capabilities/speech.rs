use anyhow::Result;

use crate::capabilities::{Capability, Dependency, EnvBinding, Factory};

/// Audio transcription/translation capability using Granite Speech models.
pub struct SpeechCapability {
    id: String,
    name: String,
    description: String,
    dependencies: Vec<Dependency>,
}

impl SpeechCapability {
    pub fn new() -> Self {
        Self {
            id: "speech".to_string(),
            name: "Audio Transcription".to_string(),
            description: "Transcribe and translate audio files using Granite Speech models.".to_string(),
            dependencies: vec![
                Dependency::Model {
                    id: "granite-speech-1.0".to_string(),
                    required: true,
                },
            ],
        }
    }
}

impl Default for SpeechCapability {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Capability for SpeechCapability {
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
        println!("Checking for Granite Speech model...");

        let has_model = factory.resolve_model("granite-speech-1.0").await.is_ok();
        if !has_model {
            println!(
                "\nGranite Speech model is not configured. Run:\n  granite-cli model setup granite-speech-1.0\n\n\
                 Speech capability requires the Granite Speech 1.0 model."
            );
        } else {
            println!("Granite Speech model is configured.");
        }

        Ok(())
    }

    fn runtime_bindings(&self) -> Vec<EnvBinding> {
        vec![
            EnvBinding {
                key: "GRANITE_SPEECH_ENABLED".to_string(),
                value: "true".to_string(),
            },
            EnvBinding {
                key: "GRANITE_SPEECH_MODEL".to_string(),
                value: "granite-speech-1.0".to_string(),
            },
            EnvBinding {
                key: "GRANITE_SPEECH_LANGUAGES".to_string(),
                value: "en,es,fr,de,zh".to_string(),
            },
        ]
    }
}

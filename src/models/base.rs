use crate::registry::ConfigConstructable;
use serde::{Deserialize, Serialize};

/*-- ModelFunction Enum ------------------------------------------------------*/

/// Functional capabilities that models can provide
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelFunction {

    /*-- Chat Functions --*/

    /// Text-based conversational interaction
    Chat,
    /// Tool inputs and invocations
    ToolCalling,
    /// Chain-of-thought reasoning
    Thinking,
    /// Visual content analysis and understanding
    ImageUnderstanding,
    /// Detect harms
    Guardian,

    /*-- Embedding Functions --*/

    /// Vector representation generation for text
    Embeddings,

    /*-- Audio Functions --*/

    /// Audio-to-text transcription
    Transcription,
    /// Audio translation
    Translation,
    /// Speaker attribution in audio
    SpeakerAttribution,
    /// Keyword biasing in audio
    KeywordBiasing,
}

impl std::fmt::Display for ModelFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelFunction::Chat => write!(f, "Chat"),
            ModelFunction::ToolCalling => write!(f, "ToolCalling"),
            ModelFunction::Thinking => write!(f, "Thinking"),
            ModelFunction::ImageUnderstanding => write!(f, "Image Understanding"),
            ModelFunction::Guardian => write!(f, "Guardian"),
            ModelFunction::Embeddings => write!(f, "Embeddings"),
            ModelFunction::Transcription => write!(f, "Transcription"),
            ModelFunction::Translation => write!(f, "Translation"),
            ModelFunction::SpeakerAttribution => write!(f, "Speaker Attribution"),
            ModelFunction::KeywordBiasing => write!(f, "Keyword Biasing"),
        }
    }
}

/*-- Model Trait -------------------------------------------------------------*/

/// Core trait for model implementations.
/// All models must implement this trait along with ConfigConstructable.
pub trait Model: ConfigConstructable {

    /// Get the model family name
    fn family(&self) -> &str;

    /// Get the model version
    fn version(&self) -> &str;

    /// Get the model size in parameters
    fn size(&self) -> u64;

    /// Get the context length
    fn context_length(&self) -> u64;

    /// Get the model type
    fn model_type(&self) -> &ModelType;

    /// Get the HuggingFace repository
    fn huggingface_repo(&self) -> &str;

    /// Get available variants
    fn variants(&self) -> &[ModelVariant];

    /// Get description if available
    fn description(&self) -> Option<&str>;

    /// Get tags
    fn tags(&self) -> &[String];

    /// Model functions this model supports (OR logic - any of these)
    fn supported_functions(&self) -> &[ModelFunction];
}

/*-- Metadata Types ----------------------------------------------------------*/

/// Metadata describing a model implementation.
/// This is what the factory returns when querying model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub family: String,
    pub version: String,
    pub size: u64,
    pub context_length: u64,
    pub model_type: ModelType,
    pub huggingface_repo: String,
    pub variants: Vec<ModelVariant>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub supported_functions: Vec<ModelFunction>,
}

impl std::fmt::Display for ModelMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}) - {}B params, {} context, Type: {}",
            self.family,
            self.size / 1_000_000_000,
            self.context_length,
            self.model_type
        )
    }
}

/*-- Supporting Types --------------------------------------------------------*/

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelType {
    Text,
    Vision,
    Speech,
    Embedding,
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::Text => write!(f, "Text"),
            ModelType::Vision => write!(f, "Vision"),
            ModelType::Speech => write!(f, "Speech"),
            ModelType::Embedding => write!(f, "Embedding"),
        }
    }
}

impl From<&str> for ModelType {
    fn from(s: &str) -> Self {
        match s {
            "Text" => ModelType::Text,
            "Vision" => ModelType::Vision,
            "Speech" => ModelType::Speech,
            "Embedding" => ModelType::Embedding,
            _ => panic!("Unknown model type: {}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVariant {
    pub format: String,
    pub precision: String,
    pub size_gb: f64,
    pub url: String,
}

/*-- Factory Definition ------------------------------------------------------*/

use crate::define_factory;

define_factory!(Model, ModelMetadata, ModelFactory);

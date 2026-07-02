use crate::registry::ConfigConstructable;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::marker::PhantomData;

/*-- Model Trait -------------------------------------------------------------*/

/// Core trait for model implementations.
/// All models must implement this trait along with ConfigConstructable.
pub trait Model: ConfigConstructable {
    /// Get the unique identifier for this model
    fn id(&self) -> &str;

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

    /// Get required provider capabilities
    fn required_provider_capabilities(&self) -> &[String];

    /// Get available variants
    fn variants(&self) -> &[ModelVariant];

    /// Get description if available
    fn description(&self) -> Option<&str>;

    /// Get tags
    fn tags(&self) -> &[String];
}

/*-- Configuration Types -----------------------------------------------------*/

/// Configuration for constructing a Model instance.
/// This is loaded from resources/models.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub family: String,
    pub version: String,
    pub size: u64,
    pub context_length: u64,
    pub model_type: ModelType,
    pub huggingface_repo: String,
    pub required_provider_capabilities: Vec<String>,
    pub variants: Vec<ModelVariant>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/*-- Metadata Types ----------------------------------------------------------*/

/// Metadata describing a model implementation.
/// This is what the factory returns when querying model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub id: String,
    pub family: String,
    pub version: String,
    pub size: u64,
    pub context_length: u64,
    pub model_type: ModelType,
    pub huggingface_repo: String,
    pub required_provider_capabilities: Vec<String>,
    pub variants: Vec<ModelVariant>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl std::fmt::Display for ModelMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) - {}B params, {} context, Type: {}",
            self.id,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVariant {
    pub format: String,
    pub precision: String,
    pub size_gb: f64,
    pub huggingface_path: String,
}

/*-- Factory Definition ------------------------------------------------------*/

use crate::define_factory;

define_factory!(
    Model,
    ModelConfig,
    ModelMetadata,
    ModelFactory
);
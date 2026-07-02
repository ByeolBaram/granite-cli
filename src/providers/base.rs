use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/*-- Provider Trait ----------------------------------------------------------*/

/// Core trait for provider implementations.
/// All providers must implement this trait along with ConfigConstructable.
#[async_trait]
pub trait Provider: ConfigConstructable + Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn api_capabilities(&self) -> Vec<ApiSurface>;

    // Model support
    fn supported_formats(&self) -> Vec<ModelFormat>;
    fn supported_precisions(&self) -> Vec<Precision>;
    fn can_run_model(&self, _variant_format: &str, _variant_precision: &str) -> bool {
        true
    }

    // Inference
    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;
    async fn stream_chat(
        &self,
        request: ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<ChatChunk, ProviderError>> + Send + Unpin>, ProviderError>;

    // Health
    async fn health_check(&self) -> Result<HealthStatus, ProviderError>;
}

/*-- Configuration Types -----------------------------------------------------*/

/// Configuration for constructing a Provider instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider_type: ProviderType,
    pub default_endpoint: String,
    pub api_capabilities: Vec<ApiSurface>,
    pub supported_formats: Vec<ModelFormat>,
    pub supported_precisions: Vec<Precision>,
    pub authentication: Vec<AuthType>,
    pub tags: Vec<String>,
}

/*-- Metadata Types ----------------------------------------------------------*/

/// Metadata describing a provider implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider_type: ProviderType,
    pub default_endpoint: String,
    pub api_capabilities: Vec<ApiSurface>,
    pub supported_formats: Vec<ModelFormat>,
    pub supported_precisions: Vec<Precision>,
    pub authentication: Vec<AuthType>,
    pub tags: Vec<String>,
}

impl std::fmt::Display for ProviderMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}): {} - {}",
            self.id, self.provider_type, self.name, self.description
        )
    }
}

/*-- Supporting Types --------------------------------------------------------*/

/// API surfaces that providers can support.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiSurface {
    OpenAIChat,         // /v1/chat/completions
    OllamaChat,         // /api/chat
    AnthropicMessages,  // /v1/messages
}

impl std::fmt::Display for ApiSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiSurface::OpenAIChat => write!(f, "OpenAI Chat (/v1/chat/completions)"),
            ApiSurface::OllamaChat => write!(f, "Ollama Chat (/api/chat)"),
            ApiSurface::AnthropicMessages => write!(f, "Anthropic Messages (/v1/messages)"),
        }
    }
}

/// Model formats that providers can serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum ModelFormat {
    Safetensors,
    GGUF,
    ONNX,
}

impl std::fmt::Display for ModelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelFormat::Safetensors => write!(f, "safetensors"),
            ModelFormat::GGUF => write!(f, "GGUF"),
            ModelFormat::ONNX => write!(f, "ONNX"),
        }
    }
}

/// Model precision/quantization levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum Precision {
    BF16,
    FP16,
    FP8,
    Q8_0,
    Q4_K_M,
    Q5_K_M,
    Q3_K_M,
}

impl std::fmt::Display for Precision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Precision::BF16 => write!(f, "BF16"),
            Precision::FP16 => write!(f, "FP16"),
            Precision::FP8 => write!(f, "FP8"),
            Precision::Q8_0 => write!(f, "Q8_0"),
            Precision::Q4_K_M => write!(f, "Q4_K_M"),
            Precision::Q5_K_M => write!(f, "Q5_K_M"),
            Precision::Q3_K_M => write!(f, "Q3_K_M"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Hosted,
    Local,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Hosted => write!(f, "Hosted"),
            ProviderType::Local => write!(f, "Local"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthType {
    ApiKey,
    BearerToken,
    None,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::ApiKey => write!(f, "API Key"),
            AuthType::BearerToken => write!(f, "Bearer Token"),
            AuthType::None => write!(f, "None"),
        }
    }
}

/// Health status from a provider health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub latency: Duration,
    pub error: Option<String>,
}

/// A chat message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::System => write!(f, "system"),
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
        }
    }
}

/// Chat request sent to a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stop_sequences: Option<Vec<String>>,
    pub stream: bool,
}

impl Default for ChatRequest {
    fn default() -> Self {
        Self {
            model: String::new(),
            messages: vec![],
            temperature: Some(0.7),
            max_tokens: None,
            stop_sequences: None,
            stream: false,
        }
    }
}

/// Chat response from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Option<UsageInfo>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A chunk from a streaming response.
#[derive(Debug, Clone)]
pub struct ChatChunk {
    pub content: String,
    pub finish_reason: Option<String>,
}

/// Errors specific to provider operations.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Provider error: {0}")]
    Other(String),
}

/*-- Factory Definition ------------------------------------------------------*/

use crate::define_factory;

define_factory!(
    Provider,
    ProviderConfig,
    ProviderMetadata,
    ProviderFactory
);

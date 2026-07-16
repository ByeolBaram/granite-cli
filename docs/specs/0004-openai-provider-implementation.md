# OpenAI-Compatible Provider Implementation Plan (Final)

## Overview

This document outlines the implementation plan for adding an OpenAI-compatible provider to the Granite CLI. The provider will support OpenAI API endpoints for chat completions, embeddings, and audio transcriptions, enabling Granite models to be accessed through OpenAI-compatible interfaces.

## Architectural Foundation: Three-Layer Model

### The Three Layers

This implementation introduces a clear three-layer architecture that separates concerns:

```
Layer 1: ModelFunction (What models CAN DO)
   ↓
Layer 2: ApiType + ApiEndpoint (How to ACCESS functions)
   ↓
Layer 3: Provider Instance (Concrete MAPPINGS at runtime)
```

#### Layer 1: ModelFunction (Logical Capabilities)

**What it represents:** The functional capabilities that models provide, independent of any specific API protocol.

**Examples:**
- `Chat` - Text-based conversational interaction
- `Embeddings` - Vector representation generation
- `Transcription` - Audio-to-text conversion
- `ImageUnderstanding` - Visual content analysis

**Used by:** Models declare which functions they support (OR logic - model supports ANY of these)

**Key insight:** A model that supports `Chat` doesn't care whether it's accessed via OpenAI's `/v1/chat/completions` or Ollama's `/api/chat` - it just provides the chat function.

#### Layer 2: ApiType + ApiEndpoint (Protocol Mappings)

**What it represents:** The specific API protocols and their endpoints that can provide access to model functions.

**ApiType (Protocol Family):**
- `OpenAI` - OpenAI-compatible protocol
- `Ollama` - Ollama protocol
- `Anthropic` - Anthropic protocol

**ApiEndpoint (Concrete Endpoint):**
- `OpenAIChat` (`/v1/chat/completions`) → provides `Chat` function
- `OpenAIEmbeddings` (`/v1/embeddings`) → provides `Embeddings` function
- `OpenAIAudioTranscription` (`/v1/audio/transcriptions`) → provides `Transcription` function
- `OllamaChat` (`/api/chat`) → provides `Chat` function
- `OllamaEmbeddings` (`/api/embeddings`) → provides `Embeddings` function

**Used by:**
- ProviderMetadata declares which ApiTypes the provider implementation supports (AND logic)
- ApiEndpoints map to ModelFunctions they can serve

#### Layer 3: Provider Instance (Runtime Configuration)

**What it represents:** A configured, running provider that maps model functions to concrete API endpoints it actually supports.

**Example mapping:**
```
vLLM instance at localhost:8080:
  Chat → OpenAI /v1/chat/completions
  Embeddings → OpenAI /v1/embeddings
  (Transcription not supported by this instance)
```

**Used by:** Tool configuration to determine if a provider can serve a model for a specific function.

### Dependency Resolution Example

When configuring a Tool with a Capability:

1. **Tool declares:** "I need a provider that speaks Anthropic OR OpenAI"
2. **Capability requires:** A model that supports `Chat` function
3. **Model declares:** "I support `Chat` and `Embeddings` functions"
4. **Provider instance reports:** "I support `Chat` via OpenAI `/v1/chat/completions`"
5. **Resolution:** ✅ Provider can serve this model for this tool

## Current State Analysis

### Existing Provider Infrastructure

From `src/providers/base.rs`:
- `Provider` trait defines the core interface
- `ProviderMetadata` describes provider capabilities
- Current `ApiSurface` enum mixes layers (needs refactoring)
- Factory pattern via `define_factory!` macro
- Health check support

### Models Supporting Functions

From `resources/models.yaml`:
- All text models support `Chat` function
- Speech models support `Transcription` function
- Some models support `Embeddings` function
- Vision models support `ImageUnderstanding` function

## Implementation Plan

### Step 1: Define ModelFunction Enum

**File:** `src/models/base.rs`

Create the foundational enum for model capabilities:

```rust
/// Functional capabilities that models can provide
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelFunction {

    // Chat and sub-functions //

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

    // Embedding functions //

    /// Vector representation generation for text
    Embeddings,

    // Audio functions //

    /// Audio-to-text transcription
    Transcription,
    Translation,
    SpeakerAttribution,
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
```

### Step 2: Define ApiType and ApiEndpoint Enums

**File:** `src/providers/base.rs`

Create the protocol layer enums:

```rust
/// API protocol families that providers can implement
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiType {
    OpenAI,      // OpenAI-compatible API protocol
    Ollama,      // Ollama API protocol
    Anthropic,   // Anthropic API protocol
}

impl std::fmt::Display for ApiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiType::OpenAI => write!(f, "OpenAI"),
            ApiType::Ollama => write!(f, "Ollama"),
            ApiType::Anthropic => write!(f, "Anthropic"),
        }
    }
}

/// Specific API endpoints within an API family
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiEndpoint {
    // OpenAI endpoints
    OpenAIChat,               // /v1/chat/completions
    OpenAIEmbeddings,         // /v1/embeddings
    OpenAIAudioTranscription, // /v1/audio/transcriptions

    // Ollama endpoints
    OllamaChat,              // /api/chat
    OllamaEmbeddings,        // /api/embeddings

    // Anthropic endpoints
    AnthropicMessages,       // /v1/messages
}

impl ApiEndpoint {
    /// Returns the API type this endpoint belongs to
    pub fn api_type(&self) -> ApiType {
        match self {
            ApiEndpoint::OpenAIChat
            | ApiEndpoint::OpenAIEmbeddings
            | ApiEndpoint::OpenAIAudioTranscription => ApiType::OpenAI,

            ApiEndpoint::OllamaChat
            | ApiEndpoint::OllamaEmbeddings => ApiType::Ollama,

            ApiEndpoint::AnthropicMessages => ApiType::Anthropic,
        }
    }

    /// Returns the endpoint path
    pub fn path(&self) -> &'static str {
        match self {
            ApiEndpoint::OpenAIChat => "/v1/chat/completions",
            ApiEndpoint::OpenAIEmbeddings => "/v1/embeddings",
            ApiEndpoint::OpenAIAudioTranscription => "/v1/audio/transcriptions",
            ApiEndpoint::OllamaChat => "/api/chat",
            ApiEndpoint::OllamaEmbeddings => "/api/embeddings",
            ApiEndpoint::AnthropicMessages => "/v1/messages",
        }
    }

    /// Returns the model function this endpoint provides
    pub fn provides_function(&self) -> Vec<ModelFunction> {
        match self {
            ApiEndpoint::OpenAIChat
            | ApiEndpoint::OllamaChat
            | ApiEndpoint::AnthropicMessages => vec![
                ModelFunction::Chat,
                ModelFunction::ToolCalling,
                ModelFunction::Thinking,
                ModelFunction::ImageUnderstanding,
                ModelFunction::Guardian,
            ],

            ApiEndpoint::OpenAIEmbeddings
            | ApiEndpoint::OllamaEmbeddings => vec![
                ModelFunction::Embeddings,
            ],

            ApiEndpoint::OpenAIAudioTranscription => vec![
                ModelFunction::Transcription,
                ModelFunction::Translation,
                ModelFunction::SpeakerAttribution,
                ModelFunction::KeywordBiasing,
            ],
        }
    }
}

impl std::fmt::Display for ApiEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} ({})",
            self.api_type(),
            self.path(),
            self.provides_function()
        )
    }
}
```

**Key Design:**
- `api_type()` creates the hierarchical relationship to protocol family
- `path()` provides the actual endpoint path
- `provides_function()` maps endpoint to the model function it serves

### Step 3: Update Provider Trait and Metadata

**File:** `src/providers/base.rs`

Update the provider infrastructure:

```rust
/// Core trait for provider implementations.
#[async_trait]
pub trait Provider: ConfigConstructable + Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    /// Returns the mapping of model functions to API endpoints this provider instance supports
    /// This is runtime/configuration-specific
    fn function_endpoints(&self) -> HashMap<ModelFunction, Vec<ApiEndpoint>>;

    // Model support
    fn supported_formats(&self) -> Vec<ModelFormat>;
    fn supported_precisions(&self) -> Vec<String>;
    fn can_run_model(&self, _variant_format: &str, _variant_precision: &str) -> bool {
        true
    }

    // Health
    async fn health_check(&self) -> Result<HealthStatus, ProviderError>;

    /// Helper: Check if this provider can serve a specific function
    fn supports_function(&self, function: &ModelFunction) -> bool {
        self.function_endpoints().contains_key(function)
    }

    /// Helper: Get endpoints for a specific function
    fn endpoints_for_function(&self, function: &ModelFunction) -> Vec<ApiEndpoint> {
        self.function_endpoints()
            .get(function)
            .cloned()
            .unwrap_or_default()
    }
}

/// Metadata describing a provider implementation (type-level).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider_type: ProviderType,
    pub default_url: String,

    /// API types this provider implementation supports (AND logic)
    pub supported_api_types: Vec<ApiType>,

    /// Default function-to-endpoint mappings for this provider type
    pub default_function_endpoints: HashMap<ModelFunction, Vec<ApiEndpoint>>,

    pub supported_formats: Vec<ModelFormat>,
    pub supported_precisions: Vec<String>,
    pub authentication: Vec<AuthType>,
    pub tags: Vec<String>,
}
```

**Key Changes:**
- `function_endpoints()` returns the runtime mapping of functions to endpoints
- Helper methods for checking function support
- Metadata includes default function-to-endpoint mappings

### Step 4: Update Model Trait and Metadata

**File:** `src/models/base.rs`

Update models to declare functions they support:

```rust
pub trait Model: ConfigConstructable + Send + Sync {
    fn id(&self) -> &str;
    fn family(&self) -> &str;
    fn version(&self) -> &str;
    fn size(&self) -> u64;
    fn context_length(&self) -> u64;
    fn model_type(&self) -> &ModelType;
    fn huggingface_repo(&self) -> &str;

    /// Model functions this model supports (OR logic - any of these)
    fn supported_functions(&self) -> &[ModelFunction];

    fn variants(&self) -> &[ModelVariant];
    fn description(&self) -> Option<&str>;
    fn tags(&self) -> &[String];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub id: String,
    pub family: String,
    pub version: String,
    pub size: u64,
    pub context_length: u64,
    pub model_type: ModelType,
    pub huggingface_repo: String,

    /// Model functions this model supports
    pub supported_functions: Vec<ModelFunction>,

    pub variants: Vec<ModelVariant>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}
```

### Step 5: Create OpenAI Provider Implementation

**File:** `src/providers/openai.rs`

```rust
use crate::models::base::ModelFunction;
use crate::providers::base::{
    ApiEndpoint, ApiType, AuthType, HealthStatus, ModelFormat, Provider, ProviderError,
    ProviderMetadata, ProviderType, HasProviderMetadata,
};
use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/*-- OpenAI Provider Configuration -------------------------------------------*/

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIProviderConfig {
    /// Base URL for the OpenAI-compatible API
    pub base_url: String,

    /// API key for authentication (optional for local providers)
    pub api_key: Option<String>,

    /// Timeout for health checks in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Whether to verify SSL certificates
    #[serde(default = "default_verify_ssl")]
    pub verify_ssl: bool,

    /// Endpoint to use for health checks
    #[serde(default = "default_health_endpoint")]
    pub health_check_endpoint: String,

    /// Specific function-to-endpoint mappings this instance supports
    /// If None, will use default OpenAI mappings
    pub function_endpoints: Option<HashMap<ModelFunction, Vec<ApiEndpoint>>>,
}

fn default_timeout() -> u64 {
    10
}

fn default_verify_ssl() -> bool {
    true
}

fn default_health_endpoint() -> String {
    "/v1/models".to_string()
}

impl Default for OpenAIProviderConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            api_key: None,
            timeout_secs: 10,
            verify_ssl: true,
            health_check_endpoint: "/v1/models".to_string(),
            function_endpoints: None,
        }
    }
}

/*-- OpenAI Provider Implementation ------------------------------------------*/

pub struct OpenAIProvider {
    config: OpenAIProviderConfig,
    client: reqwest::Client,
    function_endpoints: HashMap<ModelFunction, Vec<ApiEndpoint>>,
}

impl OpenAIProvider {
    fn default_function_endpoints() -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
        let mut map = HashMap::new();
        map.insert(ModelFunction::Chat, vec![ApiEndpoint::OpenAIChat]);
        map.insert(ModelFunction::Embeddings, vec![ApiEndpoint::OpenAIEmbeddings]);
        map.insert(ModelFunction::Transcription, vec![ApiEndpoint::OpenAIAudioTranscription]);
        map
    }
}

impl ConfigConstructable for OpenAIProvider {
    fn new(cfg: &serde_json::Value) -> Self {
        let config: OpenAIProviderConfig = serde_json::from_value(cfg.clone())
            .unwrap_or_default();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .danger_accept_invalid_certs(!config.verify_ssl)
            .build()
            .expect("Failed to create HTTP client");

        // Determine function-to-endpoint mappings
        let function_endpoints = config.function_endpoints.clone()
            .unwrap_or_else(Self::default_function_endpoints);

        Self { config, client, function_endpoints }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn id(&self) -> &str {
        "openai-compatible"
    }

    fn name(&self) -> &str {
        "OpenAI Compatible Provider"
    }

    fn function_endpoints(&self) -> HashMap<ModelFunction, Vec<ApiEndpoint>> {
        self.function_endpoints.clone()
    }

    fn supported_formats(&self) -> Vec<ModelFormat> {
        vec![
            ModelFormat::Safetensors,
            ModelFormat::GGUF,
        ]
    }

    fn supported_precisions(&self) -> Vec<String> {
        vec![
            "fp16".to_string(),
            "fp32".to_string(),
            "int8".to_string(),
            "int4".to_string(),
        ]
    }

    fn can_run_model(&self, _variant_format: &str, _variant_precision: &str) -> bool {
        true
    }

    async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
        let start = Instant::now();

        let url = format!("{}{}", self.config.base_url, self.config.health_check_endpoint);

        let mut request = self.client.get(&url);

        if let Some(ref api_key) = self.config.api_key {
            request = request.bearer_auth(api_key);
        }

        match request.send().await {
            Ok(response) => {
                let latency = start.elapsed();

                if response.status().is_success() {
                    Ok(HealthStatus {
                        healthy: true,
                        latency,
                        error: None,
                    })
                } else {
                    Ok(HealthStatus {
                        healthy: false,
                        latency,
                        error: Some(format!("HTTP {}: {}",
                            response.status(),
                            response.text().await.unwrap_or_default()
                        )),
                    })
                }
            }
            Err(e) => {
                let latency = start.elapsed();
                Ok(HealthStatus {
                    healthy: false,
                    latency,
                    error: Some(format!("Connection failed: {}", e)),
                })
            }
        }
    }
}

impl HasProviderMetadata for OpenAIProvider {
    fn metadata() -> ProviderMetadata {
        let mut default_mappings = HashMap::new();
        default_mappings.insert(ModelFunction::Chat, vec![ApiEndpoint::OpenAIChat]);
        default_mappings.insert(ModelFunction::Embeddings, vec![ApiEndpoint::OpenAIEmbeddings]);
        default_mappings.insert(ModelFunction::Transcription, vec![ApiEndpoint::OpenAIAudioTranscription]);

        ProviderMetadata {
            id: "openai-compatible".to_string(),
            name: "OpenAI Compatible Provider".to_string(),
            description: "Provider for OpenAI-compatible API endpoints supporting chat, embeddings, and audio transcription".to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost:8080".to_string(),
            supported_api_types: vec![ApiType::OpenAI],
            default_function_endpoints: default_mappings,
            supported_formats: vec![
                ModelFormat::Safetensors,
                ModelFormat::GGUF,
            ],
            supported_precisions: vec![
                "fp16".to_string(),
                "fp32".to_string(),
                "int8".to_string(),
                "int4".to_string(),
            ],
            authentication: vec![
                AuthType::BearerToken,
                AuthType::None,
            ],
            tags: vec![
                "openai".to_string(),
                "compatible".to_string(),
                "local".to_string(),
            ],
        }
    }
}
```

### Step 6: Update models.yaml Format

**File:** `resources/models.yaml`

Update to use model functions:

```yaml
- id: granite-4.1-8b
  # ... other fields ...
  supported_functions:
  - Chat
  # ... rest of model definition ...

- id: granite-speech-4.1-2b
  # ... other fields ...
  supported_functions:
  - Chat
  - Transcription
  # ... rest of model definition ...

- id: granite-vision-3.3-2b-embedding
  # ... other fields ...
  supported_functions:
  - Embeddings
  - ImageUnderstanding
  # ... rest of model definition ...
```

### Step 7: Capability Matching Logic

**File:** `src/capabilities/base.rs` or new matching module

```rust
use std::collections::HashSet;

/// Check if a provider instance can serve a model for a specific function
pub fn can_provider_serve_model_function(
    provider: &dyn Provider,
    model: &dyn Model,
    function: &ModelFunction,
) -> bool {
    // Model must support the function
    if !model.supported_functions().contains(function) {
        return false;
    }

    // Provider must have endpoints for the function
    provider.supports_function(function)
}

/// Check if a provider instance can serve a model for any function
pub fn can_provider_serve_model(
    provider: &dyn Provider,
    model: &dyn Model,
) -> bool {
    let provider_functions: HashSet<_> = provider.function_endpoints().keys().collect();
    let model_functions: HashSet<_> = model.supported_functions().iter().collect();

    // Provider must support at least one function the model provides
    !provider_functions.is_disjoint(&model_functions)
}

/// Get the functions a provider can serve for a model
pub fn get_servable_functions(
    provider: &dyn Provider,
    model: &dyn Model,
) -> Vec<ModelFunction> {
    let provider_functions: HashSet<_> = provider.function_endpoints().keys().cloned().collect();
    let model_functions: HashSet<_> = model.supported_functions().iter().cloned().collect();

    provider_functions.intersection(&model_functions).cloned().collect()
}
```

### Step 8: Register Provider

**File:** `src/providers/mod.rs`

```rust
use std::sync::LazyLock;

pub static PROVIDER_REGISTRY: LazyLock<base::ProviderFactory> = LazyLock::new(|| {
    let mut factory = base::ProviderFactory::new();
    factory.register::<openai::OpenAIProvider>("openai-compatible");
    factory
});

mod base;
pub use base::{
    ApiEndpoint, ApiType, AuthType, HealthStatus, ModelFormat,
    Provider, ProviderError, ProviderMetadata, ProviderType,
};

mod openai;
pub use openai::{OpenAIProvider, OpenAIProviderConfig};
```

## Migration Path

### Phase 1: Add New Types (Non-Breaking)
1. Add `ModelFunction` enum to `src/models/base.rs`
2. Add `ApiType` and `ApiEndpoint` enums to `src/providers/base.rs`
3. Keep existing code for backward compatibility

### Phase 2: Update Infrastructure
1. Update `Provider` trait with `function_endpoints()` method
2. Update `ProviderMetadata` structure
3. Update `Model` trait with `supported_functions()` method
4. Update `ModelMetadata` structure

### Phase 3: Implement OpenAI Provider
1. Create `src/providers/openai.rs`
2. Register in `PROVIDER_REGISTRY`
3. Add capability matching logic

### Phase 4: Update Data (Breaking)
1. Update `models.yaml` format
2. Update build script to generate new format
3. Test with real models

### Phase 5: Cleanup
1. Remove old `ApiSurface` enum if it exists
2. Update all references
3. Update documentation

## Testing Strategy

### Unit Tests

1. **ModelFunction Tests**: Test Display implementation
2. **ApiEndpoint Tests**: Test `api_type()`, `path()`, and `provides_function()`
3. **Provider Construction**: Test with various configurations
4. **Capability Matching**: Test matching logic with various scenarios

### Integration Tests

1. **Health Check**: Test against mock server
2. **Function Mapping**: Test function-to-endpoint resolution
3. **Model-Provider Matching**: Test compatibility checking

### Manual Testing

1. **Local Server**: Test against vLLM or LocalAI
2. **Multiple Functions**: Test models with multiple functions
3. **Configuration**: Test various provider configurations

## Implementation Order

1. ✅ Analyze and create final plan
2. Add `ModelFunction` enum to `src/models/base.rs`
3. Add `ApiType` and `ApiEndpoint` enums to `src/providers/base.rs`
4. Update `Provider` trait and `ProviderMetadata`
5. Update `Model` trait and `ModelMetadata`
6. Create `src/providers/openai.rs`
7. Add capability matching logic
8. Register provider in `PROVIDER_REGISTRY`
9. Update `models.yaml` format
10. Update build script
11. Add tests
12. Update documentation

## Success Criteria

- [ ] `ModelFunction` enum defines logical capabilities
- [ ] `ApiType` enum defines protocol families
- [ ] `ApiEndpoint` enum with `api_type()`, `path()`, and `provides_function()` methods
- [ ] `Provider::function_endpoints()` returns function-to-endpoint mapping
- [ ] `Model::supported_functions()` returns model capabilities
- [ ] `OpenAIProvider` implements three-layer architecture
- [ ] Capability matching logic works correctly
- [ ] Provider registered in `PROVIDER_REGISTRY`
- [ ] Health check uses configurable endpoint
- [ ] Code compiles without errors

## Benefits of This Architecture

1. **Clear Separation of Concerns**: Function, protocol, and runtime layers
2. **Protocol Independence**: Models don't care about API protocols
3. **Flexible Configuration**: Providers can support subsets of functions
4. **Type Safety**: Compile-time checking of relationships
5. **Extensibility**: Easy to add new functions, protocols, and endpoints
6. **Tool Configuration**: Clear dependency resolution for tools
7. **Runtime Discovery**: Providers report actual capabilities
8. **Future-Proof**: Supports complex capability-based routing

## Example: Tool Configuration Flow

```
1. Tool "opencode" declares: Supports [OpenAI, Anthropic, Ollama] protocols
2. Capability requires: Model with Chat function
3. Model "granite-4.1-8b" declares: Supports [Chat]
4. Provider instance "local-vllm" reports:
   - Chat → OpenAI /v1/chat/completions
   - Embeddings → OpenAI /v1/embeddings
5. Resolution:
   ✅ Tool supports OpenAI protocol
   ✅ Model supports Chat function
   ✅ Provider maps Chat to OpenAI endpoint
   → Configuration valid!
```

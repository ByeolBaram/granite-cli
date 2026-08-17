//! VLM backend abstraction trait and default implementations.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use url::Url;

use crate::app_config;

// ─── Error Types ────────────────────────────────────────────────────

/// Errors from VLM operations.
#[derive(Debug, thiserror::Error)]
pub enum VlmError {
    #[error("API error: {0}")]
    ApiError(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Invalid image data: {0}")]
    InvalidImage(String),
    #[error("Image exceeds size limit: {limit} bytes")]
    ImageTooLarge { limit: u64 },
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Timeout after {0}s")]
    Timeout(u64),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
    #[error("Other: {0}")]
    Other(String),
}

// ─── Analysis Types ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisType {
    ObjectDetection,
    TextExtraction,
    SceneDescription,
    UiAnalysis,
    Custom(String),
}

impl fmt::Display for AnalysisType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalysisType::ObjectDetection => write!(f, "object-detection"),
            AnalysisType::TextExtraction => write!(f, "text-extraction"),
            AnalysisType::SceneDescription => write!(f, "scene-description"),
            AnalysisType::UiAnalysis => write!(f, "ui-analysis"),
            AnalysisType::Custom(_) => write!(f, "custom"),
        }
    }
}

// ─── Image Source ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ImageSource {
    Bytes { data: Vec<u8>, mime: String },
    File(PathBuf),
    Url(Url),
}

impl ImageSource {
    pub fn mime(&self) -> &str {
        match self {
            ImageSource::Bytes { mime, .. } => mime.as_str(),
            ImageSource::File(path) => match path.extension().and_then(|e| e.to_str()) {
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("png") => "image/png",
                Some("webp") => "image/webp",
                Some("gif") => "image/gif",
                Some("bmp") => "image/bmp",
                Some("tiff") | Some("tif") => "image/tiff",
                _ => "image/jpeg",
            },
            ImageSource::Url(url) => {
                let path = url.path();
                match path.rsplit('/').next().unwrap_or("") {
                    "jpg" | "jpeg" => "image/jpeg",
                    "png" => "image/png",
                    "webp" => "image/webp",
                    "gif" => "image/gif",
                    "bmp" => "image/bmp",
                    _ => "image/jpeg",
                }
            }
        }
    }

    pub async fn to_bytes(&self) -> Result<(Vec<u8>, String), anyhow::Error> {
        match self {
            ImageSource::Bytes { data, mime } => Ok((data.clone(), mime.clone())),
            ImageSource::File(path) => {
                let data = tokio::fs::read(path).await?;
                Ok((data, self.mime().to_string()))
            }
            ImageSource::Url(url) => {
                let resp = reqwest::get(url.clone()).await?;
                let data = resp.bytes().await?.to_vec();
                Ok((data, self.mime().to_string()))
            }
        }
    }

    pub async fn to_data_uri(&self) -> Result<String, anyhow::Error> {
        let (data, mime) = self.to_bytes().await?;
        let encoded = encode_base64(&data);
        Ok(format!("data:{};base64,{}", mime, encoded))
    }

    pub async fn to_base64(&self) -> Result<(String, String), anyhow::Error> {
        let (data, mime) = self.to_bytes().await?;
        Ok((encode_base64(&data), mime))
    }

    /// Check if image exceeds the size limit.
    pub fn check_size(&self, limit: u64) -> Result<(), VlmError> {
        match self {
            ImageSource::Bytes { data, .. } => {
                if data.len() as u64 > limit {
                    Err(VlmError::ImageTooLarge { limit })
                } else {
                    Ok(())
                }
            }
            ImageSource::File(path) => {
                let meta = std::fs::metadata(path).map_err(|e| {
                    VlmError::InvalidImage(format!("Failed to read file metadata: {}", e))
                })?;
                if meta.len() > limit {
                    Err(VlmError::ImageTooLarge { limit })
                } else {
                    Ok(())
                }
            }
            // For URLs, we can't check size without fetching; defer to backend
            ImageSource::Url(_) => Ok(()),
        }
    }
}

/// Simple base64 encoding.
fn encode_base64(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// ─── VLM Backend Trait ──────────────────────────────────────────────

#[async_trait]
pub trait VlmBackend: Send + Sync + fmt::Debug {
    async fn describe_image(
        &self,
        image: ImageSource,
        prompt: Option<String>,
    ) -> Result<String, VlmError>;

    async fn compare_images(
        &self,
        images: Vec<ImageSource>,
        prompt: Option<String>,
    ) -> Result<String, VlmError>;

    async fn ocr(
        &self,
        image: ImageSource,
        language: Option<String>,
    ) -> Result<String, VlmError>;

    async fn analyze(
        &self,
        image: ImageSource,
        analysis_type: AnalysisType,
        prompt: Option<String>,
    ) -> Result<String, VlmError>;

    async fn health(&self) -> Result<VlmHealth, VlmError>;

    async fn list_models(&self) -> Result<Vec<String>, VlmError>;
}

#[derive(Debug, Clone)]
pub struct VlmHealth {
    pub model: String,
    pub ready: bool,
    pub details: Option<String>,
}

// ─── API Types (OpenAI-Compatible) ──────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(rename = "stream")]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: ChatContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ChatContent {
    Text(String),
    Multi(Vec<ContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ContentImageUrl },
}

#[derive(Debug, Serialize)]
struct ContentImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

// ─── API Types (Ollama Native) ──────────────────────────────────────

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: &'static str,
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessageResponse,
}

#[derive(Debug, Deserialize)]
struct OllamaMessageResponse {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelEntry {
    name: String,
}

// ─── OpenAI-Compatible VLM Implementation ───────────────────────────

#[derive(Debug)]
pub struct OpenAiCompatibleVlm {
    client: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: String,
    max_image_bytes: u64,
    extra_headers: std::collections::HashMap<String, String>,
}

impl OpenAiCompatibleVlm {
    /// Create from app config.
    pub fn from_config(config: &app_config::Config) -> Result<Self> {
        let max_image_bytes = config
            .vlm
            .dos_protection
            .as_ref()
            .map(|d| d.max_image_bytes)
            .unwrap_or(config.server.dos_protection.max_image_bytes);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.vlm.timeout_seconds))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            client,
            endpoint: config.vlm.endpoint.clone(),
            model: config.vlm.model.clone(),
            api_key: config.vlm.api_key.clone(),
            max_image_bytes,
            extra_headers: config.vlm.extra_headers.clone(),
        })
    }

    /// Create with explicit parameters.
    pub fn new(
        endpoint: String,
        model: String,
        api_key: String,
        max_image_bytes: u64,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap();

        Self {
            client,
            endpoint,
            model,
            api_key,
            max_image_bytes,
            extra_headers: std::collections::HashMap::new(),
        }
    }

    fn build_endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.endpoint.trim_end_matches('/'), path)
    }

    /// Send a chat request and get the response content.
    async fn chat_text(&self, system: &str, content: ChatContent) -> Result<String, VlmError> {
        let url = self.build_endpoint("chat/completions");

        let mut builder = self.client.post(&url);
        builder = builder.header("Content-Type", "application/json");
        if !self.api_key.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", self.api_key));
        }
        for (k, v) in &self.extra_headers {
            builder = builder.header(k, v);
        }

        let messages = vec![
            ChatMessage {
                role: "system",
                content: ChatContent::Text(system.to_string()),
            },
            ChatMessage {
                role: "user",
                content,
            },
        ];

        let req = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            max_tokens: Some(4096),
        };

        let resp = builder
            .json(&req)
            .send()
            .await
            .map_err(|e| VlmError::Connection(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            if status.as_u16() == 429 {
                return Err(VlmError::RateLimited(format!("Rate limited: {}", body)));
            }
            return Err(VlmError::ApiError(format!("{}: {}", status, body)));
        }

        let body: ChatResponse = resp
            .json()
            .await
            .map_err(|e| VlmError::ApiError(format!("Failed to parse response: {}", e)))?;

        body.choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| VlmError::ApiError("Empty response from VLM".to_string()))
    }
}

#[async_trait]
impl VlmBackend for OpenAiCompatibleVlm {
    async fn describe_image(
        &self,
        image: ImageSource,
        prompt: Option<String>,
    ) -> Result<String, VlmError> {
        image.check_size(self.max_image_bytes)?;

        let user_prompt = prompt.unwrap_or_else(|| {
            "Describe this image in detail. Include objects, text, colors, layout, and any notable features."
                .to_string()
        });

        let content = ChatContent::Multi(vec![
            ContentPart::ImageUrl {
                image_url: ContentImageUrl {
                    url: image.to_data_uri().await.map_err(|e| {
                        VlmError::InvalidImage(e.to_string())
                    })?,
                },
            },
            ContentPart::Text { text: user_prompt },
        ]);

        self.chat_text(
            "You are a helpful visual assistant. Describe images accurately and comprehensively.",
            content,
        )
        .await
    }

    async fn compare_images(
        &self,
        images: Vec<ImageSource>,
        prompt: Option<String>,
    ) -> Result<String, VlmError> {
        if images.len() < 2 {
            return Err(VlmError::ApiError(
                "compare_images requires at least 2 images".to_string(),
            ));
        }

        for img in &images {
            img.check_size(self.max_image_bytes)?;
        }

        let user_prompt = prompt.unwrap_or_else(|| {
            "Compare these images. Note similarities, differences, and any changes between them."
                .to_string()
        });

        let mut parts = Vec::new();
        for (i, img) in images.iter().enumerate() {
            let label = if images.len() == 2 {
                if i == 0 { "First image" } else { "Second image" }
            } else {
                &format!("Image {}", i + 1)
            };
            parts.push(ContentPart::Text { text: label.to_string() });
            parts.push(ContentPart::ImageUrl {
                image_url: ContentImageUrl {
                    url: img.to_data_uri().await.map_err(|e| {
                        VlmError::InvalidImage(e.to_string())
                    })?,
                },
            });
        }
        parts.push(ContentPart::Text { text: user_prompt });

        self.chat_text(
            "You are a visual comparison assistant. Analyze differences and similarities between images.",
            ChatContent::Multi(parts),
        )
        .await
    }

    async fn ocr(
        &self,
        image: ImageSource,
        language: Option<String>,
    ) -> Result<String, VlmError> {
        image.check_size(self.max_image_bytes)?;

        let lang_hint = language
            .as_deref()
            .map(|l| format!(" Extract text in {} language.", l))
            .unwrap_or_default();

        let user_prompt = format!(
            "Extract all visible text from this image.{} Provide the output as plain text with line breaks preserved.",
            lang_hint
        );

        let content = ChatContent::Multi(vec![
            ContentPart::ImageUrl {
                image_url: ContentImageUrl {
                    url: image.to_data_uri().await.map_err(|e| {
                        VlmError::InvalidImage(e.to_string())
                    })?,
                },
            },
            ContentPart::Text { text: user_prompt },
        ]);

        self.chat_text(
            "You are an OCR engine. Extract all visible text from images accurately.",
            content,
        )
        .await
    }

    async fn analyze(
        &self,
        image: ImageSource,
        analysis_type: AnalysisType,
        prompt: Option<String>,
    ) -> Result<String, VlmError> {
        image.check_size(self.max_image_bytes)?;

        let (system_prompt, analysis_name) = match &analysis_type {
            AnalysisType::ObjectDetection => (
                "You are an object detection assistant. List all objects in the image with their positions and descriptions.",
                "object detection",
            ),
            AnalysisType::TextExtraction => (
                "You are a text extraction assistant. Transcribe all text in the image exactly as it appears.",
                "text extraction",
            ),
            AnalysisType::SceneDescription => (
                "You are a scene description assistant. Describe the overall scene, layout, atmosphere, and notable elements.",
                "scene description",
            ),
            AnalysisType::UiAnalysis => (
                "You are a UI analysis assistant. Identify all UI elements (buttons, text fields, icons, etc.) and describe their purpose and state.",
                "UI analysis",
            ),
            AnalysisType::Custom(p) => (p.as_str(), "custom analysis"),
        };

        let user_prompt = prompt.unwrap_or_else(|| {
            match &analysis_type {
                AnalysisType::Custom(_) => {
                    "Analyze this image according to the instructions above.".to_string()
                }
                _ => format!("Analyze this image for {}.", analysis_name),
            }
        });

        let content = ChatContent::Multi(vec![
            ContentPart::ImageUrl {
                image_url: ContentImageUrl {
                    url: image.to_data_uri().await.map_err(|e| {
                        VlmError::InvalidImage(e.to_string())
                    })?,
                },
            },
            ContentPart::Text { text: user_prompt },
        ]);

        self.chat_text(system_prompt, content).await
    }

    async fn health(&self) -> Result<VlmHealth, VlmError> {
        let models = self.list_models().await?;
        Ok(VlmHealth {
            model: self.model.clone(),
            ready: !models.is_empty(),
            details: Some(format!("{} models available", models.len())),
        })
    }

    async fn list_models(&self) -> Result<Vec<String>, VlmError> {
        let url = self.build_endpoint("models");
        let resp = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| VlmError::Connection(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(VlmError::ApiError(format!(
                "Models endpoint returned {}",
                resp.status()
            )));
        }

        let body: ListModelsResponse = resp
            .json()
            .await
            .map_err(|e| VlmError::ApiError(format!("Failed to parse response: {}", e)))?;

        Ok(body.data.iter().map(|m| m.id.clone()).collect())
    }
}

// ─── Ollama Native Implementation ───────────────────────────────────

#[derive(Debug)]
pub struct OllamaVlm {
    client: reqwest::Client,
    endpoint: String,
    max_image_bytes: u64,
}

impl OllamaVlm {
    pub fn new(endpoint: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap();

        Self {
            client,
            endpoint,
            max_image_bytes: 50 * 1024 * 1024, // 50 MiB default
        }
    }

    pub fn with_max_image_size(mut self, max_bytes: u64) -> Self {
        self.max_image_bytes = max_bytes;
        self
    }

    fn build_endpoint(&self, path: &str) -> String {
        format!("{}/api/{}", self.endpoint.trim_end_matches('/'), path)
    }
}

#[async_trait]
impl VlmBackend for OllamaVlm {
    async fn describe_image(
        &self,
        image: ImageSource,
        prompt: Option<String>,
    ) -> Result<String, VlmError> {
        image.check_size(self.max_image_bytes)?;

        let user_prompt = prompt.unwrap_or_else(|| {
            "Describe this image in detail. Include objects, text, colors, layout, and any notable features."
                .to_string()
        });

        let (b64, _) = image.to_base64().await.map_err(|e| {
            VlmError::InvalidImage(format!("Failed to read image: {}", e))
        })?;

        self.ollama_chat("You are a helpful visual assistant.", &user_prompt, Some(vec![b64])).await
    }

    async fn compare_images(
        &self,
        images: Vec<ImageSource>,
        prompt: Option<String>,
    ) -> Result<String, VlmError> {
        if images.len() < 2 {
            return Err(VlmError::ApiError(
                "compare_images requires at least 2 images".to_string(),
            ));
        }

        for img in &images {
            img.check_size(self.max_image_bytes)?;
        }

        let user_prompt = prompt.unwrap_or_else(|| {
            "Compare these images. Note similarities, differences, and any changes between them."
                .to_string()
        });

        let mut b64s = Vec::new();
        for img in &images {
            let (b64, _) = img.to_base64().await.map_err(|e| {
                VlmError::InvalidImage(format!("Failed to read image: {}", e))
            })?;
            b64s.push(b64);
        }

        self.ollama_chat(
            "You are a visual comparison assistant.",
            &user_prompt,
            Some(b64s),
        )
        .await
    }

    async fn ocr(
        &self,
        image: ImageSource,
        language: Option<String>,
    ) -> Result<String, VlmError> {
        image.check_size(self.max_image_bytes)?;

        let lang_hint = language
            .as_deref()
            .map(|l| format!(" Extract text in {} language.", l))
            .unwrap_or_default();

        let user_prompt = format!(
            "Extract all visible text from this image.{} Provide the output as plain text with line breaks preserved.",
            lang_hint
        );

        let (b64, _) = image.to_base64().await.map_err(|e| {
            VlmError::InvalidImage(format!("Failed to read image: {}", e))
        })?;

        self.ollama_chat(
            "You are an OCR engine. Extract all visible text from images accurately.",
            &user_prompt,
            Some(vec![b64]),
        )
        .await
    }

    async fn analyze(
        &self,
        image: ImageSource,
        analysis_type: AnalysisType,
        prompt: Option<String>,
    ) -> Result<String, VlmError> {
        image.check_size(self.max_image_bytes)?;

        let (system, _) = match &analysis_type {
            AnalysisType::ObjectDetection => ("You are an object detection assistant.", ""),
            AnalysisType::TextExtraction => ("You are a text extraction assistant.", ""),
            AnalysisType::SceneDescription => ("You are a scene description assistant.", ""),
            AnalysisType::UiAnalysis => ("You are a UI analysis assistant.", ""),
            AnalysisType::Custom(p) => (p.as_str(), ""),
        };

        let user_prompt = prompt.unwrap_or_else(|| match &analysis_type {
            AnalysisType::Custom(_) => {
                "Analyze this image according to the instructions above.".to_string()
            }
            _ => format!("Analyze this image for {}.", analysis_type),
        });

        let (b64, _) = image.to_base64().await.map_err(|e| {
            VlmError::InvalidImage(format!("Failed to read image: {}", e))
        })?;

        self.ollama_chat(system, &user_prompt, Some(vec![b64])).await
    }

    async fn health(&self) -> Result<VlmHealth, VlmError> {
        let models = self.list_models().await?;
        Ok(VlmHealth {
            model: "ollama".to_string(),
            ready: !models.is_empty(),
            details: Some(format!("{} models available", models.len())),
        })
    }

    async fn list_models(&self) -> Result<Vec<String>, VlmError> {
        let resp = self
            .client
            .get(&self.build_endpoint("tags"))
            .send()
            .await
            .map_err(|e| VlmError::Connection(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(VlmError::ApiError(format!(
                "Tags endpoint returned {}",
                resp.status()
            )));
        }

        let body: OllamaModelsResponse = resp
            .json()
            .await
            .map_err(|e| VlmError::ApiError(format!("Failed to parse response: {}", e)))?;

        Ok(body.models.iter().map(|m| m.name.clone()).collect())
    }
}

impl OllamaVlm {
    async fn ollama_chat(
        &self,
        system: &str,
        user: &str,
        images: Option<Vec<String>>,
    ) -> Result<String, VlmError> {
        let url = self.build_endpoint("chat");

        let req = OllamaChatRequest {
            model: String::new(),
            messages: vec![
                OllamaMessage {
                    role: "system",
                    content: system.to_string(),
                    images: vec![],
                },
                OllamaMessage {
                    role: "user",
                    content: user.to_string(),
                    images: images.unwrap_or_default(),
                },
            ],
            stream: false,
        };

        let resp = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| VlmError::Connection(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(VlmError::ApiError(format!("{}: {}", status, body)));
        }

        let body: OllamaChatResponse = resp
            .json()
            .await
            .map_err(|e| VlmError::ApiError(format!("Failed to parse response: {}", e)))?;

        Ok(body.message.content)
    }
}

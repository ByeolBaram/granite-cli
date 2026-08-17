//! MCP tool definitions for VLM operations.
//!
//! Uses rmcp 3.x #[tool] and #[tool_router] macros for zero-boilerplate
//! MCP tool registration with automatic JSON Schema generation.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{ErrorData, ErrorCode},
    schemars, tool, tool_router,
};
use crate::vlm::{ImageSource, AnalysisType, VlmBackend};
use anyhow::Result;
use url::Url;

// ─── ImageSource Parsing ────────────────────────────────────────────

pub trait ParseImage {
    fn parse_image(&self) -> Result<ImageSource>;
}

impl ParseImage for DescribeImageArgs {
    fn parse_image(&self) -> Result<ImageSource> {
        Ok(match self.source_type.as_str() {
            "file" => ImageSource::File(std::path::PathBuf::from(&self.image)),
            "base64" => {
                let data = base64_decode(&self.image)
                    .map_err(|e| anyhow::anyhow!("Invalid base64: {}", e))?;
                ImageSource::Bytes {
                    data,
                    mime: "image/jpeg".to_string(),
                }
            }
            "url" => {
                let url = Url::parse(&self.image)
                    .map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;
                ImageSource::Url(url)
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unknown image source type: '{}'. Use 'file', 'base64', or 'url'.",
                    other
                ))
            }
        })
    }
}

impl ParseImage for OcrArgs {
    fn parse_image(&self) -> Result<ImageSource> {
        Ok(match self.source_type.as_str() {
            "file" => ImageSource::File(std::path::PathBuf::from(&self.image)),
            "base64" => {
                let data = base64_decode(&self.image)
                    .map_err(|e| anyhow::anyhow!("Invalid base64: {}", e))?;
                ImageSource::Bytes {
                    data,
                    mime: "image/jpeg".to_string(),
                }
            }
            "url" => {
                let url = Url::parse(&self.image)
                    .map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;
                ImageSource::Url(url)
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unknown image source type: '{}'. Use 'file', 'base64', or 'url'.",
                    other
                ))
            }
        })
    }
}

impl ParseImage for AnalyzeArgs {
    fn parse_image(&self) -> Result<ImageSource> {
        Ok(match self.source_type.as_str() {
            "file" => ImageSource::File(std::path::PathBuf::from(&self.image)),
            "base64" => {
                let data = base64_decode(&self.image)
                    .map_err(|e| anyhow::anyhow!("Invalid base64: {}", e))?;
                ImageSource::Bytes {
                    data,
                    mime: "image/jpeg".to_string(),
                }
            }
            "url" => {
                let url = Url::parse(&self.image)
                    .map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;
                ImageSource::Url(url)
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unknown image source type: '{}'. Use 'file', 'base64', or 'url'.",
                    other
                ))
            }
        })
    }
}

/// Simple base64 decoding using standard alphabet (no external crate needed).
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    const DECODER: [u8; 256] = {
        let mut d = [255u8; 256];
        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0u8;
        while i < 64 {
            d[chars[i as usize] as usize] = i;
            i += 1;
        }
        d
    };

    let input = input.trim_end_matches('=');
    let mut output = Vec::with_capacity(input.len() * 3 / 4);

    let chunks = input.as_bytes().chunks(4);
    for chunk in chunks {
        if chunk.len() < 2 {
            return Err(anyhow::anyhow!("Invalid base64 length"));
        }
        let b0 = DECODER[chunk[0] as usize];
        let b1 = if chunk.len() > 1 { DECODER[chunk[1] as usize] } else { 255 };

        if b0 >= 64 || b1 >= 64 {
            return Err(anyhow::anyhow!("Invalid base64 character"));
        }

        let triple = ((b0 as u32) << 18) | ((b1 as u32) << 12);

        if chunk.len() > 2 {
            let b2 = DECODER[chunk[2] as usize];
            if b2 >= 64 {
                return Err(anyhow::anyhow!("Invalid base64 character"));
            }
            output.push(((triple >> 16) & 0xFF) as u8);
            output.push(((triple >> 8) & 0xFF) as u8);
        } else {
            output.push(((triple >> 16) & 0xFF) as u8);
        }

        if chunk.len() > 3 {
            let b3 = DECODER[chunk[3] as usize];
            if b3 >= 64 {
                return Err(anyhow::anyhow!("Invalid base64 character"));
            }
            output.push((triple & 0xFF) as u8);
        }
    }

    Ok(output)
}

// ─── Tool Argument Types ────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DescribeImageArgs {
    #[schemars(description = "Image source type: 'file' (path), 'base64' (data), or 'url' (HTTP(S))")]
    pub source_type: String,
    /// The image: file path, base64-encoded data, or HTTP(S) URL
    pub image: String,
    /// Optional free-text instruction overriding the default description prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OcrArgs {
    #[schemars(description = "Image source type: 'file', 'base64', or 'url'")]
    pub source_type: String,
    pub image: String,
    #[schemars(description = "Language code for text extraction (e.g., 'en', 'zh', 'ja')")]
    pub language: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CompareImagesArgs {
    /// Array of image sources to compare (minimum 2)
    pub images: Vec<DescribeImageArgs>,
    /// Optional free-text instruction
    pub prompt: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AnalyzeArgs {
    #[schemars(description = "Image source type: 'file', 'base64', or 'url'")]
    pub source_type: String,
    pub image: String,
    #[schemars(description = "Analysis type: 'object-detection', 'text-extraction', 'scene-description', 'ui-analysis', or 'custom'")]
    pub analysis_type: String,
    /// Optional instruction (required when analysis_type == 'custom')
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct HealthArgs {}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListModelsArgs {}

// ─── Tool Router (MCP Tools) ────────────────────────────────────────

/// VLM MCP tool registry.
///
/// Wraps a VLM backend and exposes it as MCP tools via #[tool_router].
#[derive(Debug, Clone)]
pub struct VlmToolRegistry {
    vlm: std::sync::Arc<dyn VlmBackend>,
}

impl VlmToolRegistry {
    /// Create a new tool registry with a VLM backend.
    pub fn new(vlm: impl VlmBackend + 'static) -> Self {
        Self {
            vlm: std::sync::Arc::new(vlm),
        }
    }
}

#[tool_router(server_handler)]
impl VlmToolRegistry {
    /// Describe an image with a Vision Language Model.
    #[tool(description = "Describe an image with a Vision Language Model. Returns a natural language description of the image content, including objects, text, colors, layout, and notable features.")]
    async fn vlm_describe_image(
        &self,
        Parameters(args): Parameters<DescribeImageArgs>,
    ) -> Result<String, ErrorData> {
        let image = args.parse_image().map_err(|e| {
            ErrorData::new(ErrorCode::INVALID_PARAMS, e.to_string(), None)
        })?;

        let result = self.vlm.describe_image(image, args.prompt).await
            .map_err(|e| {
                ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
            })?;

        Ok(result)
    }

    /// Extract text from an image using OCR.
    #[tool(description = "Extract all visible text from an image using OCR. Returns the transcribed text as a string, preserving line breaks and layout where possible.")]
    async fn vlm_ocr(
        &self,
        Parameters(args): Parameters<OcrArgs>,
    ) -> Result<String, ErrorData> {
        let image = args.parse_image().map_err(|e| {
            ErrorData::new(ErrorCode::INVALID_PARAMS, e.to_string(), None)
        })?;

        let result = self.vlm.ocr(image, args.language).await
            .map_err(|e| {
                ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
            })?;

        Ok(result)
    }

    /// Compare two or more images.
    #[tool(description = "Compare two or more images and note differences, similarities, and changes. Pass at least 2 images to compare.")]
    async fn vlm_compare_images(
        &self,
        Parameters(args): Parameters<CompareImagesArgs>,
    ) -> Result<String, ErrorData> {
        if args.images.len() < 2 {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "compare_images requires at least 2 images".to_string(),
                None,
            ));
        }

        let mut images = Vec::with_capacity(args.images.len());
        for img_arg in &args.images {
            images.push(img_arg.parse_image().map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Image {}: {}", images.len() + 1, e),
                    None,
                )
            })?);
        }

        let result = self.vlm.compare_images(images, args.prompt).await
            .map_err(|e| {
                ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
            })?;

        Ok(result)
    }

    /// Analyze an image for specific visual characteristics.
    #[tool(description = "Analyze an image for specific visual characteristics. Use 'object-detection' to list objects, 'text-extraction' for OCR, 'scene-description' for layout/atmosphere, 'ui-analysis' for UI elements, or 'custom' with a prompt for arbitrary analysis.")]
    async fn vlm_analyze(
        &self,
        Parameters(args): Parameters<AnalyzeArgs>,
    ) -> Result<String, ErrorData> {
        let image = args.parse_image().map_err(|e| {
            ErrorData::new(ErrorCode::INVALID_PARAMS, e.to_string(), None)
        })?;

        let analysis_type = match args.analysis_type.as_str() {
            "object-detection" => AnalysisType::ObjectDetection,
            "text-extraction" => AnalysisType::TextExtraction,
            "scene-description" => AnalysisType::SceneDescription,
            "ui-analysis" => AnalysisType::UiAnalysis,
            "custom" => {
                let prompt = args.prompt.clone().ok_or_else(|| {
                    ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "'custom' analysis requires a prompt".to_string(),
                        None,
                    )
                })?;
                AnalysisType::Custom(prompt)
            }
            other => {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Unknown analysis type: '{}'", other),
                    None,
                ));
            }
        };

        let result = self.vlm.analyze(image, analysis_type, args.prompt).await
            .map_err(|e| {
                ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
            })?;

        Ok(result)
    }

    /// Check VLM backend health.
    #[tool(description = "Check VLM backend connectivity and return model information.")]
    async fn vlm_health(&self) -> Result<String, ErrorData> {
        let health = self.vlm.health().await
            .map_err(|e| {
                ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Health check failed: {}", e), None)
            })?;

        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "model": health.model,
            "ready": health.ready,
            "details": health.details,
        })).map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Serialization error: {}", e), None)
        })?)
    }

    /// List available VLM models.
    #[tool(description = "List all available VLM models from the backend endpoint.")]
    async fn vlm_list_models(&self) -> Result<String, ErrorData> {
        let models = self.vlm.list_models().await
            .map_err(|e| {
                ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to list models: {}", e), None)
            })?;

        Ok(serde_json::to_string_pretty(&models).map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Serialization error: {}", e), None)
        })?)
    }
}

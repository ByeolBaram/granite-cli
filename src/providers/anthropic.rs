use super::base::{
    ApiSurface, AuthType, ChatChunk, ChatMessage, ChatRequest, ChatResponse, HasProviderMetadata,
    HealthStatus, ModelFormat, Precision, Provider, ProviderConfig, ProviderError,
    ProviderMetadata, ProviderType,
};
use crate::registry::ConfigConstructable;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/*-- public ------------------------------------------------------------------*/

/// Anthropic provider client.
/// Connects to the Anthropic Messages API.
pub struct AnthropicProvider {
    config: ProviderConfig,
    endpoint: String,
    api_key: String,
    client: Client,
}

impl ConfigConstructable for AnthropicProvider {
    type Config = ProviderConfig;

    fn new(cfg: &Self::Config) -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        Self {
            config: cfg.clone(),
            endpoint: cfg.default_endpoint.trim_end_matches('/').to_string(),
            api_key,
            client: Client::new(),
        }
    }
}

impl HasProviderMetadata for AnthropicProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            description: "Anthropic API with Claude models for safe and helpful AI.".to_string(),
            provider_type: ProviderType::Hosted,
            default_endpoint: "https://api.anthropic.com".to_string(),
            api_capabilities: vec![ApiSurface::AnthropicMessages],
            supported_formats: vec![ModelFormat::Safetensors],
            supported_precisions: vec![Precision::BF16, Precision::FP16],
            authentication: vec![AuthType::ApiKey],
            tags: vec![
                "anthropic".to_string(),
                "claude".to_string(),
                "hosted".to_string(),
            ],
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn api_capabilities(&self) -> Vec<ApiSurface> {
        self.config.api_capabilities.clone()
    }

    fn supported_formats(&self) -> Vec<ModelFormat> {
        self.config.supported_formats.clone()
    }

    fn supported_precisions(&self) -> Vec<Precision> {
        self.config.supported_precisions.clone()
    }

    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let start = Instant::now();

        if request.messages.is_empty() {
            return Err(ProviderError::Other(
                "Message list cannot be empty".to_string(),
            ));
        }

        let messages: Vec<AnthropicMessage> = request
            .messages
            .into_iter()
            .map(|m| AnthropicMessage {
                role: m.role.to_string(),
                content: m.content,
            })
            .collect();

        let chat_request = AnthropicChatRequest {
            model: request.model,
            messages,
            max_tokens: request.max_tokens.or(Some(1024)),
            temperature: request.temperature,
            stream: false,
        };

        let url = format!("{}/v1/messages", self.endpoint);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("x-api-key", self.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .json(&chat_request)
            .send()
            .await?;

        if resp.status() == 401 {
            return Err(ProviderError::Auth(
                "Invalid or missing API key".to_string(),
            ));
        }

        if resp.status() == 429 {
            let body = resp.text().await.ok();
            return Err(ProviderError::RateLimited(body.unwrap_or_default()));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.ok().unwrap_or_default();
            return Err(ProviderError::Other(format!("HTTP {}: {}", status, body)));
        }

        let anthropic_resp: AnthropicChatResponse = resp.json().await?;

        let content = anthropic_resp
            .content
            .iter()
            .filter_map(|block| block.text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let usage = Some(super::base::UsageInfo {
            prompt_tokens: anthropic_resp.usage.input_tokens,
            completion_tokens: anthropic_resp.usage.output_tokens,
            total_tokens: anthropic_resp.usage.input_tokens + anthropic_resp.usage.output_tokens,
        });

        let _latency = start.elapsed();

        Ok(ChatResponse {
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            finish_reason: anthropic_resp.stop_reason,
            usage,
        })
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
    ) -> Result<
        Box<dyn Stream<Item = Result<ChatChunk, ProviderError>> + Send + Unpin>,
        ProviderError,
    > {
        if request.messages.is_empty() {
            return Err(ProviderError::Other(
                "Message list cannot be empty".to_string(),
            ));
        }

        let messages: Vec<AnthropicMessage> = request
            .messages
            .into_iter()
            .map(|m| AnthropicMessage {
                role: m.role.to_string(),
                content: m.content,
            })
            .collect();

        let chat_request = AnthropicChatRequest {
            model: request.model,
            messages,
            max_tokens: request.max_tokens.or(Some(1024)),
            temperature: request.temperature,
            stream: true,
        };

        let url = format!("{}/v1/messages", self.endpoint);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("x-api-key", self.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .header("Accept", "text/event-stream")
            .json(&chat_request)
            .send()
            .await?;

        if resp.status() == 401 {
            return Err(ProviderError::Auth(
                "Invalid or missing API key".to_string(),
            ));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.ok().unwrap_or_default();
            return Err(ProviderError::Other(format!("HTTP {}: {}", status, body)));
        }

        let stream = resp
            .bytes_stream()
            .map(|result| {
                let bytes = result.map_err(ProviderError::Http)?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                let lines: Vec<&str> = text.split('\n').collect();

                for line in lines {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if let Ok(chunk_data) = serde_json::from_str::<AnthropicStreamChunk>(data)
                        {
                            match chunk_data.event_type.as_str() {
                                "content_block_delta" => {
                                    if let Some(delta) = chunk_data.delta {
                                        if let Some(text) = delta.text {
                                            if !text.is_empty() {
                                                return Ok::<Option<ChatChunk>, ProviderError>(
                                                    Some(ChatChunk {
                                                        content: text,
                                                        finish_reason: None,
                                                    }),
                                                );
                                            }
                                        }
                                    }
                                }
                                "message_stop" => {
                                    return Ok::<Option<ChatChunk>, ProviderError>(None);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ok::<Option<ChatChunk>, ProviderError>(None)
            })
            .filter_map(|result| async move {
                match result {
                    Ok(Some(chunk)) => Some(Ok(chunk)),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                }
            });

        Ok(Box::new(Box::pin(stream)))
    }

    async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
        let start = Instant::now();

        let minimal_request = AnthropicChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            max_tokens: Some(1),
            temperature: None,
            stream: false,
        };

        let url = format!("{}/v1/messages", self.endpoint);
        match self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("x-api-key", self.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .json(&minimal_request)
            .send()
            .await
        {
            Ok(resp) => {
                let latency = start.elapsed();
                let healthy = resp.status().as_u16() != 401 && resp.status().as_u16() != 404;
                Ok(HealthStatus {
                    healthy,
                    latency,
                    error: if healthy {
                        None
                    } else {
                        Some(format!("Health check failed: HTTP {}", resp.status()))
                    },
                })
            }
            Err(e) => {
                let latency = start.elapsed();
                Ok(HealthStatus {
                    healthy: false,
                    latency,
                    error: Some(e.to_string()),
                })
            }
        }
    }
}

/*-- Anthropic API Types -----------------------------------------------------*/

#[derive(Serialize)]
struct AnthropicChatRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicChatResponse {
    content: Vec<AnthropicContentBlock>,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    _block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicStreamChunk {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    delta: Option<AnthropicDelta>,
}

#[derive(Deserialize)]
struct AnthropicDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ProviderConfig {
        ProviderConfig {
            id: "test".to_string(),
            name: "Test Anthropic".to_string(),
            description: "Test".to_string(),
            provider_type: ProviderType::Hosted,
            default_endpoint: "https://api.anthropic.com".to_string(),
            api_capabilities: vec![ApiSurface::AnthropicMessages],
            supported_formats: vec![ModelFormat::Safetensors],
            supported_precisions: vec![Precision::BF16],
            authentication: vec![AuthType::ApiKey],
            tags: vec![],
        }
    }

    #[test]
    fn test_provider_creation() {
        let config = create_test_config();
        let provider = AnthropicProvider::new(&config);
        assert_eq!(provider.id(), "test");
        assert_eq!(provider.name(), "Test Anthropic");
        assert!(provider
            .api_capabilities()
            .contains(&ApiSurface::AnthropicMessages));
    }

    #[test]
    fn test_provider_trait_bounds() {
        let config = create_test_config();
        let provider: Box<dyn Provider<Config = ProviderConfig>> =
            Box::new(AnthropicProvider::new(&config));
        assert_eq!(provider.id(), "test");
    }
}
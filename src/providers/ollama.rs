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

/// Ollama provider client.
/// Connects to a local or remote Ollama instance.
pub struct OllamaProvider {
    config: ProviderConfig,
    endpoint: String,
    client: Client,
}

impl ConfigConstructable for OllamaProvider {
    type Config = ProviderConfig;

    fn new(cfg: &Self::Config) -> Self {
        Self {
            config: cfg.clone(),
            endpoint: cfg.default_endpoint.trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }
}

impl HasProviderMetadata for OllamaProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            id: "ollama".to_string(),
            name: "Ollama".to_string(),
            description: "Local model serving via Ollama. Supports many open-source models."
                .to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost:11434".to_string(),
            api_capabilities: vec![ApiSurface::OllamaChat],
            supported_formats: vec![ModelFormat::GGUF],
            supported_precisions: vec![
                Precision::Q8_0,
                Precision::Q4_K_M,
                Precision::Q5_K_M,
                Precision::Q3_K_M,
            ],
            authentication: vec![AuthType::None],
            tags: vec![
                "ollama".to_string(),
                "local".to_string(),
                "gguf".to_string(),
            ],
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
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

        let messages: Vec<OllamaMessage> = request
            .messages
            .into_iter()
            .map(|m| OllamaMessage {
                role: m.role.to_string(),
                content: m.content,
            })
            .collect();

        let chat_request = OllamaChatRequest {
            model: request.model,
            messages,
            temperature: request.temperature,
            stream: false,
        };

        let url = format!("{}/api/chat", self.endpoint);
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.ok().unwrap_or_default();
            return Err(ProviderError::Other(format!("HTTP {}: {}", status, body)));
        }

        let ollama_resp: OllamaChatResponse = resp.json().await?;

        let content = ollama_resp.message.map(|m| m.content);

        let _latency = start.elapsed();

        Ok(ChatResponse {
            content,
            finish_reason: if ollama_resp.done {
                Some("stop".to_string())
            } else {
                None
            },
            usage: None,
        })
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
    ) -> Result<
        Box<dyn Stream<Item = Result<ChatChunk, ProviderError>> + Send + Unpin>,
        ProviderError,
    > {
        let messages: Vec<OllamaMessage> = request
            .messages
            .into_iter()
            .map(|m| OllamaMessage {
                role: m.role.to_string(),
                content: m.content,
            })
            .collect();

        let chat_request = OllamaChatRequest {
            model: request.model,
            messages,
            temperature: request.temperature,
            stream: true,
        };

        let url = format!("{}/api/chat", self.endpoint);
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await?;

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
                    if line.is_empty() {
                        continue;
                    }

                    if let Ok(resp_data) = serde_json::from_str::<serde_json::Value>(&line) {
                        if let Some(content) = resp_data
                            .get("message")
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            if !content.is_empty() {
                                return Ok::<Option<ChatChunk>, ProviderError>(Some(ChatChunk {
                                    content: content.to_string(),
                                    finish_reason: None,
                                }));
                            }
                        }

                        if let Some(done) = resp_data.get("done").and_then(|d| d.as_bool()) {
                            if done {
                                return Ok::<Option<ChatChunk>, ProviderError>(None);
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

        let url = format!("{}/api/tags", self.endpoint);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                let latency = start.elapsed();
                let healthy = resp.status().is_success();
                Ok(HealthStatus {
                    healthy,
                    latency,
                    error: if healthy {
                        None
                    } else {
                        Some(format!("HTTP {}", resp.status()))
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

/*-- Ollama API Types --------------------------------------------------------*/

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(rename = "stream", skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    _model: String,
    message: Option<OllamaResponseMessage>,
    done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    _total_duration: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ProviderConfig {
        ProviderConfig {
            id: "test".to_string(),
            name: "Test Ollama".to_string(),
            description: "Test".to_string(),
            provider_type: ProviderType::Local,
            default_endpoint: "http://localhost:11434".to_string(),
            api_capabilities: vec![ApiSurface::OllamaChat],
            supported_formats: vec![ModelFormat::GGUF],
            supported_precisions: vec![Precision::Q4_K_M],
            authentication: vec![AuthType::None],
            tags: vec![],
        }
    }

    #[test]
    fn test_provider_creation() {
        let config = create_test_config();
        let provider = OllamaProvider::new(&config);
        assert_eq!(provider.id(), "test");
        assert_eq!(provider.name(), "Test Ollama");
        assert!(provider
            .api_capabilities()
            .contains(&ApiSurface::OllamaChat));
    }

    #[test]
    fn test_provider_trait_bounds() {
        let config = create_test_config();
        let provider: Box<dyn Provider<Config = ProviderConfig>> =
            Box::new(OllamaProvider::new(&config));
        assert_eq!(provider.id(), "test");
    }
}
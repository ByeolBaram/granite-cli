use crate::providers::{
    ApiSurface, ChatChunk, ChatRequest, ChatResponse, HealthStatus,
    ModelFormat, Precision, Provider, ProviderError,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// OpenAI-compatible provider client.
/// Works with any API that follows the OpenAI chat completions format.
pub struct OpenAiCompatProvider {
    id: String,
    name: String,
    endpoint: String,
    api_key: String,
    client: Client,
}

#[derive(Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: Option<OpenAiResponseMessage>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModelInfo>,
}

#[derive(Deserialize)]
struct OpenAiModelInfo {
    id: String,
}

impl OpenAiCompatProvider {
    pub fn new(id: impl Into<String>, name: impl Into<String>, endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            client: Client::new(),
        }
    }

    async fn models_list(&self) -> Result<Vec<String>, ProviderError> {
        let url = format!("{}/v1/models", self.endpoint);
        let resp = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            if status == 401 {
                return Err(ProviderError::Auth("Invalid or missing API key".to_string()));
            }
            return Err(ProviderError::Other(format!("Failed to list models: HTTP {}", status)));
        }

        let models_resp: OpenAiModelsResponse = resp.json().await?;
        Ok(models_resp.data.into_iter().map(|m| m.id).collect())
    }
}

#[async_trait]
impl Provider for OpenAiCompatProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn api_capabilities(&self) -> Vec<ApiSurface> {
        vec![ApiSurface::OpenAIChat]
    }

    fn supported_formats(&self) -> Vec<ModelFormat> {
        vec![ModelFormat::Safetensors]
    }

    fn supported_precisions(&self) -> Vec<Precision> {
        vec![Precision::BF16, Precision::FP16]
    }

    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let start = Instant::now();

        let messages: Vec<OpenAiMessage> = request.messages.into_iter().map(|m| OpenAiMessage {
            role: m.role.to_string(),
            content: m.content,
        }).collect();

        let chat_request = OpenAiChatRequest {
            model: request.model,
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop_sequences,
            stream: false,
        };

        let url = format!("{}/v1/chat/completions", self.endpoint);
        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await?;

        if resp.status() == 401 {
            return Err(ProviderError::Auth("Invalid or missing API key".to_string()));
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

        let openai_resp: OpenAiChatResponse = resp.json().await?;

        let content = openai_resp.choices.first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone());

        let finish_reason = openai_resp.choices.first()
            .and_then(|c| c.finish_reason.clone());

        let usage = openai_resp.usage.map(|u| crate::providers::UsageInfo {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        let _latency = start.elapsed();

        Ok(ChatResponse {
            content,
            finish_reason,
            usage,
        })
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<ChatChunk, ProviderError>> + Send + Unpin>, ProviderError> {
        let messages: Vec<OpenAiMessage> = request.messages.into_iter().map(|m| OpenAiMessage {
            role: m.role.to_string(),
            content: m.content,
        }).collect();

        let chat_request = OpenAiChatRequest {
            model: request.model,
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop_sequences,
            stream: true,
        };

        let url = format!("{}/v1/chat/completions", self.endpoint);
        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&chat_request)
            .send()
            .await?;

        if resp.status() == 401 {
            return Err(ProviderError::Auth("Invalid or missing API key".to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.ok().unwrap_or_default();
            return Err(ProviderError::Other(format!("HTTP {}: {}", status, body)));
        }

        let stream = resp.bytes_stream()
            .map(|result| {
                let bytes = result.map_err(ProviderError::Http)?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                let lines: Vec<&str> = text.split('\n').collect();

                for line in lines {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" {
                            return Ok::<Option<ChatChunk>, ProviderError>(None);
                        }
                        if let Ok(delta) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(content) = delta.get("choices")
                                .and_then(|c| c.get(0))
                                .and_then(|c| c.get("delta"))
                                .and_then(|d| d.get("content"))
                                .and_then(|c| c.as_str())
                            {
                                if !content.is_empty() {
                                    return Ok(Some(ChatChunk {
                                        content: content.to_string(),
                                        finish_reason: None,
                                    }));
                                }
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
        match self.models_list().await {
            Ok(_models) => {
                let latency = start.elapsed();
                Ok(HealthStatus {
                    healthy: true,
                    latency,
                    error: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OpenAiCompatProvider::new("test", "Test Provider", "https://api.test.com", "sk-test");
        assert_eq!(provider.id(), "test");
        assert_eq!(provider.name(), "Test Provider");
        assert!(provider.api_capabilities().contains(&ApiSurface::OpenAIChat));
    }

    #[test]
    fn test_provider_trait_bounds() {
        let provider: Box<dyn Provider> = Box::new(OpenAiCompatProvider::new("test", "Test", "https://api.test.com", "key"));
        assert_eq!(provider.id(), "test");
    }
}

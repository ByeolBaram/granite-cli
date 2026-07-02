use anyhow::Result;
use dialoguer::Input;
use futures::StreamExt;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::utils::{self, fetch_markdown};
use crate::registry::Registry;

/// REPL chatbot that connects to a provider and streams responses.
pub struct RunChatbot;

struct ConversationState {
    messages: VecDeque<crate::providers::ChatMessage>,
    model_id: Option<String>,
}

impl RunChatbot {
    /// Run the REPL chatbot loop.
    pub async fn run(ctx: &mut crate::AppContext, model_id: Option<String>) -> Result<()> {
        let mut state = ConversationState {
            messages: VecDeque::new(),
            model_id,
        };

        println!("Granite-cli Chatbot");
        println!("Type your message or use commands:");
        println!("  /help     - Show available commands");
        println!("  /clear    - Clear conversation history");
        println!("  /exit     - Exit the chatbot");
        println!("  /quit     - Exit the chatbot");
        println!("  /model    - Switch model");
        println!();

        // Try to resolve a provider
        let provider = Self::resolve_provider(ctx).await;

        loop {
            // Add system prompt if empty
            if state.messages.is_empty() {
                state.messages.push_back(crate::providers::ChatMessage {
                    role: crate::providers::MessageRole::System,
                    content: "You are a helpful assistant powered by Granite models.".to_string(),
                });
            }

            // Read user input
            let input: String = Input::new()
                .with_prompt("You")
                .interact_text()?;

            let input_trimmed = input.trim();
            if input_trimmed.is_empty() {
                continue;
            }

            // Handle commands
            if input_trimmed.starts_with('/') {
                match Self::handle_command(&mut state, input_trimmed, &provider) {
                    CommandResult::Exit => break,
                    CommandResult::Clear => {
                        state.messages.clear();
                        println!("Conversation cleared.");
                    }
                    CommandResult::Switch(model_id) => {
                        let new_model = model_id.clone();
                        state.model_id = Some(model_id);
                        println!("Switched to model: {}", new_model);
                    }
                    CommandResult::Help => {
                        Self::print_help();
                    }
                    CommandResult::Continue => {}
                }
                continue;
            }

            // Add user message
            state.messages.push_back(crate::providers::ChatMessage {
                role: crate::providers::MessageRole::User,
                content: input_trimmed.to_string(),
            });

            // Check for URLs in input
            let urls = utils::extract_urls(input_trimmed);
            if !urls.is_empty() {
                println!("\nFetching {} URL(s)...\n", urls.len());
                for url in &urls {
                    match fetch_markdown(url).await {
                        Ok(content) => {
                            let truncated = if content.len() > 2000 {
                                format!("{}...", &content[..2000])
                            } else {
                                content
                            };
                            state.messages.push_back(crate::providers::ChatMessage {
                                role: crate::providers::MessageRole::User,
                                content: format!("\n\n--- Content from {} ---\n\n{}", url, truncated),
                            });
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to fetch {}: {}", url, e);
                        }
                    }
                }
            }

            // Get assistant response
            match &provider {
                Some((provider, model)) => {
                    let prefix = if provider.name().len() > 1 {
                        provider.name()[..1].to_uppercase()
                    } else {
                        "G".to_string()
                    };
                    print!("\n{} ", prefix);

                    let request = crate::providers::ChatRequest {
                        model: model.clone(),
                        messages: state.messages.iter().cloned().collect(),
                        temperature: Some(0.7),
                        max_tokens: Some(2048),
                        stop_sequences: None,
                        stream: true,
                    };

                    match provider.stream_chat(request).await {
                        Ok(mut stream) => {
                            let mut full_response = String::new();
                            while let Some(chunk_result) = stream.next().await {
                                match chunk_result {
                                    Ok(chunk) => {
                                        print!("{}", chunk.content);
                                        full_response.push_str(&chunk.content);
                                    }
                                    Err(e) => {
                                        eprintln!("\nError: {}", e);
                                        break;
                                    }
                                }
                            }
                            println!();
                            println!();

                            // Add assistant response to history
                            state.messages.push_back(crate::providers::ChatMessage {
                                role: crate::providers::MessageRole::Assistant,
                                content: full_response,
                            });
                        }
                        Err(e) => {
                            eprintln!("\nError: {}", e);
                        }
                    }
                }
                None => {
                    println!("\nNo provider configured. Configure a provider first:");
                    println!("  granite-cli provider setup <provider-id>");
                    println!();
                    println!("Or run without a model:");
                    println!("  granite-cli run <model-id>");
                    println!();
                    println!("Available bundled models:");
                    // TODO: List available models from registry
                    // for model in crate::registry::MODEL_REGISTRY.list() {
                    //     println!("  - {} ({})", model.id, model.model_type);
                    // }
                    println!();
                }
            }
        }

        Ok(())
    }

    async fn resolve_provider(ctx: &mut crate::AppContext) -> Option<(Arc<dyn crate::providers::Provider<Config = crate::providers::base::ProviderConfig>>, String)> {
        // Try to find a configured provider
        let configured_providers: Vec<_> = ctx.config.providers.keys().cloned().collect();

        if configured_providers.is_empty() {
            return None;
        }

        // Use the first configured provider
        let provider_id = &configured_providers[0];

        let factory = crate::di::Factory::new(ctx.config.clone());
        let provider = factory.resolve_configured_provider(provider_id).await.ok()?;

        // Determine model
        let model = if let Some(ref model_id) = ctx.config.models.keys().next().cloned() {
            // Try to use a configured model
            if factory.resolve_model_provider(model_id).await.ok().flatten().is_some() {
                model_id.clone()
            } else {
                // Fall back to the first available model
                ctx.config.models.keys().next().cloned().unwrap_or_default()
            }
        } else {
            // Use first bundled model
            // TODO: Resolve default model from registry
            return None;
            // crate::registry::MODEL_REGISTRY.list().first()
            //     .map(|m| m.id.clone())
            //     .unwrap_or_default()
        };

        Some((provider, model))
    }

    fn handle_command(
        _state: &mut ConversationState,
        input: &str,
        provider: &Option<(Arc<dyn crate::providers::Provider<Config = crate::providers::base::ProviderConfig>>, String)>,
    ) -> CommandResult {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts[0];

        match command {
            "/exit" | "/quit" => CommandResult::Exit,
            "/clear" => CommandResult::Clear,
            "/help" => CommandResult::Help,
            "/model" => {
                if let Some(model_id) = parts.get(1) {
                    CommandResult::Switch(model_id.to_string())
                } else {
                    // Show available models for switching
                    if let Some((_provider, current_model)) = provider {
                        println!("\nCurrent model: {}", current_model);
                        println!("\nAvailable bundled models:");
                        // TODO: List available models
                        let models = Vec::<crate::models::ModelMetadata>::new();
                        // let models: Vec<_> = crate::registry::MODEL_REGISTRY.list().to_vec();
                        // for (idx, model) in models.iter().enumerate() {
                        //     println!("  [{}] {} ({})", idx, model.id, model.model_type);
                        // }

                        let selection = dialoguer::Select::new()
                            .with_prompt("Select a model")
                            .items(&models.iter()
                                .map(|m| m.id.as_str())
                                .collect::<Vec<_>>())
                            .default(0)
                            .interact();

                        if let Ok(idx) = selection {
                            let selected = models[idx].id.clone();
                            CommandResult::Switch(selected)
                        } else {
                            CommandResult::Continue
                        }
                    } else {
                        println!("No provider configured. Run: granite-cli provider setup <id>");
                        CommandResult::Continue
                    }
                }
            }
            _ => CommandResult::Continue,
        }
    }

    fn print_help() {
        println!("\nAvailable commands:");
        println!("  /help     - Show this help message");
        println!("  /clear    - Clear conversation history");
        println!("  /exit     - Exit the chatbot");
        println!("  /quit     - Exit the chatbot");
        println!("  /model [id] - Switch to a different model");
        println!("              Without argument, shows available models");
        println!();
        println!("Tips:");
        println!("  - Paste URLs in your message to include web content");
        println!("  - URLs are fetched and converted to markdown");
        println!("  - Conversation history is kept in memory for context");
    }
}

enum CommandResult {
    Exit,
    Clear,
    Switch(String),
    Help,
    Continue,
}

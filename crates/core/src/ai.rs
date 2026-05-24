use anyhow::Result;
use serde_json::json;
use std::env;
use std::time::Duration;

/// HTTP-based AI client for Ollama and OpenAI-compatible APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpAiClient {
    api_url: String,
    model: String,
    api_key: Option<String>,
    provider: AiProvider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AiProvider {
    Ollama,
    OpenAiCompatible,
}

impl HttpAiClient {
    pub fn from_env() -> Option<Self> {
        let provider = env::var("NEOJOPLIN_AI_PROVIDER").unwrap_or_else(|_| "ollama".to_string());

        match provider.as_str() {
            "ollama" => Some(Self {
                api_url: env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string()),
                model: env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string()),
                api_key: None,
                provider: AiProvider::Ollama,
            }),
            "openai" => Some(Self {
                api_url: env::var("OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string()),
                model: env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
                api_key: env::var("OPENAI_API_KEY").ok(),
                provider: AiProvider::OpenAiCompatible,
            }),
            _ => None,
        }
    }

    pub async fn generate_text(&self, prompt: &str, system_prompt: Option<&str>) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;

        let mut messages: Vec<_> = Vec::new();

        if let Some(sys) = system_prompt {
            messages.push(json!({
                "role": "system",
                "content": sys
            }));
        }

        messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let mut request_body = json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.7,
            "max_tokens": 2048,
        });

        if self.provider == AiProvider::Ollama {
            request_body["stream"] = json!(false);
        }

        let mut request = client.post(self.request_url()).json(&request_body);

        if let Some(ref key) = self.api_key {
            request = request.bearer_auth(key);
        }

        let response = request.send().await?;
        let body: serde_json::Value = response.json().await?;
        Self::extract_text(&body)
    }

    fn request_url(&self) -> String {
        match self.provider {
            AiProvider::Ollama => format!("{}/api/chat", self.api_url.trim_end_matches('/')),
            AiProvider::OpenAiCompatible => self.api_url.clone(),
        }
    }

    fn extract_text(body: &serde_json::Value) -> Result<String> {
        if let Some(message) = body["message"].as_object() {
            if let Some(content) = message["content"].as_str() {
                return Ok(content.to_string());
            }
        }

        if let Some(choices) = body["choices"].as_array() {
            if let Some(first) = choices.first() {
                if let Some(message) = first["message"].as_object() {
                    if let Some(content) = message["content"].as_str() {
                        return Ok(content.to_string());
                    }
                }
            }
        }

        if let Some(response) = body["response"].as_str() {
            return Ok(response.to_string());
        }

        Err(anyhow::anyhow!(
            "Unexpected AI API response format: {}",
            body
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn unset_ai_env() {
        unsafe {
            env::remove_var("NEOJOPLIN_AI_PROVIDER");
            env::remove_var("OLLAMA_BASE_URL");
            env::remove_var("OLLAMA_MODEL");
            env::remove_var("OPENAI_BASE_URL");
            env::remove_var("OPENAI_MODEL");
            env::remove_var("OPENAI_API_KEY");
        }
    }

    #[test]
    fn from_env_defaults_to_ollama() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        unset_ai_env();

        let client = HttpAiClient::from_env().expect("client should be created");
        assert_eq!(client.provider, AiProvider::Ollama);
        assert_eq!(client.request_url(), "http://127.0.0.1:11434/api/chat");
    }

    #[test]
    fn from_env_reads_openai_values() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        unset_ai_env();
        unsafe {
            env::set_var("NEOJOPLIN_AI_PROVIDER", "openai");
            env::set_var(
                "OPENAI_BASE_URL",
                "https://example.test/v1/chat/completions",
            );
            env::set_var("OPENAI_MODEL", "test-model");
            env::set_var("OPENAI_API_KEY", "secret");
        }

        let client = HttpAiClient::from_env().expect("client should be created");
        assert_eq!(client.provider, AiProvider::OpenAiCompatible);
        assert_eq!(
            client.request_url(),
            "https://example.test/v1/chat/completions"
        );
        assert_eq!(client.model, "test-model");
        assert_eq!(client.api_key.as_deref(), Some("secret"));
    }

    #[test]
    fn from_env_rejects_unknown_provider() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        unset_ai_env();
        unsafe {
            env::set_var("NEOJOPLIN_AI_PROVIDER", "unknown-provider");
        }

        assert!(HttpAiClient::from_env().is_none());
    }

    #[test]
    fn extract_text_supports_ollama_message_format() {
        let body = json!({
            "message": {
                "content": "ollama reply"
            }
        });
        let parsed = HttpAiClient::extract_text(&body).expect("expected parsed reply");
        assert_eq!(parsed, "ollama reply");
    }

    #[test]
    fn extract_text_supports_openai_choices_format() {
        let body = json!({
            "choices": [{
                "message": {
                    "content": "openai reply"
                }
            }]
        });
        let parsed = HttpAiClient::extract_text(&body).expect("expected parsed reply");
        assert_eq!(parsed, "openai reply");
    }

    #[test]
    fn extract_text_supports_ollama_generate_format() {
        let body = json!({
            "response": "generate reply"
        });
        let parsed = HttpAiClient::extract_text(&body).expect("expected parsed reply");
        assert_eq!(parsed, "generate reply");
    }

    #[test]
    fn extract_text_errors_on_unknown_format() {
        let body = json!({ "x": 1 });
        let err = HttpAiClient::extract_text(&body).expect_err("expected error");
        assert!(err
            .to_string()
            .contains("Unexpected AI API response format"));
    }
}

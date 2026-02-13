use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("LLM error: {0}")]
    Llm(String),
}

pub type LlmResult<T> = Result<T, LlmError>;

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    temperature: f32,
    max_tokens: usize,
    stream: bool,
}

#[derive(Deserialize, Debug)]
struct GenerateResponse {
    response: String,
}

pub struct LlmClient {
    client: Client,
    model: String,
    temperature: f32,
    max_tokens: usize,
    base_url: String,
}

impl LlmClient {
    pub fn new(
        model: &str,
        temperature: f32,
        max_tokens: usize,
        base_url: &str,
    ) -> LlmResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;

        Ok(Self {
            client,
            model: model.to_string(),
            temperature,
            max_tokens,
            base_url: base_url.to_string(),
        })
    }

    pub fn generate(&self, prompt: &str) -> LlmResult<String> {
        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            stream: false,
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&request)
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(LlmError::Llm(format!("status: {}, body: {}", status, text)));
        }

        let generate_response: GenerateResponse = response.json()?;

        Ok(generate_response.response)
    }
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::new(
            "minimax-m2.5:cloud",
            0.7,
            2048,
            "http://localhost:11434",
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_client_creation() {
        let client = LlmClient::new("minimax-m2.5:cloud", 0.7, 2048, "http://localhost:11434");
        assert!(client.is_ok());
    }

    #[test]
    #[ignore]
    fn test_llm_generate() {
        let client = LlmClient::default();
        let result = client.generate("Say 'Hello, World!' in exactly 3 words.");
        if let Ok(response) = result {
            println!("LLM response: {}", response);
            assert!(!response.is_empty());
        }
    }
}

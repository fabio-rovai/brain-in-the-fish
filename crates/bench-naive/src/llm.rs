//! Claude API client for real LLM-backed subagent evaluation.
//!
//! Copied from `crates/core/src/llm.rs` with the essential types needed
//! for scoring, debate challenges, and debate responses.

use serde::{Deserialize, Serialize};

/// Claude API client configuration.
pub struct ClaudeClient {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

/// A message in the Claude conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Claude API response.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeResponse {
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
}

impl ClaudeClient {
    /// Create a new client from environment variable ANTHROPIC_API_KEY.
    /// Model defaults to claude-sonnet-4-6 (fast + capable).
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            anyhow::anyhow!("ANTHROPIC_API_KEY not set. Set it to use real LLM scoring.")
        })?;
        Ok(Self {
            api_key,
            model: std::env::var("BRAIN_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-6".to_string()),
            base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".to_string()),
            client: reqwest::Client::new(),
        })
    }

    /// Check if a client can be created (API key exists).
    pub fn available() -> bool {
        std::env::var("ANTHROPIC_API_KEY").is_ok()
    }

    /// Send a message to Claude and get a text response.
    pub async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        max_tokens: u32,
    ) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
        });

        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Claude API error {}: {}", status, text);
        }

        let response: ClaudeResponse = resp.json().await?;
        let text = response
            .content
            .iter()
            .filter_map(|b| b.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("");

        Ok(text)
    }

    /// Score a document section against a criterion as a specific agent persona.
    /// Returns parsed JSON with score, justification, evidence_used, gaps_identified.
    pub async fn score_as_agent(
        &self,
        prompt: &str,
    ) -> anyhow::Result<ScoringResult> {
        let system = "You are an expert evaluator. Respond ONLY with valid JSON matching the requested format. No markdown, no explanation outside the JSON.";
        let messages = vec![Message {
            role: "user".into(),
            content: prompt.to_string(),
        }];

        let response = self.complete(system, &messages, 2000).await?;
        let json_str = extract_json(&response);
        let result: ScoringResult = serde_json::from_str(&json_str).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse scoring response: {}. Raw: {}",
                e,
                response
            )
        })?;

        Ok(result)
    }

    /// Generate a debate challenge argument.
    pub async fn generate_challenge(
        &self,
        prompt: &str,
    ) -> anyhow::Result<ChallengeResult> {
        let system =
            "You are an expert evaluator in a structured debate. Respond ONLY with valid JSON.";
        let messages = vec![Message {
            role: "user".into(),
            content: prompt.to_string(),
        }];
        let response = self.complete(system, &messages, 1500).await?;
        let json_str = extract_json(&response);
        let result: ChallengeResult = serde_json::from_str(&json_str).map_err(|e| {
            anyhow::anyhow!("Failed to parse challenge: {}. Raw: {}", e, response)
        })?;
        Ok(result)
    }

    /// Generate a debate response (defending or adjusting score).
    pub async fn generate_response(
        &self,
        prompt: &str,
    ) -> anyhow::Result<ResponseResult> {
        let system = "You are an expert evaluator responding to a challenge. Respond ONLY with valid JSON.";
        let messages = vec![Message {
            role: "user".into(),
            content: prompt.to_string(),
        }];
        let response = self.complete(system, &messages, 1500).await?;
        let json_str = extract_json(&response);
        let result: ResponseResult = serde_json::from_str(&json_str).map_err(|e| {
            anyhow::anyhow!("Failed to parse response: {}. Raw: {}", e, response)
        })?;
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringResult {
    pub score: f64,
    pub justification: String,
    pub evidence_used: Vec<String>,
    pub gaps_identified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResult {
    pub argument: String,
    pub evidence_cited: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseResult {
    pub maintain_score: bool,
    pub new_score: Option<f64>,
    pub response: String,
    pub justification: String,
}

/// Extract JSON from a response that may be wrapped in ```json blocks.
pub fn extract_json(text: &str) -> String {
    let trimmed = text.trim();
    // Try to find JSON block
    if let Some(start) = trimmed.find("```json")
        && let Some(end) = trimmed[start + 7..].find("```")
    {
        return trimmed[start + 7..start + 7 + end].trim().to_string();
    }
    if let Some(start) = trimmed.find("```")
        && let Some(end) = trimmed[start + 3..].find("```")
    {
        let inner = trimmed[start + 3..start + 3 + end].trim();
        if inner.starts_with('{') || inner.starts_with('[') {
            return inner.to_string();
        }
    }
    // Try to find raw JSON object
    if let Some(start) = trimmed.find('{')
        && let Some(end) = trimmed.rfind('}')
    {
        return trimmed[start..=end].to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_raw() {
        let input = r#"{"score": 7, "justification": "good"}"#;
        assert_eq!(extract_json(input), input);
    }

    #[test]
    fn test_extract_json_markdown_block() {
        let input = "Here's my evaluation:\n```json\n{\"score\": 7}\n```\nDone.";
        assert_eq!(extract_json(input), "{\"score\": 7}");
    }

    #[test]
    fn test_extract_json_with_text_around() {
        let input = "The score is {\"score\": 7, \"justification\": \"test\"} as shown.";
        assert_eq!(
            extract_json(input),
            "{\"score\": 7, \"justification\": \"test\"}"
        );
    }

    #[test]
    fn test_scoring_result_deserialize() {
        let json = r#"{"score": 7.5, "justification": "Strong analysis", "evidence_used": ["citation1"], "gaps_identified": ["gap1"]}"#;
        let result: ScoringResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.score, 7.5);
        assert_eq!(result.evidence_used.len(), 1);
    }
}

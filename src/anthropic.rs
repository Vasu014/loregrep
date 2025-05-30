use std::time::Duration;
use serde::{Deserialize, Serialize};
use reqwest::Client;
use anyhow::{Result, Context};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    api_key: String,
    model: String,
    base_url: String,
    max_tokens: u32,
    temperature: f64,
    timeout_seconds: u64,
    client: Client,
}

impl AnthropicClient {
    pub fn new(
        api_key: String,
        model: Option<String>,
        max_tokens: Option<u32>,
        temperature: Option<f64>,
        timeout_seconds: Option<u64>,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds.unwrap_or(30)))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            api_key,
            model: model.unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string()),
            base_url: "https://api.anthropic.com".to_string(),
            max_tokens: max_tokens.unwrap_or(4096),
            temperature: temperature.unwrap_or(0.1),
            timeout_seconds: timeout_seconds.unwrap_or(30),
            client,
        }
    }

    pub fn get_api_key(&self) -> &str {
        &self.api_key
    }

    pub async fn send_message(&self, messages: Vec<Message>, tools: Vec<ToolSchema>) -> Result<ClaudeResponse> {
        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            messages,
            tools: if tools.is_empty() { None } else { Some(tools) },
        };

        let response = self
            .client
            .post(&format!("{}/v1/messages", self.base_url))
            .header("x-api-key", format!("{}", self.api_key))
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic API")?;

        if response.status().is_success() {
            let claude_response: ClaudeResponse = response
                .json()
                .await
                .context("Failed to parse Anthropic API response")?;
            Ok(claude_response)
        } else {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            anyhow::bail!("Anthropic API error: {}", error_text);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSchema>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeResponse {
    pub content: Vec<ContentBlock>,
    pub usage: Option<Usage>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { 
        id: String, 
        name: String, 
        input: serde_json::Value 
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ConversationContext {
    messages: Vec<Message>,
    max_history: usize,
    created_at: DateTime<Utc>,
}

impl ConversationContext {
    pub fn new(max_history: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_history,
            created_at: Utc::now(),
        }
    }

    pub fn add_message(&mut self, role: MessageRole, content: String) {
        self.messages.push(Message { role, content });
        
        // Keep only the last max_history messages
        if self.messages.len() > self.max_history {
            self.messages.drain(0..self.messages.len() - self.max_history);
        }
    }

    pub fn get_messages(&self) -> Vec<Message> {
        self.messages.clone()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn get_context_summary(&self) -> String {
        format!(
            "Conversation started at: {}\nMessage count: {}",
            self.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
            self.messages.len()
        )
    }
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self::new(20) // Default to last 20 messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_client_creation() {
        let client = AnthropicClient::new(
            "test-key".to_string(),
            Some("claude-3-5-sonnet-20241022".to_string()),
            Some(1000),
            Some(0.5),
            Some(60),
        );

        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model, "claude-3-5-sonnet-20241022");
        assert_eq!(client.max_tokens, 1000);
        assert_eq!(client.temperature, 0.5);
        assert_eq!(client.timeout_seconds, 60);
    }

    #[test]
    fn test_anthropic_client_defaults() {
        let client = AnthropicClient::new(
            "test-key".to_string(),
            None,
            None,
            None,
            None,
        );

        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model, "claude-3-5-sonnet-20241022");
        assert_eq!(client.max_tokens, 4096);
        assert_eq!(client.temperature, 0.1);
        assert_eq!(client.timeout_seconds, 30);
    }

    #[test]
    fn test_conversation_context() {
        let mut context = ConversationContext::new(3);
        
        context.add_message(MessageRole::User, "Hello".to_string());
        context.add_message(MessageRole::Assistant, "Hi there!".to_string());
        context.add_message(MessageRole::User, "How are you?".to_string());
        context.add_message(MessageRole::Assistant, "I'm doing well".to_string());
        
        assert_eq!(context.messages.len(), 4);
        
        // Add one more to trigger limit
        context.add_message(MessageRole::User, "Good to hear".to_string());
        assert_eq!(context.messages.len(), 3); // Should be limited to max_history
        
        // First message should be removed
        assert!(context.messages[0].content != "Hello");
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            role: MessageRole::User,
            content: "Test message".to_string(),
        };

        let serialized = serde_json::to_string(&message).unwrap();
        assert!(serialized.contains("\"role\":\"user\""));
        assert!(serialized.contains("\"content\":\"Test message\""));
    }

    #[test]
    fn test_tool_schema_creation() {
        let tool = ToolSchema {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"}
                }
            }),
        };

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool");
    }

    #[test]
    fn test_conversation_context_summary() {
        let mut context = ConversationContext::new(10);
        context.add_message(MessageRole::User, "Test".to_string());
        
        let summary = context.get_context_summary();
        assert!(summary.contains("Message count: 1"));
        assert!(summary.contains("Conversation started at:"));
    }

    #[test]
    fn test_conversation_context_clear() {
        let mut context = ConversationContext::new(10);
        context.add_message(MessageRole::User, "Test".to_string());
        assert_eq!(context.messages.len(), 1);
        
        context.clear();
        assert_eq!(context.messages.len(), 0);
    }
} 
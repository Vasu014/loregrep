use anyhow::Result;
use serde_json::json;

use crate::anthropic::{AnthropicClient, ConversationContext, MessageRole, Message, ContentBlock};
use crate::ai_tools::{LocalAnalysisTools, ToolResult};
use crate::config::CliConfig;

pub struct ConversationEngine {
    claude_client: AnthropicClient,
    local_tools: LocalAnalysisTools,
    context: ConversationContext,
    system_prompt: String,
}

impl ConversationEngine {
    pub fn new(
        claude_client: AnthropicClient,
        local_tools: LocalAnalysisTools,
        max_history: Option<usize>,
    ) -> Self {
        let system_prompt = Self::create_system_prompt();
        let context = ConversationContext::new(max_history.unwrap_or(20));

        Self {
            claude_client,
            local_tools,
            context,
            system_prompt,
        }
    }

    fn create_system_prompt() -> String {
        r#"You are an AI assistant specialized in code analysis and repository understanding. You have access to local analysis tools that can help you understand codebases, search for functions and structures, analyze files, and explore dependencies.

When users ask about code, you should:
1. Use the available tools to gather relevant information
2. Provide clear, accurate analysis based on the tool results
3. Explain code patterns, relationships, and potential issues
4. Suggest improvements or alternatives when appropriate
5. Ask clarifying questions if the user's request is ambiguous

Available tools:
- scan_repository: Scan a directory to analyze all code files
- search_functions: Search for functions by name pattern
- search_structs: Search for structs/classes by name pattern  
- analyze_file: Analyze a specific file in detail
- get_dependencies: Get import/export dependencies for a file
- find_callers: Find where a function is called
- get_repository_overview: Get high-level repository information

Always be helpful, accurate, and provide actionable insights about the code."#.to_string()
    }

    pub async fn process_user_message(&mut self, user_input: &str) -> Result<String> {
        // Add user message to conversation history
        self.context.add_message(MessageRole::User, user_input.to_string());

        // Prepare messages with system prompt
        let mut messages = vec![
            Message {
                role: MessageRole::User,
                content: self.system_prompt.clone(),
            }
        ];
        messages.extend(self.context.get_messages());

        // Get available tools
        let tools = self.local_tools.get_tool_schemas();

        // Send initial request to Claude
        let claude_response = self.claude_client.send_message(messages.clone(), tools).await?;

        // Process the response and handle any tool calls
        let final_response = self.process_claude_response(claude_response).await?;

        // Add final response to conversation history
        self.context.add_message(MessageRole::Assistant, final_response.clone());

        Ok(final_response)
    }

    async fn process_claude_response(&self, response: crate::anthropic::ClaudeResponse) -> Result<String> {
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        // Extract text and tool calls from response
        for content_block in response.content {
            match content_block {
                ContentBlock::Text { text } => {
                    text_parts.push(text);
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push((id, name, input));
                }
            }
        }

        // If there are no tool calls, just return the text
        if tool_calls.is_empty() {
            return Ok(text_parts.join("\n"));
        }

        // Execute tool calls
        let mut tool_results = Vec::new();
        for (id, tool_name, input) in tool_calls {
            let result = self.local_tools.execute_tool(&tool_name, input).await
                .unwrap_or_else(|e| ToolResult::error(format!("Tool execution failed: {}", e)));
            
            tool_results.push((id, tool_name, result));
        }

        // Prepare follow-up message with tool results
        let mut follow_up_messages = vec![
            Message {
                role: MessageRole::User,
                content: self.system_prompt.clone(),
            }
        ];
        follow_up_messages.extend(self.context.get_messages());

        // Add the assistant's response with tool calls
        let assistant_content = if text_parts.is_empty() {
            "I'll analyze this using the available tools.".to_string()
        } else {
            text_parts.join("\n")
        };
        follow_up_messages.push(Message {
            role: MessageRole::Assistant,
            content: assistant_content,
        });

        // Add tool results as user message
        let tool_results_content = self.format_tool_results(&tool_results);
        follow_up_messages.push(Message {
            role: MessageRole::User,
            content: format!("Tool results:\n\n{}", tool_results_content),
        });

        // Send follow-up request to get final response
        let final_response = self.claude_client.send_message(follow_up_messages, vec![]).await?;

        // Extract final text response
        let mut final_text = Vec::new();
        for content_block in final_response.content {
            if let ContentBlock::Text { text } = content_block {
                final_text.push(text);
            }
        }

        Ok(final_text.join("\n"))
    }

    fn format_tool_results(&self, tool_results: &[(String, String, ToolResult)]) -> String {
        let mut formatted = String::new();
        
        for (id, tool_name, result) in tool_results {
            formatted.push_str(&format!("**Tool: {}** (ID: {})\n", tool_name, id));
            
            if result.success {
                formatted.push_str("✅ **Status**: Success\n");
                formatted.push_str(&format!("**Result**:\n```json\n{}\n```\n\n", 
                    serde_json::to_string_pretty(&result.data).unwrap_or_else(|_| "Invalid JSON".to_string())));
            } else {
                formatted.push_str("❌ **Status**: Error\n");
                if let Some(error) = &result.error {
                    formatted.push_str(&format!("**Error**: {}\n", error));
                }
                if result.data != json!({}) {
                    formatted.push_str(&format!("**Data**:\n```json\n{}\n```\n", 
                        serde_json::to_string_pretty(&result.data).unwrap_or_else(|_| "Invalid JSON".to_string())));
                }
                formatted.push_str("\n");
            }
        }
        
        formatted
    }

    pub fn clear_conversation(&mut self) {
        self.context.clear();
    }

    pub fn get_conversation_summary(&self) -> String {
        self.context.get_context_summary()
    }

    pub fn get_message_count(&self) -> usize {
        self.context.get_messages().len()
    }

    pub fn has_api_key(&self) -> bool {
        !self.claude_client.get_api_key().is_empty()
    }
}

impl ConversationEngine {
    pub fn from_config_and_tools(
        config: &CliConfig,
        local_tools: LocalAnalysisTools,
    ) -> Result<Self> {
        let api_key = config.anthropic_api_key().clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not found in config or environment"))?;

        let claude_client = AnthropicClient::new(
            api_key,
            config.anthropic_model(),
            config.max_tokens(),
            config.temperature(),
            config.timeout_seconds(),
        );

        Ok(Self::new(
            claude_client,
            local_tools,
            config.conversation_memory(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::storage::memory::RepoMap;
    use crate::scanner::discovery::RepositoryScanner;
    use crate::analyzers::rust::RustAnalyzer;

    fn create_mock_conversation_engine() -> ConversationEngine {
        let claude_client = AnthropicClient::new(
            "test-key".to_string(),
            Some("claude-3-5-sonnet-20241022".to_string()),
            Some(1000),
            Some(0.1),
            Some(30),
        );

        let repo_map = Arc::new(RepoMap::new());
        
        // Create scanner with proper config
        let config = crate::config::FileScanningConfig {
            include_patterns: vec!["*.rs".to_string()],
            exclude_patterns: vec![],
            max_file_size: 1024 * 1024,
            follow_symlinks: false,
            max_depth: Some(10),
        };
        let scanner = RepositoryScanner::new(&config, None).unwrap();
        let rust_analyzer = RustAnalyzer::new().unwrap();
        let local_tools = LocalAnalysisTools::new(repo_map, scanner, rust_analyzer);

        ConversationEngine::new(claude_client, local_tools, Some(10))
    }

    #[test]
    fn test_conversation_engine_creation() {
        let engine = create_mock_conversation_engine();
        assert_eq!(engine.get_message_count(), 0);
        assert!(engine.has_api_key());
    }

    #[test]
    fn test_system_prompt_creation() {
        let prompt = ConversationEngine::create_system_prompt();
        assert!(prompt.contains("code analysis"));
        assert!(prompt.contains("scan_repository"));
        assert!(prompt.contains("search_functions"));
        assert!(prompt.len() > 100); // Should be substantial
    }

    #[test]
    fn test_conversation_summary() {
        let engine = create_mock_conversation_engine();
        let summary = engine.get_conversation_summary();
        assert!(summary.contains("Message count: 0"));
        assert!(summary.contains("Conversation started at:"));
    }

    #[test]
    fn test_clear_conversation() {
        let mut engine = create_mock_conversation_engine();
        
        // Add a message manually to context
        engine.context.add_message(MessageRole::User, "Test message".to_string());
        assert_eq!(engine.get_message_count(), 1);
        
        engine.clear_conversation();
        assert_eq!(engine.get_message_count(), 0);
    }

    #[test]
    fn test_format_tool_results() {
        let engine = create_mock_conversation_engine();
        
        let tool_results = vec![
            (
                "test-id-1".to_string(),
                "test_tool".to_string(),
                ToolResult::success(json!({"result": "success"}))
            ),
            (
                "test-id-2".to_string(),
                "error_tool".to_string(),
                ToolResult::error("Test error".to_string())
            ),
        ];

        let formatted = engine.format_tool_results(&tool_results);
        
        assert!(formatted.contains("✅ **Status**: Success"));
        assert!(formatted.contains("❌ **Status**: Error"));
        assert!(formatted.contains("test_tool"));
        assert!(formatted.contains("error_tool"));
        assert!(formatted.contains("Test error"));
    }

    #[test]
    fn test_has_api_key() {
        let engine = create_mock_conversation_engine();
        assert!(engine.has_api_key()); // test-key is not empty
        
        let claude_client = AnthropicClient::new(
            "".to_string(),
            None,
            None,
            None,
            None,
        );
        let repo_map = Arc::new(RepoMap::new());
        
        // Create scanner with proper config for empty key test
        let config = crate::config::FileScanningConfig {
            include_patterns: vec!["*.rs".to_string()],
            exclude_patterns: vec![],
            max_file_size: 1024 * 1024,
            follow_symlinks: false,
            max_depth: Some(10),
        };
        let scanner = RepositoryScanner::new(&config, None).unwrap();
        let rust_analyzer = RustAnalyzer::new().unwrap();
        let local_tools = LocalAnalysisTools::new(repo_map, scanner, rust_analyzer);
        
        let engine_no_key = ConversationEngine::new(claude_client, local_tools, None);
        assert!(!engine_no_key.has_api_key()); // empty key
    }

    // Note: We can't test the actual API calls without a real API key
    // and without making actual HTTP requests, but we can test the structure
    #[test]
    fn test_conversation_engine_structure() {
        let engine = create_mock_conversation_engine();
        
        // Test that all required components are present
        assert!(engine.system_prompt.len() > 0);
        assert_eq!(engine.get_message_count(), 0);
        
        // Test that we can get tool schemas
        let tools = engine.local_tools.get_tool_schemas();
        assert!(tools.len() > 0);
        assert!(tools.iter().any(|t| t.name == "scan_repository"));
    }
} 
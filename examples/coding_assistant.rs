// Full coding assistant integration example
// This example demonstrates how to build a complete coding assistant using LoreGrep

use loregrep::{LoreGrep, ToolSchema, ToolResult, Result as LoreGrepResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

/// A complete coding assistant that integrates LoreGrep for code analysis
pub struct CodingAssistant {
    loregrep: LoreGrep,
    project_path: String,
    conversation_history: Vec<ConversationMessage>,
    tool_schemas: Vec<ToolSchema>,
}

#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub parameters: Value,
    pub result: Option<ToolResult>,
}

impl CodingAssistant {
    /// Initialize the coding assistant for a project
    pub async fn new(project_path: &str) -> LoreGrepResult<Self> {
        println!("🤖 Initializing Coding Assistant for: {}", project_path);
        
        // Create LoreGrep instance with optimized settings for coding assistance
        let mut loregrep = LoreGrep::builder()
            .with_rust_analyzer()
            .max_files(5000)                    // Handle larger projects
            .cache_ttl(600)                     // 10-minute cache
            .include_patterns(vec![
                "**/*.rs".to_string(),
                "**/*.toml".to_string(),
                "**/*.md".to_string(),
            ])
            .exclude_patterns(vec![
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
                "**/node_modules/**".to_string(),
                "**/test-repos/**".to_string(),
                "**/.*".to_string(),              // Exclude hidden files
            ])
            .max_file_size(1024 * 1024)         // 1MB max file size
            .follow_symlinks(false)
            .build()?;

        // Initial project scan
        println!("📁 Scanning project...");
        let scan_result = loregrep.scan(project_path).await?;
        println!("   ✅ Indexed {} files with {} functions and {} structs", 
            scan_result.files_scanned, scan_result.functions_found, scan_result.structs_found);

        // Get available tools for LLM integration
        let tool_schemas = LoreGrep::get_tool_definitions();
        println!("   🔧 Loaded {} analysis tools", tool_schemas.len());

        Ok(Self {
            loregrep,
            project_path: project_path.to_string(),
            conversation_history: Vec::new(),
            tool_schemas,
        })
    }

    /// Get tool definitions for LLM integration
    pub fn get_available_tools(&self) -> &[ToolSchema] {
        &self.tool_schemas
    }

    /// Process a user query with optional tool usage
    pub async fn process_query(&mut self, user_query: &str) -> LoreGrepResult<String> {
        println!("\n💬 Processing query: \"{}\"", user_query);
        
        // Add user message to conversation history
        self.conversation_history.push(ConversationMessage {
            role: "user".to_string(),
            content: user_query.to_string(),
            tool_calls: Vec::new(),
        });

        // Analyze the query to determine which tools might be helpful
        let suggested_tools = self.analyze_query_intent(user_query);
        
        let mut response_parts = Vec::new();
        let mut tool_calls = Vec::new();

        // Execute suggested tools
        for (tool_name, params) in suggested_tools {
            println!("   🔧 Executing tool: {} with params: {}", tool_name, params);
            
            let tool_result = self.loregrep.execute_tool(&tool_name, params.clone()).await?;
            
            tool_calls.push(ToolCall {
                name: tool_name.clone(),
                parameters: params,
                result: Some(tool_result.clone()),
            });

            if tool_result.success {
                let formatted_result = self.format_tool_result(&tool_name, &tool_result);
                response_parts.push(formatted_result);
            } else {
                response_parts.push(format!("Tool {} failed: {:?}", tool_name, tool_result.error));
            }
        }

        // Generate a comprehensive response
        let response = if response_parts.is_empty() {
            self.generate_general_response(user_query)
        } else {
            format!("Based on the code analysis:\n\n{}", response_parts.join("\n\n"))
        };

        // Add assistant response to conversation history
        self.conversation_history.push(ConversationMessage {
            role: "assistant".to_string(),
            content: response.clone(),
            tool_calls,
        });

        Ok(response)
    }

    /// Refresh the project index (call when files change)
    pub async fn refresh_index(&mut self) -> LoreGrepResult<()> {
        println!("🔄 Refreshing project index...");
        let scan_result = self.loregrep.scan(&self.project_path).await?;
        println!("   ✅ Re-indexed {} files", scan_result.files_scanned);
        Ok(())
    }

    /// Get project statistics
    pub fn get_project_stats(&self) -> LoreGrepResult<HashMap<String, Value>> {
        let stats = self.loregrep.get_stats()?;
        let mut result = HashMap::new();
        
        result.insert("files_scanned".to_string(), json!(stats.files_scanned));
        result.insert("functions_found".to_string(), json!(stats.functions_found));
        result.insert("structs_found".to_string(), json!(stats.structs_found));
        result.insert("languages".to_string(), json!(stats.languages));
        result.insert("is_indexed".to_string(), json!(self.loregrep.is_scanned()));
        
        Ok(result)
    }

    /// Analyze user query to determine relevant tools
    fn analyze_query_intent(&self, query: &str) -> Vec<(String, Value)> {
        let query_lower = query.to_lowercase();
        let mut tools = Vec::new();

        // Function-related queries
        if query_lower.contains("function") || query_lower.contains("method") {
            if query_lower.contains("find") || query_lower.contains("search") || query_lower.contains("show") {
                // Extract potential pattern from query
                let pattern = self.extract_pattern_from_query(&query_lower, &["function", "method"]);
                tools.push(("search_functions".to_string(), json!({
                    "pattern": pattern,
                    "limit": 10
                })));
            }
        }

        // Struct-related queries
        if query_lower.contains("struct") || query_lower.contains("type") || query_lower.contains("class") {
            let pattern = self.extract_pattern_from_query(&query_lower, &["struct", "type", "class"]);
            tools.push(("search_structs".to_string(), json!({
                "pattern": pattern,
                "limit": 10
            })));
        }

        // File analysis queries
        if query_lower.contains("analyze") || query_lower.contains("what's in") {
            if let Some(file_path) = self.extract_file_path_from_query(query) {
                tools.push(("analyze_file".to_string(), json!({
                    "file_path": file_path,
                    "include_source": false
                })));
            }
        }

        // Dependency queries
        if query_lower.contains("import") || query_lower.contains("depend") || query_lower.contains("use") {
            if let Some(file_path) = self.extract_file_path_from_query(query) {
                tools.push(("get_dependencies".to_string(), json!({
                    "file_path": file_path
                })));
            }
        }

        // Caller/usage queries
        if query_lower.contains("caller") || query_lower.contains("used") || query_lower.contains("call") {
            let pattern = self.extract_pattern_from_query(&query_lower, &["caller", "call", "used"]);
            tools.push(("find_callers".to_string(), json!({
                "function_name": pattern,
                "limit": 10
            })));
        }

        // Repository structure queries
        if query_lower.contains("structure") || query_lower.contains("overview") || query_lower.contains("tree") {
            tools.push(("get_repository_tree".to_string(), json!({
                "include_file_details": true,
                "max_depth": 3
            })));
        }

        // If no specific intent detected, provide general search
        if tools.is_empty() && !query_lower.contains("how") && !query_lower.contains("what") {
            let general_pattern = query.split_whitespace().next().unwrap_or("main");
            tools.push(("search_functions".to_string(), json!({
                "pattern": general_pattern,
                "limit": 5
            })));
        }

        tools
    }

    /// Extract a search pattern from the user query
    fn extract_pattern_from_query(&self, query: &str, _keywords: &[&str]) -> String {
        // Simple pattern extraction - in a real implementation, you'd use more sophisticated NLP
        let words: Vec<&str> = query.split_whitespace().collect();
        
        // Look for quoted strings first
        if let Some(start) = query.find('"') {
            if let Some(end) = query.rfind('"') {
                if end > start {
                    return query[start+1..end].to_string();
                }
            }
        }
        
        // Look for patterns after certain keywords
        for (i, word) in words.iter().enumerate() {
            if matches!(*word, "named" | "called" | "like" | "containing") && i + 1 < words.len() {
                return words[i + 1].to_string();
            }
        }
        
        // Default to the last word that looks like an identifier
        words.iter()
            .rev()
            .find(|w| w.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .unwrap_or(&"main")
            .to_string()
    }

    /// Extract a file path from the user query
    fn extract_file_path_from_query(&self, query: &str) -> Option<String> {
        // Look for file extensions
        for word in query.split_whitespace() {
            if word.ends_with(".rs") || word.ends_with(".toml") {
                return Some(word.to_string());
            }
        }
        
        // Look for src/filename patterns
        if query.contains("src/") {
            if let Some(start) = query.find("src/") {
                let remainder = &query[start..];
                if let Some(end) = remainder.find(' ') {
                    return Some(remainder[..end].to_string());
                } else {
                    return Some(remainder.to_string());
                }
            }
        }
        
        None
    }

    /// Format tool results for human-readable output
    fn format_tool_result(&self, tool_name: &str, result: &ToolResult) -> String {
        match tool_name {
            "search_functions" => {
                if let Some(functions) = result.data.as_array() {
                    let mut output = format!("Found {} functions:", functions.len());
                    for func in functions.iter().take(5) {
                        if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                            let file = func.get("file_path").and_then(|v| v.as_str()).unwrap_or("unknown");
                            let line = func.get("line_number").and_then(|v| v.as_u64()).unwrap_or(0);
                            output.push_str(&format!("\n• {} ({}:{})", name, file, line));
                        }
                    }
                    output
                } else {
                    "No functions found.".to_string()
                }
            }
            "search_structs" => {
                if let Some(structs) = result.data.as_array() {
                    let mut output = format!("Found {} structs:", structs.len());
                    for struct_item in structs.iter().take(5) {
                        if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                            let file = struct_item.get("file_path").and_then(|v| v.as_str()).unwrap_or("unknown");
                            output.push_str(&format!("\n• {} ({})", name, file));
                        }
                    }
                    output
                } else {
                    "No structs found.".to_string()
                }
            }
            "analyze_file" => {
                let mut output = "File analysis:".to_string();
                if let Some(language) = result.data.get("language").and_then(|v| v.as_str()) {
                    output.push_str(&format!("\n• Language: {}", language));
                }
                if let Some(functions) = result.data.get("functions").and_then(|v| v.as_array()) {
                    output.push_str(&format!("\n• Functions: {}", functions.len()));
                }
                if let Some(structs) = result.data.get("structs").and_then(|v| v.as_array()) {
                    output.push_str(&format!("\n• Structs: {}", structs.len()));
                }
                output
            }
            _ => format!("Tool {} executed successfully.", tool_name),
        }
    }

    /// Generate a general response when no tools are applicable
    fn generate_general_response(&self, query: &str) -> String {
        if query.to_lowercase().contains("help") {
            "I can help you analyze your codebase! Ask me things like:\n\
            • 'Find functions named main'\n\
            • 'Show me all structs'\n\
            • 'Analyze src/lib.rs'\n\
            • 'What imports does main.rs have?'\n\
            • 'Who calls the parse function?'".to_string()
        } else {
            format!("I understand you're asking about: \"{}\"\n\
                    I've analyzed your codebase and can help with code exploration, \
                    function searches, struct analysis, and dependency tracking. \
                    Could you be more specific about what you'd like to know?", query)
        }
    }
}

#[tokio::main]
async fn main() -> LoreGrepResult<()> {
    println!("🚀 Coding Assistant Integration Example");
    println!("======================================\n");

    // Initialize the coding assistant
    let mut assistant = CodingAssistant::new(".").await?;

    // Show project statistics
    let stats = assistant.get_project_stats()?;
    println!("📊 Project Statistics:");
    for (key, value) in stats {
        println!("   • {}: {}", key, value);
    }

    // Example queries
    let example_queries = vec![
        "Find functions named new",
        "Show me all Config structs",
        "Analyze src/main.rs",
        "What's the structure of this project?",
        "Who calls the scan function?",
    ];

    println!("\n🎯 Running example queries:\n");

    for query in example_queries {
        println!("═".repeat(50));
        let response = assistant.process_query(query).await?;
        println!("Response:\n{}\n", response);
    }

    println!("═".repeat(50));
    println!("✨ Coding Assistant Integration Complete!");
    println!("\n💡 In a real application, you would:");
    println!("   1. Connect this to a chat interface");
    println!("   2. Add streaming responses for better UX");
    println!("   3. Implement context awareness across conversations");
    println!("   4. Add file watching for automatic re-indexing");
    println!("   5. Integrate with your preferred LLM (OpenAI, Anthropic, etc.)");

    Ok(())
}
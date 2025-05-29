# CLI Interface & Anthropic Claude Integration

## Overview
Interactive command-line interface providing developers with natural language access to repository analysis through Anthropic Claude, which uses local MCP tools for code understanding.

## Architecture

### Core Components
```rust
pub struct CliApplication {
    claude_client: AnthropicClient,
    mcp_client: McpClient,
    conversation_context: ConversationContext,
    output_formatter: OutputFormatter,
    command_history: CommandHistory,
    config: CliConfig,
    state: CliState,
}

pub struct AnthropicClient {
    api_key: String,
    model: String,
    base_url: String,
    max_tokens: u32,
    temperature: f64,
    timeout_seconds: u64,
}

pub struct McpClient {
    server_url: String,
    timeout_seconds: u64,
    available_tools: Vec<McpTool>,
}

pub struct CliConfig {
    pub api_key: String,
    pub model: String,
    pub auto_scan: bool,
    pub colors: bool,
    pub max_results: usize,
    pub conversation_memory: usize,
    pub include_context: bool,
}

pub struct CliState {
    pub current_repo: Option<String>,
    pub last_scan_time: Option<DateTime<Utc>>,
    pub working_directory: PathBuf,
    pub active_filters: FilterState,
}
```

## Conversation Flow

### Claude + MCP Integration
```rust
pub struct ConversationEngine {
    claude_client: AnthropicClient,
    mcp_client: McpClient,
    context: ConversationContext,
}

impl ConversationEngine {
    pub async fn process_user_message(&mut self, user_input: &str) -> Result<String> {
        // Add user message to conversation history
        self.context.add_message(MessageRole::User, user_input);
        
        // Prepare Claude request with MCP tools available
        let claude_request = ClaudeRequest {
            model: self.claude_client.model.clone(),
            max_tokens: self.claude_client.max_tokens,
            temperature: self.claude_client.temperature,
            messages: self.context.get_messages(),
            tools: self.get_available_mcp_tools(),
        };
        
        // Send to Claude API
        let claude_response = self.claude_client.send_request(claude_request).await?;
        
        // Process tool calls if any
        if let Some(tool_calls) = claude_response.tool_calls {
            let tool_results = self.execute_mcp_tools(tool_calls).await?;
            
            // Send tool results back to Claude for final response
            let final_response = self.claude_client.continue_conversation(tool_results).await?;
            
            self.context.add_message(MessageRole::Assistant, &final_response.content);
            Ok(final_response.content)
        } else {
            self.context.add_message(MessageRole::Assistant, &claude_response.content);
            Ok(claude_response.content)
        }
    }
    
    async fn execute_mcp_tools(&self, tool_calls: Vec<ToolCall>) -> Result<Vec<ToolResult>> {
        let mut results = Vec::new();
        
        for tool_call in tool_calls {
            let result = match tool_call.name.as_str() {
                "scan_repository" => {
                    self.mcp_client.call_tool("scan_repository", tool_call.arguments).await?
                }
                "search_functions" => {
                    self.mcp_client.call_tool("search_functions", tool_call.arguments).await?
                }
                "get_dependencies" => {
                    self.mcp_client.call_tool("get_dependencies", tool_call.arguments).await?
                }
                "analyze_file" => {
                    self.mcp_client.call_tool("analyze_file", tool_call.arguments).await?
                }
                _ => {
                    ToolResult::error(format!("Unknown tool: {}", tool_call.name))
                }
            };
            
            results.push(result);
        }
        
        Ok(results)
    }
}
```

### Anthropic Claude Client
```rust
impl AnthropicClient {
    pub async fn send_request(&self, request: ClaudeRequest) -> Result<ClaudeResponse> {
        let client = reqwest::Client::new();
        
        let response = client
            .post(&format!("{}/v1/messages", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .timeout(Duration::from_secs(self.timeout_seconds))
            .send()
            .await?;
            
        if response.status().is_success() {
            let claude_response: ClaudeResponse = response.json().await?;
            Ok(claude_response)
        } else {
            let error_text = response.text().await?;
            Err(CliError::AnthropicApiError(error_text))
        }
    }
}

#[derive(Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub messages: Vec<Message>,
    pub tools: Vec<McpToolSchema>,
}

#[derive(Deserialize)]
pub struct ClaudeResponse {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub usage: Option<Usage>,
}

#[derive(Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}
```

## MCP Tool Integration

### Available MCP Tools
```rust
impl ConversationEngine {
    fn get_available_mcp_tools(&self) -> Vec<McpToolSchema> {
        vec![
            McpToolSchema {
                name: "scan_repository".to_string(),
                description: "Scan repository for code analysis".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Repository path"},
                        "include_patterns": {"type": "array", "items": {"type": "string"}},
                        "exclude_patterns": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["path"]
                }),
            },
            McpToolSchema {
                name: "search_functions".to_string(),
                description: "Search for functions by pattern or name".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "Search pattern"},
                        "scope": {"type": "string", "enum": ["repository", "file", "directory"]},
                        "filters": {
                            "type": "object",
                            "properties": {
                                "visibility": {"type": "array", "items": {"enum": ["public", "private"]}},
                                "is_async": {"type": "boolean"},
                                "languages": {"type": "array", "items": {"type": "string"}}
                            }
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            McpToolSchema {
                name: "get_dependencies".to_string(),
                description: "Analyze file dependencies and call graphs".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {"type": "string", "description": "File to analyze"},
                        "direction": {"type": "string", "enum": ["incoming", "outgoing", "both"]},
                        "depth": {"type": "number", "description": "Analysis depth"}
                    },
                    "required": ["file_path"]
                }),
            },
            McpToolSchema {
                name: "analyze_file".to_string(),
                description: "Detailed analysis of a specific file".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {"type": "string", "description": "Path to file"},
                        "include_function_bodies": {"type": "boolean", "default": false}
                    },
                    "required": ["file_path"]
                }),
            },
        ]
    }
}
```

## Command System

### Direct Commands
```rust
pub enum CliCommand {
    Help,
    Scan(ScanOptions),
    Analyze(String),
    Dependencies(String),
    Map(MapOptions),
    Config,
    Exit,
}

impl CliApplication {
    pub async fn process_input(&mut self, input: &str) -> Result<()> {
        let trimmed = input.trim();
        
        // Handle direct commands
        if trimmed.starts_with('/') {
            self.handle_direct_command(trimmed).await?;
            return Ok(());
        }
        
        // Handle exit commands
        if trimmed == "exit" || trimmed == "quit" {
            println!("Goodbye!");
            std::process::exit(0);
        }
        
        // Process as natural language through Claude
        let response = self.conversation_engine.process_user_message(trimmed).await?;
        println!("{}", self.output_formatter.format_response(&response));
        
        Ok(())
    }
    
    async fn handle_direct_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command[1..].split_whitespace().collect();
        
        match parts.get(0) {
            Some(&"help") => self.show_help(),
            Some(&"scan") => {
                let path = parts.get(1).unwrap_or(&".");
                self.scan_repository(path).await?;
            }
            Some(&"analyze") => {
                if let Some(file_path) = parts.get(1) {
                    self.analyze_file(file_path).await?;
                } else {
                    println!("Usage: /analyze <file_path>");
                }
            }
            Some(&"deps") => {
                if let Some(file_path) = parts.get(1) {
                    self.show_dependencies(file_path).await?;
                } else {
                    println!("Usage: /deps <file_path>");
                }
            }
            Some(&"map") => {
                self.show_repository_map().await?;
            }
            Some(&"config") => {
                self.show_config();
            }
            _ => {
                println!("Unknown command. Type /help for available commands.");
            }
        }
        
        Ok(())
    }
}
```

## Conversation Context

### Context Management
```rust
pub struct ConversationContext {
    pub messages: Vec<Message>,
    pub current_repository: Option<String>,
    pub last_analysis_results: Option<serde_json::Value>,
    pub max_messages: usize,
    pub include_file_context: bool,
}

pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_results: Option<Vec<ToolResult>>,
}

pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl ConversationContext {
    pub fn add_message(&mut self, role: MessageRole, content: &str) {
        self.messages.push(Message {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_calls: None,
            tool_results: None,
        });
        
        // Trim conversation if too long
        if self.messages.len() > self.max_messages {
            // Keep system message + recent messages
            let keep_count = self.max_messages - 1;
            let system_messages: Vec<_> = self.messages
                .iter()
                .filter(|m| matches!(m.role, MessageRole::System))
                .cloned()
                .collect();
                
            let recent_messages: Vec<_> = self.messages
                .iter()
                .rev()
                .take(keep_count)
                .cloned()
                .collect();
                
            self.messages = system_messages;
            self.messages.extend(recent_messages.into_iter().rev());
        }
    }
    
    pub fn get_messages(&self) -> Vec<Message> {
        self.messages.clone()
    }
    
    pub fn add_system_context(&mut self, repository_info: &str) {
        let system_message = format!(
            "You are a code analysis assistant. You have access to a local code analysis server \
            with tools to search functions, analyze dependencies, and explore codebases. \
            Current repository: {}\n\n\
            Available tools:\n\
            - scan_repository: Scan repository for analysis\n\
            - search_functions: Search for functions by pattern\n\
            - get_dependencies: Analyze file dependencies\n\
            - analyze_file: Detailed file analysis\n\n\
            Always use these tools to provide accurate, data-driven responses about the codebase.",
            repository_info
        );
        
        self.add_message(MessageRole::System, &system_message);
    }
}
```

## Output Formatting

### Response Formatting
```rust
pub struct OutputFormatter {
    colors_enabled: bool,
    max_results_display: usize,
}

impl OutputFormatter {
    pub fn format_response(&self, response: &str) -> String {
        if self.colors_enabled {
            self.add_syntax_highlighting(response)
        } else {
            response.to_string()
        }
    }
    
    fn add_syntax_highlighting(&self, text: &str) -> String {
        // Add color codes for different elements
        text.replace("```rust", "\x1b[36m```rust\x1b[0m")
            .replace("```python", "\x1b[33m```python\x1b[0m")
            .replace("```typescript", "\x1b[34m```typescript\x1b[0m")
            .replace("•", "\x1b[32m•\x1b[0m")
            .replace("🔍", "\x1b[35m🔍\x1b[0m")
            .replace("📋", "\x1b[36m📋\x1b[0m")
            .replace("🔗", "\x1b[33m🔗\x1b[0m")
    }
}
```

## Application Lifecycle

### Initialization & Main Loop
```rust
impl CliApplication {
    pub async fn new(config: CliConfig) -> Result<Self> {
        // Initialize Anthropic client
        let claude_client = AnthropicClient {
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: "https://api.anthropic.com".to_string(),
            max_tokens: 4096,
            temperature: 0.1,
            timeout_seconds: 30,
        };
        
        // Initialize MCP client
        let mcp_client = McpClient::new("http://localhost:3000")?;
        
        // Test connections
        claude_client.test_connection().await?;
        mcp_client.test_connection().await?;
        
        let mut conversation_context = ConversationContext::new(config.conversation_memory);
        
        // Initialize repository context if auto_scan is enabled
        if config.auto_scan {
            let repo_info = mcp_client.call_tool("scan_repository", json!({"path": "."})).await?;
            conversation_context.add_system_context(&repo_info.content);
        }
        
        Ok(Self {
            claude_client,
            mcp_client,
            conversation_context,
            output_formatter: OutputFormatter::new(config.colors),
            command_history: CommandHistory::new(1000),
            config,
            state: CliState::default(),
        })
    }
    
    pub async fn run(&mut self) -> Result<()> {
        self.print_welcome_message().await?;
        
        loop {
            print!("\n> ");
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            let start_time = Instant::now();
            let result = self.process_input(&input).await;
            let duration = start_time.elapsed();
            
            match result {
                Ok(()) => {
                    self.command_history.add_entry(input.trim().to_string(), duration.as_millis() as u64, true);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    self.command_history.add_entry(input.trim().to_string(), duration.as_millis() as u64, false);
                }
            }
        }
    }
    
    async fn print_welcome_message(&self) -> Result<()> {
        println!("🔍 Loregrep CLI - Powered by Claude + Local Analysis");
        
        if self.config.auto_scan {
            println!("🔄 Scanning repository...");
            let scan_result = self.mcp_client.call_tool("scan_repository", json!({"path": "."})).await?;
            println!("{}", scan_result.content);
        }
        
        println!("\n💬 Ask me anything about your codebase!");
        println!("Examples:");
        println!("  • What functions handle authentication?");
        println!("  • Show me the User struct");
        println!("  • What would break if I change this function?");
        println!("  • /help for direct commands");
        
        Ok(())
    }
}
```

## Error Handling

### CLI-Specific Errors
```rust
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("Anthropic API error: {0}")]
    AnthropicApiError(String),
    
    #[error("MCP server error: {0}")]
    McpServerError(String),
    
    #[error("Connection timeout")]
    ConnectionTimeout,
    
    #[error("Invalid API key")]
    InvalidApiKey,
    
    #[error("Conversation context error: {0}")]
    ContextError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
```

## Configuration

### CLI Configuration
```toml
[cli]
# Anthropic Claude configuration
provider = "anthropic"
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-3-5-sonnet-20241022"
max_tokens = 4096
temperature = 0.1

# MCP server connection
mcp_server_url = "http://localhost:3000"
mcp_timeout_seconds = 30

# Interface settings
auto_scan = true
colors = true
max_results = 50
conversation_memory = 100
include_context = true

# Output formatting
response_style = "friendly"
include_examples = true
syntax_highlighting = true
```

## Testing

### Unit Tests
- Anthropic API client functionality
- MCP tool call execution
- Conversation context management
- Output formatting correctness

### Integration Tests
- End-to-end Claude + MCP workflow
- Error handling and recovery
- Performance under various loads
- API rate limiting behavior

### Mock Testing
```rust
pub struct MockAnthropicClient {
    responses: Vec<ClaudeResponse>,
    call_count: usize,
}

impl MockAnthropicClient {
    pub fn with_responses(responses: Vec<ClaudeResponse>) -> Self {
        Self { responses, call_count: 0 }
    }
    
    pub async fn send_request(&mut self, _request: ClaudeRequest) -> Result<ClaudeResponse> {
        if self.call_count < self.responses.len() {
            let response = self.responses[self.call_count].clone();
            self.call_count += 1;
            Ok(response)
        } else {
            Err(CliError::AnthropicApiError("No more mock responses".to_string()))
        }
    }
}
``` 
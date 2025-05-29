# MCP Server Interface

## Overview
Model Context Protocol (MCP) server implementation providing programmatic access to code analysis capabilities for LLM agents and other automated systems.

## MCP Server Architecture

### Server Configuration
```rust
pub struct McpServer {
    server_info: ServerInfo,
    capabilities: ServerCapabilities,
    tools: HashMap<String, Box<dyn McpTool>>,
    resources: HashMap<String, Box<dyn McpResource>>,
    analyzer: Arc<CodeAnalyzer>,
    database: Arc<DatabaseManager>,
    config: McpConfig,
}

pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
    pub description: String,
    pub author: String,
}

pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>, 
    pub prompts: Option<PromptsCapability>,
    pub logging: Option<LoggingCapability>,
}

pub struct McpConfig {
    pub host: String,
    pub port: u16,
    pub max_request_size_mb: usize,
    pub timeout_seconds: u64,
    pub rate_limiting: RateLimitConfig,
    pub authentication: Option<AuthConfig>,
}
```

## Tools Implementation

### Tool Registry
```rust
pub trait McpTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn execute(&self, arguments: serde_json::Value, context: &McpContext) -> Result<ToolResult>;
}

pub struct ToolResult {
    pub content: Vec<Content>,
    pub is_error: bool,
    pub metadata: Option<serde_json::Value>,
}

pub enum Content {
    Text { text: String },
    Resource { uri: String, mime_type: Option<String> },
    Json { data: serde_json::Value },
}
```

### Core Tools

#### 1. scan_repository
```rust
pub struct ScanRepositoryTool;

impl McpTool for ScanRepositoryTool {
    fn name(&self) -> &str { "scan_repository" }
    
    fn description(&self) -> &str {
        "Build complete repository analysis including functions, structs, and imports"
    }
    
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Repository root path to analyze"
                },
                "include_patterns": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Glob patterns for files to include",
                    "default": ["**/*.rs", "**/*.py", "**/*.ts", "**/*.js", "**/*.go"]
                },
                "exclude_patterns": {
                    "type": "array", 
                    "items": { "type": "string" },
                    "description": "Glob patterns for files to exclude",
                    "default": ["**/node_modules/**", "**/target/**", "**/.git/**"]
                },
                "max_file_size_mb": {
                    "type": "number",
                    "description": "Maximum file size to analyze",
                    "default": 10
                },
                "parallel_workers": {
                    "type": "number",
                    "description": "Number of parallel analysis workers",
                    "default": 4
                }
            },
            "required": ["path"]
        })
    }
    
    fn execute(&self, arguments: serde_json::Value, context: &McpContext) -> Result<ToolResult> {
        // Implementation here
    }
}
```

#### 2. analyze_file
```rust
pub struct AnalyzeFileTool;

impl McpTool for AnalyzeFileTool {
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to file for detailed analysis"
                },
                "include_function_bodies": {
                    "type": "boolean",
                    "description": "Include function implementation details",
                    "default": false
                },
                "include_dependencies": {
                    "type": "boolean", 
                    "description": "Include file dependency information",
                    "default": true
                }
            },
            "required": ["file_path"]
        })
    }
}
```

#### 3. search_functions
```rust
pub struct SearchFunctionsTool;

impl McpTool for SearchFunctionsTool {
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (regex or text match)"
                },
                "scope": {
                    "type": "string",
                    "enum": ["repository", "file", "directory"],
                    "description": "Search scope",
                    "default": "repository"
                },
                "scope_path": {
                    "type": "string",
                    "description": "Path for file/directory scope"
                },
                "filters": {
                    "type": "object",
                    "properties": {
                        "visibility": {
                            "type": "array",
                            "items": { "enum": ["public", "private", "protected"] }
                        },
                        "is_async": { "type": "boolean" },
                        "is_method": { "type": "boolean" },
                        "languages": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "min_parameter_count": { "type": "number" },
                        "max_parameter_count": { "type": "number" }
                    }
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of results",
                    "default": 50
                },
                "include_signature": {
                    "type": "boolean",
                    "description": "Include full function signatures",
                    "default": true
                }
            },
            "required": ["pattern"]
        })
    }
}
```

#### 4. search_structs
```rust
pub struct SearchStructsTool;

impl McpTool for SearchStructsTool {
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Search pattern for struct names"
                },
                "include_fields": {
                    "type": "boolean",
                    "description": "Include struct field information",
                    "default": true
                },
                "include_methods": {
                    "type": "boolean",
                    "description": "Include associated methods",
                    "default": false
                },
                "struct_types": {
                    "type": "array",
                    "items": { "enum": ["struct", "class", "interface", "enum", "trait"] }
                }
            },
            "required": ["pattern"]
        })
    }
}
```

#### 5. get_dependencies
```rust
pub struct GetDependenciesTool;

impl McpTool for GetDependenciesTool {
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "File to analyze dependencies for"
                },
                "direction": {
                    "type": "string",
                    "enum": ["incoming", "outgoing", "both"],
                    "description": "Dependency direction",
                    "default": "both"
                },
                "depth": {
                    "type": "number",
                    "description": "Maximum dependency depth",
                    "default": 3
                },
                "include_external": {
                    "type": "boolean",
                    "description": "Include external dependencies",
                    "default": false
                }
            },
            "required": ["file_path"]
        })
    }
}
```

#### 6. update_file_index
```rust
pub struct UpdateFileIndexTool;

impl McpTool for UpdateFileIndexTool {
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "File path to re-index after changes"
                },
                "force_reanalysis": {
                    "type": "boolean",
                    "description": "Force re-analysis even if file hash unchanged",
                    "default": false
                }
            },
            "required": ["file_path"]
        })
    }
}
```

#### 7. get_function_calls
```rust
pub struct GetFunctionCallsTool;

impl McpTool for GetFunctionCallsTool {
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "function_name": {
                    "type": "string",
                    "description": "Function name to find calls for"
                },
                "qualified_name": {
                    "type": "string",
                    "description": "Fully qualified function name"
                },
                "scope": {
                    "type": "string",
                    "enum": ["repository", "file", "directory"],
                    "default": "repository"
                },
                "include_call_context": {
                    "type": "boolean",
                    "description": "Include surrounding code context",
                    "default": false
                }
            }
        })
    }
}
```

#### 8. get_repo_metrics
```rust
pub struct GetRepoMetricsTool;

impl McpTool for GetRepoMetricsTool {
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "include_complexity": {
                    "type": "boolean",
                    "description": "Include complexity analysis",
                    "default": false
                },
                "group_by_language": {
                    "type": "boolean",
                    "description": "Group metrics by programming language",
                    "default": true
                }
            }
        })
    }
}
```

## Resources Implementation

### Resource Registry
```rust
pub trait McpResource: Send + Sync {
    fn uri_pattern(&self) -> &str;
    fn description(&self) -> &str;
    fn mime_type(&self) -> &str;
    fn get_content(&self, uri: &str, context: &McpContext) -> Result<ResourceContent>;
    fn list_resources(&self, context: &McpContext) -> Result<Vec<ResourceInfo>>;
}

pub struct ResourceContent {
    pub content: String,
    pub mime_type: String,
    pub metadata: Option<serde_json::Value>,
}

pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: String,
}
```

### Core Resources

#### 1. Repository Map
```rust
pub struct RepoMapResource;

impl McpResource for RepoMapResource {
    fn uri_pattern(&self) -> &str { "mcp://repo/map" }
    
    fn get_content(&self, uri: &str, context: &McpContext) -> Result<ResourceContent> {
        let params = parse_uri_params(uri);
        let depth = params.get("depth").unwrap_or(&"3".to_string()).parse().unwrap_or(3);
        let show_hidden = params.get("show_hidden").unwrap_or(&"false".to_string()) == "true";
        
        let repo_map = context.analyzer.generate_repository_map(depth, show_hidden)?;
        
        Ok(ResourceContent {
            content: repo_map,
            mime_type: "text/plain".to_string(),
            metadata: Some(json!({
                "depth": depth,
                "show_hidden": show_hidden,
                "generated_at": chrono::Utc::now()
            }))
        })
    }
}
```

#### 2. File Listings
```rust
pub struct FilesResource;

impl McpResource for FilesResource {
    fn uri_pattern(&self) -> &str { "mcp://repo/files" }
    
    fn get_content(&self, uri: &str, context: &McpContext) -> Result<ResourceContent> {
        let params = parse_uri_params(uri);
        let language_filter = params.get("language_filter");
        let pattern = params.get("pattern");
        
        let files = context.database.get_analyzed_files(language_filter, pattern)?;
        
        let content = serde_json::to_string_pretty(&json!({
            "files": files,
            "total_count": files.len(),
            "filters": {
                "language": language_filter,
                "pattern": pattern
            }
        }))?;
        
        Ok(ResourceContent {
            content,
            mime_type: "application/json".to_string(),
            metadata: None
        })
    }
}
```

#### 3. File Analysis
```rust
pub struct FileAnalysisResource;

impl McpResource for FileAnalysisResource {
    fn uri_pattern(&self) -> &str { "mcp://repo/file/{path}" }
    
    fn get_content(&self, uri: &str, context: &McpContext) -> Result<ResourceContent> {
        let file_path = extract_path_from_uri(uri)?;
        let analysis = context.analyzer.analyze_file(&file_path)?;
        
        let content = serde_json::to_string_pretty(&analysis)?;
        
        Ok(ResourceContent {
            content,
            mime_type: "application/json".to_string(),
            metadata: Some(json!({
                "file_path": file_path,
                "analysis_timestamp": chrono::Utc::now()
            }))
        })
    }
}
```

#### 4. Function Details
```rust
pub struct FunctionResource;

impl McpResource for FunctionResource {
    fn uri_pattern(&self) -> &str { "mcp://repo/function/{name}" }
    
    fn get_content(&self, uri: &str, context: &McpContext) -> Result<ResourceContent> {
        let function_name = extract_name_from_uri(uri)?;
        let params = parse_uri_params(uri);
        let scope = params.get("scope");
        
        let functions = context.database.search_functions_by_name(&function_name, scope)?;
        
        let content = serde_json::to_string_pretty(&json!({
            "function_name": function_name,
            "matches": functions,
            "total_matches": functions.len()
        }))?;
        
        Ok(ResourceContent {
            content,
            mime_type: "application/json".to_string(),
            metadata: None
        })
    }
}
```

#### 5. Dependency Graph
```rust
pub struct DependencyGraphResource;

impl McpResource for DependencyGraphResource {
    fn uri_pattern(&self) -> &str { "mcp://repo/dependencies/{path}" }
    
    fn get_content(&self, uri: &str, context: &McpContext) -> Result<ResourceContent> {
        let file_path = extract_path_from_uri(uri)?;
        let params = parse_uri_params(uri);
        let depth = params.get("depth").unwrap_or(&"3".to_string()).parse().unwrap_or(3);
        
        let dependencies = context.analyzer.get_dependency_graph(&file_path, depth)?;
        
        let content = serde_json::to_string_pretty(&dependencies)?;
        
        Ok(ResourceContent {
            content,
            mime_type: "application/json".to_string(),
            metadata: Some(json!({
                "file_path": file_path,
                "depth": depth
            }))
        })
    }
}
```

## Subscriptions & Events

### Event System
```rust
pub struct McpEventManager {
    subscribers: HashMap<String, Vec<EventSubscriber>>,
    event_queue: Arc<Mutex<VecDeque<McpEvent>>>,
}

pub struct EventSubscriber {
    pub client_id: String,
    pub event_type: String,
    pub filters: Option<serde_json::Value>,
}

pub enum McpEvent {
    FileAnalyzed {
        file_path: String,
        functions_count: usize,
        structs_count: usize,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    AnalysisError {
        file_path: String,
        error: String,
        language: String,
        timestamp: DateTime<Utc>,
    },
    RepositoryUpdated {
        repository_path: String,
        files_count: usize,
        total_functions: usize,
        languages: Vec<String>,
        timestamp: DateTime<Utc>,
    },
}
```

### Event Broadcasting
```rust
impl McpEventManager {
    pub fn subscribe(&mut self, subscriber: EventSubscriber) -> Result<()> {
        self.subscribers
            .entry(subscriber.event_type.clone())
            .or_insert_with(Vec::new)
            .push(subscriber);
        Ok(())
    }
    
    pub fn publish_event(&self, event: McpEvent) -> Result<()> {
        // Add to queue and notify subscribers
        let mut queue = self.event_queue.lock().unwrap();
        queue.push_back(event);
        // Trigger notification to subscribers
        Ok(())
    }
    
    pub fn get_pending_events(&self, client_id: &str) -> Result<Vec<McpEvent>> {
        // Retrieve events for specific client
        Ok(vec![])
    }
}
```

## Request/Response Handling

### Request Processing
```rust
pub struct McpRequestHandler {
    tools: HashMap<String, Box<dyn McpTool>>,
    resources: HashMap<String, Box<dyn McpResource>>,
    rate_limiter: Arc<RateLimiter>,
}

impl McpRequestHandler {
    pub async fn handle_request(&self, request: McpRequest) -> Result<McpResponse> {
        // Rate limiting check
        self.rate_limiter.check_rate_limit(&request.client_id)?;
        
        match request.method.as_str() {
            "tools/call" => self.handle_tool_call(request).await,
            "resources/read" => self.handle_resource_read(request).await,
            "resources/list" => self.handle_resource_list(request).await,
            "notifications/subscribe" => self.handle_subscription(request).await,
            _ => Err(McpError::UnsupportedMethod(request.method))
        }
    }
    
    async fn handle_tool_call(&self, request: McpRequest) -> Result<McpResponse> {
        let tool_name = request.params.get("name")
            .and_then(|v| v.as_str())
            .ok_or(McpError::MissingParameter("name"))?;
            
        let arguments = request.params.get("arguments")
            .cloned()
            .unwrap_or(json!({}));
            
        let tool = self.tools.get(tool_name)
            .ok_or(McpError::UnknownTool(tool_name.to_string()))?;
            
        let context = McpContext::from_request(&request)?;
        let result = tool.execute(arguments, &context)?;
        
        Ok(McpResponse {
            id: request.id,
            result: Some(json!(result)),
            error: None,
        })
    }
}
```

### Error Handling
```rust
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Unsupported method: {0}")]
    UnsupportedMethod(String),
    
    #[error("Unknown tool: {0}")]
    UnknownTool(String),
    
    #[error("Missing required parameter: {0}")]
    MissingParameter(&'static str),
    
    #[error("Invalid parameter value: {0}")]
    InvalidParameter(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Analysis error: {0}")]
    AnalysisError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
}
```

## Performance & Security

### Rate Limiting
```rust
pub struct RateLimiter {
    requests_per_minute: HashMap<String, VecDeque<Instant>>,
    max_requests_per_minute: usize,
    max_concurrent_requests: usize,
    active_requests: Arc<AtomicUsize>,
}

impl RateLimiter {
    pub fn check_rate_limit(&self, client_id: &str) -> Result<()> {
        // Implement sliding window rate limiting
        Ok(())
    }
}
```

### Request Validation
```rust
pub struct RequestValidator;

impl RequestValidator {
    pub fn validate_tool_request(&self, request: &McpRequest) -> Result<()> {
        // Validate request structure and parameters
        Ok(())
    }
    
    pub fn sanitize_file_path(&self, path: &str) -> Result<String> {
        // Prevent path traversal attacks
        Ok(path.to_string())
    }
}
```

## Testing

### Unit Tests
- Individual tool functionality
- Resource content generation
- Event publishing/subscription
- Error handling scenarios

### Integration Tests  
- Full MCP protocol compliance
- Client-server communication
- Concurrent request handling
- Performance under load

### Mock Framework
```rust
pub struct MockMcpClient {
    server_url: String,
    capabilities: Vec<String>,
}

impl MockMcpClient {
    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<ToolResult> {
        // Mock tool calls for testing
        Ok(ToolResult {
            content: vec![Content::Text { text: "mock result".to_string() }],
            is_error: false,
            metadata: None,
        })
    }
}
``` 
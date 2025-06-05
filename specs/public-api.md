# LoreGrep Public API Specification

## Overview

LoreGrep provides an in-memory code repository analysis library designed for integration into coding assistants and LLM-powered development tools. The library offers a tool-based interface that can be easily integrated into any AI assistant's tool calling system.

## Core Design Principles

1. **Tool-Based Interface**: All functionality exposed through LLM-compatible tool definitions
2. **Host-Managed Scanning**: Repository scanning is controlled by the host application, not the LLM
3. **Language Agnostic**: Extensible architecture supporting multiple programming languages
4. **Memory Efficient**: Fast in-memory indexing optimized for code analysis
5. **Type Safe**: Strong typing with comprehensive error handling
6. **File Path Tracking**: Every code element includes complete file path information for cross-file reference tracking

### 🎯 File Path Storage Enhancement (v0.3.3+)

**All data structures now include file path information:**
- `FunctionSignature` includes `file_path: String`
- `StructSignature` includes `file_path: String`
- `ImportStatement` includes `file_path: String`
- `ExportStatement` includes `file_path: String`
- `FunctionCall` includes `file_path: String`

This enables AI coding assistants to:
- Track function definitions vs. call sites across files
- Perform accurate impact analysis for refactoring
- Understand module boundaries and dependencies
- Generate context-aware code suggestions

## Public API Structure

### Main Entry Point

```rust
/// The main struct for interacting with LoreGrep
pub struct LoreGrep {
    // Internal implementation details hidden
}

impl LoreGrep {
    /// Create a new builder for configuring LoreGrep
    pub fn builder() -> LoreGrepBuilder;
    
    /// Scan a repository and build the in-memory index
    /// This should be called by the host application, not exposed as a tool
    pub async fn scan(&mut self, path: &str) -> Result<ScanResult>;
    
    /// Get tool definitions for adding to LLM system prompts
    /// Returns JSON Schema compatible tool definitions
    pub fn get_tool_definitions() -> Vec<ToolSchema>;
    
    /// Execute a tool call from the LLM
    /// Takes tool name and parameters, returns JSON result
    pub async fn execute_tool(&self, name: &str, params: Value) -> Result<ToolResult>;
}
```

### Builder Pattern

```rust
pub struct LoreGrepBuilder {
    // Configuration options
}

impl LoreGrepBuilder {
    /// Add Rust language analyzer (enabled by default)
    pub fn with_rust_analyzer(self) -> Self;
    
    /// Add Python language analyzer (future)
    pub fn with_python_analyzer(self) -> Self;
    
    /// Add TypeScript/JavaScript analyzer (future)
    pub fn with_typescript_analyzer(self) -> Self;
    
    /// Add Go language analyzer (future)
    pub fn with_go_analyzer(self) -> Self;
    
    /// Set maximum number of files to index
    pub fn max_files(self, limit: usize) -> Self;
    
    /// Set cache TTL for query results
    pub fn cache_ttl(self, seconds: u64) -> Self;
    
    /// Build the LoreGrep instance
    pub fn build(self) -> Result<LoreGrep>;
}
```

### Tool Definitions

The library exposes the following tools for LLM consumption:

#### 1. search_functions
Search for functions by name pattern or regex across the analyzed codebase.

```json
{
  "name": "search_functions",
  "description": "Search for functions by name pattern or regex across the analyzed codebase",
  "input_schema": {
    "type": "object",
    "properties": {
      "pattern": {
        "type": "string",
        "description": "Search pattern or regex to match function names"
      },
      "limit": {
        "type": "integer",
        "description": "Maximum number of results to return",
        "default": 20
      },
      "language": {
        "type": "string",
        "description": "Filter by programming language (optional)"
      }
    },
    "required": ["pattern"]
  }
}
```

#### 2. search_structs
Search for structs/classes by name pattern across the analyzed codebase.

```json
{
  "name": "search_structs",
  "description": "Search for structs/classes by name pattern across the analyzed codebase",
  "input_schema": {
    "type": "object",
    "properties": {
      "pattern": {
        "type": "string",
        "description": "Search pattern or regex to match struct/class names"
      },
      "limit": {
        "type": "integer",
        "description": "Maximum number of results to return",
        "default": 20
      },
      "language": {
        "type": "string",
        "description": "Filter by programming language (optional)"
      }
    },
    "required": ["pattern"]
  }
}
```

#### 3. analyze_file
Analyze a specific file to extract its functions, structs, imports, and other code elements.

```json
{
  "name": "analyze_file",
  "description": "Analyze a specific file to extract its functions, structs, imports, and other code elements",
  "input_schema": {
    "type": "object",
    "properties": {
      "file_path": {
        "type": "string",
        "description": "Path to the file to analyze"
      },
      "include_content": {
        "type": "boolean",
        "description": "Whether to include file content in the response",
        "default": false
      }
    },
    "required": ["file_path"]
  }
}
```

#### 4. get_dependencies
Get import/export dependencies for a file or analyze dependency relationships.

```json
{
  "name": "get_dependencies",
  "description": "Get import/export dependencies for a file or analyze dependency relationships",
  "input_schema": {
    "type": "object",
    "properties": {
      "file_path": {
        "type": "string",
        "description": "Path to the file to analyze dependencies for"
      }
    },
    "required": ["file_path"]
  }
}
```

#### 5. find_callers
Find all locations where a specific function is called across the codebase.

```json
{
  "name": "find_callers",
  "description": "Find all locations where a specific function is called across the codebase",
  "input_schema": {
    "type": "object",
    "properties": {
      "function_name": {
        "type": "string",
        "description": "Name of the function to find callers for"
      },
      "limit": {
        "type": "integer",
        "description": "Maximum number of results to return",
        "default": 50
      }
    },
    "required": ["function_name"]
  }
}
```

#### 6. get_repository_tree
Get complete repository information including hierarchical directory structure, file details, statistics, and metadata.

```json
{
  "name": "get_repository_tree",
  "description": "Get complete repository information including hierarchical directory structure, file details, statistics, and metadata",
  "input_schema": {
    "type": "object",
    "properties": {
      "include_file_details": {
        "type": "boolean",
        "description": "Whether to include detailed file skeletons with functions and structs",
        "default": true
      },
      "max_depth": {
        "type": "integer",
        "description": "Maximum directory depth to include (0 for unlimited)",
        "default": 0
      }
    }
  }
}
```

## Tool Output Examples with File Path Information

All tools now return results that include complete file path information for better cross-file reference tracking.

### search_functions Example Output

```json
{
  "status": "success",
  "pattern": "config",
  "results": [
    {
      "name": "parse_config",
      "file_path": "src/config.rs",
      "start_line": 45,
      "end_line": 52,
      "signature": "pub fn parse_config(path: &str) -> Result<Config>",
      "is_public": true,
      "is_async": false,
      "parameters": [
        {"name": "path", "param_type": "&str", "is_mutable": false}
      ],
      "return_type": "Result<Config>"
    },
    {
      "name": "load_config",
      "file_path": "src/utils/loader.rs",
      "start_line": 12,
      "end_line": 18,
      "signature": "fn load_config() -> Config",
      "is_public": false,
      "is_async": false,
      "parameters": [],
      "return_type": "Config"
    }
  ],
  "count": 2
}
```

### search_structs Example Output

```json
{
  "status": "success", 
  "pattern": "Config",
  "results": [
    {
      "name": "Config",
      "file_path": "src/config.rs",
      "start_line": 12,
      "end_line": 16,
      "is_public": true,
      "is_tuple_struct": false,
      "fields": [
        {"name": "port", "field_type": "u16", "is_public": true},
        {"name": "host", "field_type": "String", "is_public": true},
        {"name": "debug", "field_type": "bool", "is_public": false}
      ],
      "generics": []
    },
    {
      "name": "DatabaseConfig",
      "file_path": "src/db/mod.rs",
      "start_line": 8,
      "end_line": 12,
      "is_public": true,
      "is_tuple_struct": false,
      "fields": [
        {"name": "url", "field_type": "String", "is_public": false},
        {"name": "max_connections", "field_type": "usize", "is_public": false}
      ],
      "generics": []
    }
  ],
  "count": 2
}
```

### find_callers Example Output

```json
{
  "status": "success",
  "function_name": "parse_config",
  "callers": [
    {
      "function_name": "parse_config",
      "file_path": "src/main.rs",
      "line_number": 23,
      "column": 17,
      "is_method_call": false,
      "receiver_type": null
    },
    {
      "function_name": "parse_config",
      "file_path": "src/utils/loader.rs",
      "line_number": 45,
      "column": 12,
      "is_method_call": false,
      "receiver_type": null
    },
    {
      "function_name": "parse_config",
      "file_path": "tests/config_tests.rs",
      "line_number": 67,
      "column": 8,
      "is_method_call": false,
      "receiver_type": null
    }
  ],
  "count": 3
}
```

### analyze_file Example Output

```json
{
  "status": "success",
  "file_path": "src/config.rs",
  "language": "rust",
  "functions": [
    {
      "name": "parse_config",
      "file_path": "src/config.rs",
      "start_line": 45,
      "end_line": 52,
      "signature": "pub fn parse_config(path: &str) -> Result<Config>",
      "is_public": true,
      "is_async": false
    }
  ],
  "structs": [
    {
      "name": "Config",
      "file_path": "src/config.rs",
      "start_line": 12,
      "end_line": 16,
      "is_public": true,
      "fields": [{"name": "port", "field_type": "u16", "is_public": true}]
    }
  ],
  "imports": [
    {
      "module_path": "std::fs",
      "file_path": "src/config.rs",
      "line_number": 1,
      "imported_items": ["read_to_string"],
      "is_external": true,
      "is_glob": false
    }
  ],
  "exports": [
    {
      "exported_item": "Config",
      "file_path": "src/config.rs", 
      "line_number": 16,
      "is_public": true,
      "alias": null
    }
  ]
}
```

### get_dependencies Example Output

```json
{
  "status": "success",
  "file_path": "src/main.rs",
  "imports": [
    {
      "module_path": "./config",
      "file_path": "src/main.rs",
      "line_number": 2,
      "imported_items": ["Config"],
      "is_external": false,
      "is_glob": false
    },
    {
      "module_path": "std::env",
      "file_path": "src/main.rs",
      "line_number": 1,
      "imported_items": ["args"],
      "is_external": true,
      "is_glob": false
    }
  ],
  "exports": [
    {
      "exported_item": "main",
      "file_path": "src/main.rs",
      "line_number": 8,
      "is_public": true,
      "alias": null
    }
  ]
}
```

### Core Types

```rust
/// Tool definition for LLM system prompts
#[derive(Serialize, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

/// Result of repository scanning
#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub functions_found: usize,
    pub structs_found: usize,
    pub duration_ms: u64,
    pub languages: Vec<String>,
}

/// Main error type
#[derive(Debug, thiserror::Error)]
pub enum LoreGrepError {
    #[error("Repository not scanned")]
    NotScanned,
    
    #[error("Analysis error: {0}")]
    AnalysisError(String),
    
    #[error("Tool execution error: {0}")]
    ToolError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LoreGrepError>;
```

## Usage Examples

### Basic Integration

```rust
use loregrep::{LoreGrep, ToolSchema};
use serde_json::json;

// Initialize LoreGrep
let mut loregrep = LoreGrep::builder()
    .with_rust_analyzer()
    .max_files(10000)
    .build()?;

// Scan repository (host-managed)
let scan_result = loregrep.scan("/path/to/repo").await?;
println!("Scanned {} files", scan_result.files_scanned);

// Get tool definitions for LLM
let tools = loregrep.get_tool_definitions();
let tools_json = serde_json::to_string_pretty(&tools)?;

// Add to LLM system prompt
let system_prompt = format!(
    "You are a coding assistant with access to these tools:\n{}",
    tools_json
);

// Execute tool calls from LLM
let result = loregrep.execute_tool("search_functions", json!({
    "pattern": "handle_.*",
    "limit": 10
})).await?;
```

### Integration with Coding Assistant (Leveraging File Path Information)

```rust
use std::collections::{HashMap, HashSet};
use serde_json::{json, Value};

pub struct CodingAssistant {
    loregrep: LoreGrep,
    llm_client: LlmClient,
}

impl CodingAssistant {
    pub async fn initialize(project_path: &str) -> Result<Self> {
        // Initialize and scan
        let mut loregrep = LoreGrep::builder().build()?;
        loregrep.scan(project_path).await?;
        
        Ok(Self {
            loregrep,
            llm_client: LlmClient::new(),
        })
    }
    
    pub async fn handle_llm_tool_call(&self, call: ToolCall) -> Result<Value> {
        // Execute tool and return result with enhanced file path context
        let result = self.loregrep.execute_tool(&call.name, call.params).await?;
        Ok(serde_json::to_value(result)?)
    }
    
    /// Analyze cross-file impact using file path information
    pub async fn analyze_refactoring_impact(&self, function_name: &str) -> Result<Value> {
        // Find where function is defined
        let definitions = self.loregrep.execute_tool("search_functions", json!({
            "pattern": function_name,
            "limit": 1
        })).await?;
        
        // Find where it's called
        let callers = self.loregrep.execute_tool("find_callers", json!({
            "function_name": function_name
        })).await?;
        
        // Parse results to extract file paths
        let def_data: Value = serde_json::from_str(&definitions.data.to_string())?;
        let caller_data: Value = serde_json::from_str(&callers.data.to_string())?;
        
        let mut affected_files = HashSet::new();
        
        // Add definition file
        if let Some(functions) = def_data["results"].as_array() {
            for func in functions {
                if let Some(file_path) = func["file_path"].as_str() {
                    affected_files.insert(file_path.to_string());
                }
            }
        }
        
        // Add caller files
        if let Some(callers) = caller_data["callers"].as_array() {
            for caller in callers {
                if let Some(file_path) = caller["file_path"].as_str() {
                    affected_files.insert(file_path.to_string());
                }
            }
        }
        
        Ok(json!({
            "function_name": function_name,
            "affected_files": affected_files.into_iter().collect::<Vec<_>>(),
            "impact_analysis": {
                "files_to_review": affected_files.len(),
                "requires_testing": true,
                "recommendation": "Review all affected files before making changes"
            }
        }))
    }
    
    /// Group functions by module using file path information
    pub async fn analyze_module_organization(&self, pattern: &str) -> Result<Value> {
        let functions = self.loregrep.execute_tool("search_functions", json!({
            "pattern": pattern,
            "limit": 100
        })).await?;
        
        let func_data: Value = serde_json::from_str(&functions.data.to_string())?;
        let mut modules: HashMap<String, Vec<String>> = HashMap::new();
        
        if let Some(functions) = func_data["results"].as_array() {
            for func in functions {
                if let (Some(name), Some(file_path)) = (
                    func["name"].as_str(),
                    func["file_path"].as_str()
                ) {
                    // Extract module path from file path (e.g., "src/auth/mod.rs" -> "auth")
                    let module = extract_module_name(file_path);
                    modules.entry(module).or_default().push(name.to_string());
                }
            }
        }
        
        Ok(json!({
            "pattern": pattern,
            "modules": modules,
            "analysis": {
                "module_count": modules.len(),
                "suggestion": "Consider organizing related functions in the same module"
            }
        }))
    }
    
    pub async fn refresh_index(&mut self, path: &str) -> Result<()> {
        // Rescan when files change
        self.loregrep.scan(path).await?;
        Ok(())
    }
}

// Helper function to extract module name from file path
fn extract_module_name(file_path: &str) -> String {
    if let Some(captures) = regex::Regex::new(r"src/([^/]+)/")
        .unwrap()
        .captures(file_path) 
    {
        captures[1].to_string()
    } else if file_path.starts_with("src/") {
        "root".to_string()
    } else {
        "unknown".to_string()
    }
}
```

### File Watching Strategy

```rust
// Example: Rescan on file changes
use notify::{Watcher, RecursiveMode, watcher};

let (tx, rx) = channel();
let mut watcher = watcher(tx, Duration::from_secs(1))?;
watcher.watch("/path/to/repo", RecursiveMode::Recursive)?;

loop {
    match rx.recv() {
        Ok(event) => {
            // File changed, rescan repository
            loregrep.scan("/path/to/repo").await?;
        }
        Err(e) => println!("watch error: {:?}", e),
    }
}
```

## Module Organization

The library uses the following internal module structure (not exposed in public API):

```
src/
├── lib.rs              # Public API definitions
├── loregrep.rs         # Main LoreGrep implementation
├── core/              # Core functionality
│   ├── mod.rs
│   ├── ai_tools.rs    # Tool implementations
│   ├── types.rs       # Core type definitions
│   └── errors.rs      # Error types
├── analyzers/         # Language analyzers
│   ├── mod.rs
│   ├── traits.rs      # LanguageAnalyzer trait
│   └── rust.rs        # Rust analyzer
├── storage/           # In-memory storage
│   ├── mod.rs
│   └── memory.rs      # RepoMap implementation
├── scanner/           # Repository scanning
│   ├── mod.rs
│   └── discovery.rs   # File discovery
└── internal/          # Internal modules (CLI, etc.)
    ├── mod.rs
    ├── cli.rs         # CLI implementation
    ├── config.rs      # Configuration
    └── ui/            # UI components
```

## Feature Flags

```toml
[features]
default = ["rust-analyzer"]
rust-analyzer = []
python-analyzer = []  # Future
typescript-analyzer = []  # Future
go-analyzer = []  # Future
all-analyzers = ["rust-analyzer", "python-analyzer", "typescript-analyzer", "go-analyzer"]
```

## Thread Safety

- `LoreGrep` is `Send + Sync` and can be shared across threads
- All tool executions are thread-safe
- Repository scanning is not thread-safe (must be called from single thread)

## Performance Characteristics

- **Memory Usage**: ~10KB per analyzed file
- **Scan Speed**: ~1000 files/second on modern hardware
- **Query Speed**: <1ms for most queries on repos with <10k files
- **Cache TTL**: Configurable, defaults to 5 minutes

## Migration from Direct Usage

For users currently using internal modules directly:

```rust
// Old way (don't do this)
use loregrep::storage::memory::RepoMap;
use loregrep::analyzers::rust::RustAnalyzer;

// New way (use this)
use loregrep::LoreGrep;
let loregrep = LoreGrep::builder().build()?;
```

## Error Handling

All operations return `Result<T, LoreGrepError>` for consistent error handling:

```rust
match loregrep.execute_tool("search_functions", params).await {
    Ok(result) => {
        if result.success {
            // Process result.data
        } else {
            // Handle tool-specific error in result.error
        }
    }
    Err(e) => {
        // Handle system error
        eprintln!("Error: {}", e);
    }
}
```

## Future Additions

The API is designed to be extensible without breaking changes:

- Additional language analyzers can be added via builder pattern
- New tools can be added while maintaining backward compatibility
- Performance optimizations can be implemented internally
- Database storage can be added as an internal implementation detail
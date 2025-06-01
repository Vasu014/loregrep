# LoreGrep Public API Specification

## Overview

LoreGrep provides an in-memory code repository analysis library designed for integration into coding assistants and LLM-powered development tools. The library offers a tool-based interface that can be easily integrated into any AI assistant's tool calling system.

## Core Design Principles

1. **Tool-Based Interface**: All functionality exposed through LLM-compatible tool definitions
2. **Host-Managed Scanning**: Repository scanning is controlled by the host application, not the LLM
3. **Language Agnostic**: Extensible architecture supporting multiple programming languages
4. **Memory Efficient**: Fast in-memory indexing optimized for code analysis
5. **Type Safe**: Strong typing with comprehensive error handling

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

### Integration with Coding Assistant

```rust
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
        // Execute tool and return result
        let result = self.loregrep.execute_tool(&call.name, call.params).await?;
        Ok(serde_json::to_value(result)?)
    }
    
    pub async fn refresh_index(&mut self, path: &str) -> Result<()> {
        // Rescan when files change
        self.loregrep.scan(path).await?;
        Ok(())
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
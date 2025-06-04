# Loregrep Rust Crate

[![Crates.io](https://img.shields.io/crates/v/loregrep.svg)](https://crates.io/crates/loregrep)
[![docs.rs](https://docs.rs/loregrep/badge.svg)](https://docs.rs/loregrep)
[![Rust 1.70+](https://img.shields.io/badge/rustc-1.70+-blue.svg)](https://blog.rust-lang.org/2023/06/01/Rust-1.70.0.html)

**High-performance repository indexing library for coding assistants**

A fast, memory-efficient Rust library that parses codebases using tree-sitter and provides structured access to functions, structs, dependencies, and call graphs. Built for AI coding assistants and code analysis tools.

## Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
loregrep = "0.3.3"
tokio = { version = "1.35", features = ["full"] }
serde_json = "1.0"
```

### Basic Usage

```rust
use loregrep::LoreGrep;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create analyzer with builder pattern
    let mut loregrep = LoreGrep::builder()
        .max_file_size(2 * 1024 * 1024)  // 2MB limit
        .max_depth(10)
        .file_patterns(vec!["*.rs".to_string(), "*.py".to_string()])
        .exclude_patterns(vec!["target/".to_string(), ".git/".to_string()])
        .respect_gitignore(true)
        .build()?;
    
    // Scan repository
    let result = loregrep.scan("./my-project").await?;
    println!("📁 Scanned {} files, found {} functions", 
             result.files_scanned, result.functions_found);
    
    // Search for functions
    let functions = loregrep.execute_tool("search_functions", json!({
        "pattern": "async",
        "limit": 10
    })).await?;
    
    println!("🔍 Async functions:\n{}", functions.content);
    Ok(())
}
```

## API Reference

### Builder Pattern

```rust use loregrep::LoreGrep;

let loregrep = LoreGrep::builder()
    .max_file_size(5 * 1024 * 1024)     // 5MB max file size
    .max_depth(15)                       // Directory traversal depth
    .file_patterns(vec![
        "*.rs".to_string(),
        "*.toml".to_string(),
        "*.md".to_string()
    ])
    .exclude_patterns(vec![
        "target/".to_string(),
        ".git/".to_string(),
        "*.tmp".to_string()
    ])
    .respect_gitignore(true)             // Honor .gitignore files
    .build()?;
```

### Scanning Results

```rust
use loregrep::ScanResult;

let result: ScanResult = loregrep.scan("/path/to/repo").await?;

// Access detailed scan statistics
println!("Files scanned: {}", result.files_scanned);
println!("Functions found: {}", result.functions_found);
println!("Structs found: {}", result.structs_found);
println!("Scan duration: {}ms", result.duration_ms);

// Handle any scan errors
for error in &result.errors {
    eprintln!("Scan error: {}", error);
}
```

### Tool Execution

Loregrep provides 6 standardized tools for code analysis:

```rust
use serde_json::json;

// 1. Search functions by pattern
let functions = loregrep.execute_tool("search_functions", json!({
    "pattern": "config",
    "limit": 20
})).await?;

// 2. Search structs/enums by pattern
let structs = loregrep.execute_tool("search_structs", json!({
    "pattern": "Config",
    "limit": 10
})).await?;

// 3. Analyze specific file
let analysis = loregrep.execute_tool("analyze_file", json!({
    "file_path": "src/main.rs",
    "include_source": false
})).await?;

// 4. Get file dependencies
let deps = loregrep.execute_tool("get_dependencies", json!({
    "file_path": "src/lib.rs"
})).await?;

// 5. Find function callers
let callers = loregrep.execute_tool("find_callers", json!({
    "function_name": "parse_config"
})).await?;

// 6. Get repository tree
let tree = loregrep.execute_tool("get_repository_tree", json!({
    "include_file_details": true,
    "max_depth": 3
})).await?;
```

### Error Handling

```rust
use loregrep::{LoreGrep, LoreGrepError};

match loregrep.scan("/invalid/path").await {
    Ok(result) => println!("Scan completed: {:?}", result),
    Err(LoreGrepError::IoError(e)) => eprintln!("IO error: {}", e),
    Err(LoreGrepError::ParseError(e)) => eprintln!("Parse error: {}", e),
    Err(LoreGrepError::ConfigError(e)) => eprintln!("Config error: {}", e),
    Err(e) => eprintln!("Other error: {}", e),
}
```

## CLI Usage

Loregrep includes a powerful CLI for interactive and scripted usage:

### Installation

```bash
cargo install loregrep
```

### Basic Commands

```bash
# Scan current directory
loregrep scan .

# Search for functions
loregrep search functions "async" --limit 10

# Analyze specific file
loregrep analyze src/main.rs

# Get repository tree
loregrep tree --max-depth 3

# Interactive mode
loregrep interactive
```

### CLI Configuration

Create a `loregrep.toml` file in your project root:

```toml
[scanning]
max_file_size = 5242880  # 5MB
max_depth = 15
respect_gitignore = true

[filtering]
file_patterns = ["*.rs", "*.toml", "*.md"]
exclude_patterns = ["target/", ".git/", "*.tmp"]

[output]
format = "json"  # or "table", "tree"
max_results = 50
```

## Performance Characteristics

### Benchmarks

On a typical Rust project (100 files, 50k LOC):

- **Scanning**: ~200ms
- **Indexing**: ~50ms  
- **Function search**: ~2ms
- **Memory usage**: ~10MB

### Optimization Tips

```rust
// For large repositories
let loregrep = LoreGrep::builder()
    .max_file_size(1024 * 1024)     // Limit file size
    .exclude_patterns(vec![
        "target/".to_string(),       // Skip build artifacts
        "vendor/".to_string(),       // Skip vendored code
        "*.lock".to_string()         // Skip lock files
    ])
    .build()?;

// For memory-constrained environments
let loregrep = LoreGrep::builder()
    .max_depth(5)                   // Limit recursion depth
    .file_patterns(vec![
        "*.rs".to_string()          // Only scan Rust files
    ])
    .build()?;
```

### Async and Concurrency

Loregrep is fully async and thread-safe:

```rust
use std::sync::Arc;
use tokio::task;

// Share across tasks
let loregrep = Arc::new(loregrep);

// Concurrent operations
let handles: Vec<_> = (0..4).map(|i| {
    let lg = Arc::clone(&loregrep);
    task::spawn(async move {
        lg.execute_tool("search_functions", json!({
            "pattern": format!("handler_{}", i),
            "limit": 5
        })).await
    })
}).collect();

// Wait for all tasks
for handle in handles {
    let result = handle.await??;
    println!("Result: {}", result.content);
}
```

## Integration Patterns

### With AI Libraries

```rust
use loregrep::LoreGrep;

pub struct CodeAssistant {
    loregrep: LoreGrep,
    ai_client: AIClient,
}

impl CodeAssistant {
    pub async fn analyze_codebase(&self, query: &str) -> Result<String, Box<dyn std::error::Error>> {
        // Get structured code data
        let functions = self.loregrep.execute_tool("search_functions", json!({
            "pattern": query,
            "limit": 10
        })).await?;
        
        // Send to AI for analysis
        let response = self.ai_client.analyze(&functions.content).await?;
        Ok(response)
    }
}
```

### With Web Servers

```rust
use axum::{extract::Query, response::Json, routing::get, Router};
use loregrep::LoreGrep;
use std::sync::Arc;

async fn search_handler(
    loregrep: Arc<LoreGrep>,
    Query(params): Query<HashMap<String, String>>
) -> Json<serde_json::Value> {
    let pattern = params.get("q").unwrap_or(&"".to_string());
    
    let result = loregrep.execute_tool("search_functions", json!({
        "pattern": pattern,
        "limit": 20
    })).await.unwrap();
    
    Json(serde_json::json!({
        "results": result.content
    }))
}

#[tokio::main]
async fn main() {
    let loregrep = Arc::new(LoreGrep::builder().build().unwrap());
    
    let app = Router::new()
        .route("/search", get({
            let lg = Arc::clone(&loregrep);
            move |query| search_handler(lg, query)
        }));
    
    // Start server...
}
```

### Build Scripts Integration

```rust
// build.rs
use loregrep::LoreGrep;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    
    rt.block_on(async {
        let loregrep = LoreGrep::builder().build()?;
        let result = loregrep.scan("src").await?;
        
        // Generate code based on analysis
        println!("cargo:rustc-env=FUNCTIONS_COUNT={}", result.functions_found);
        println!("cargo:rustc-env=STRUCTS_COUNT={}", result.structs_found);
        
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    
    Ok(())
}
```

## Language Support

| Language   | Functions | Structs/Enums | Imports | Calls | Status |
|------------|-----------|---------------|---------|-------|--------|
| **Rust**   | ✅        | ✅            | ✅      | ✅    | Full   |
| Python     | 🚧        | 🚧            | 🚧      | 🚧    | Planned |
| TypeScript | 🚧        | 🚧            | 🚧      | 🚧    | Planned |
| JavaScript | 🚧        | 🚧            | 🚧      | 🚧    | Planned |

## Advanced Usage

### Custom Tool Development

```rust
use loregrep::{ToolSchema, ToolResult};
use serde_json::Value;

// Implement custom analysis tools
impl LoreGrep {
    pub async fn execute_custom_tool(&self, tool_fn: impl Fn(&Self) -> ToolResult) -> ToolResult {
        tool_fn(self)
    }
}

// Example: Find all public functions
let public_functions = loregrep.execute_custom_tool(|lg| {
    // Custom analysis logic
    ToolResult {
        content: "Custom analysis results".to_string(),
        metadata: None,
    }
}).await;
```

### Extending File Support

```rust
// Add custom file extensions
let loregrep = LoreGrep::builder()
    .file_patterns(vec![
        "*.rs".to_string(),
        "*.ron".to_string(),      // Rust Object Notation
        "*.pest".to_string(),     // Parser definitions
        "Cargo.toml".to_string(), // Specific files
    ])
    .build()?;
```

## Contributing

Want to contribute to loregrep? See our [main README](README.md#contributing) for guidelines.

### Rust-Specific Contributions

- **Language analyzers**: Implement parsers for new languages
- **Performance optimizations**: Profile and optimize hot paths
- **Tool implementations**: Add new analysis tools
- **CLI enhancements**: Improve command-line interface

## Examples

See the [`examples/`](examples/) directory for complete examples:

- [`basic_usage.rs`](examples/basic_usage.rs) - Basic scanning and searching
- [`cli_integration.rs`](examples/cli_integration.rs) - CLI tool usage
- [`performance_demo.rs`](examples/performance_demo.rs) - Performance benchmarking
- [`coding_assistant.rs`](examples/coding_assistant.rs) - Full coding assistant implementation

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option. 
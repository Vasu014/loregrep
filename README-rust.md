# Loregrep Rust Crate

[![Crates.io](https://img.shields.io/crates/v/loregrep.svg)](https://crates.io/crates/loregrep)
[![docs.rs](https://docs.rs/loregrep/badge.svg)](https://docs.rs/loregrep)
[![Rust 1.70+](https://img.shields.io/badge/rustc-1.70+-blue.svg)](https://blog.rust-lang.org/2023/06/01/Rust-1.70.0.html)

**High-performance repository indexing library for coding assistants**

A fast, memory-efficient Rust library that parses codebases using tree-sitter and provides structured access to functions, structs, dependencies, and call graphs. Built for AI coding assistants and code analysis tools.

## ✨ **What's New in v0.4.0: Enhanced User Experience**

**Transform from complex setup to delightful developer experience:**

- 🎯 **Real-time feedback**: See analyzer registration and validation immediately  
- 📊 **Comprehensive summaries**: Detailed scan results with performance metrics
- 🔧 **Actionable errors**: Clear guidance when something goes wrong
- 🌟 **Professional polish**: Emoji indicators and user-friendly messages
- 📁 **Complete file path tracking**: Every code element includes its originating file

### Before vs After
```rust
// Before: Silent, unclear feedback
let loregrep = LoreGrep::builder().build()?;

// After: Rich, helpful feedback
let mut loregrep = LoreGrep::builder()
    .with_rust_analyzer()     // ✅ Rust analyzer registered successfully
    .with_python_analyzer()   // ✅ Python analyzer registered successfully  
    .build()?;                // 🎆 LoreGrep configured with 2 languages

// Enhanced scan with comprehensive feedback
let result = loregrep.scan("./my-project").await?;
// 🔍 Starting scan... 📁 Found X files... 📊 Summary with metrics
```

## Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
loregrep = "0.4.0"
tokio = { version = "1.35", features = ["full"] }
serde_json = "1.0"
```

### Basic Usage

```rust
use loregrep::LoreGrep;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Easiest way: Zero-configuration auto-discovery
    let mut loregrep = LoreGrep::auto_discover("./my-project")?;
    // 🔍 Detected project languages: rust, python
    // ✅ Rust analyzer registered successfully
    // ✅ Python analyzer registered successfully
    
    // Alternative: Enhanced builder for fine control
    // let mut loregrep = LoreGrep::builder()
    //     .with_rust_analyzer()            // ✅ Rust analyzer registered successfully
    //     .with_python_analyzer()          // ✅ Python analyzer registered successfully
    //     .max_file_size(2 * 1024 * 1024) // 2MB limit
    //     .max_depth(10)
    //     .file_patterns(vec!["*.rs".to_string(), "*.py".to_string()])
    //     .exclude_patterns(vec!["target/".to_string(), ".git/".to_string()])
    //     .respect_gitignore(true)
    //     .build()?;                       // 🎆 LoreGrep configured with 2 languages
    
    // Enhanced scan with progress indicators and summary
    let result = loregrep.scan("./my-project").await?;
    // 🔍 Starting repository scan for: ./my-project
    // 🌐 Registered analyzers: rust, python  
    // 📁 Found 47 files to analyze
    // 📊 Scan Summary:
    //    📁 Files analyzed: 47
    //    🔧 Functions found: 156
    //    🏗️  Structs found: 23
    //    🌐 Languages detected: ["rust", "python"]
    //    ⏱️  Scan duration: 1.23s
    //    ✅ Repository successfully indexed and ready for queries!
    
    // Search for functions (same as before - backward compatible!)
    let functions = loregrep.execute_tool("search_functions", json!({
        "pattern": "async",
        "limit": 10
    })).await?;
    
    println!("🔍 Async functions:\n{}", functions.content);
    Ok(())
}
```

## API Reference

### Advanced Configuration with Builder Pattern

For cases where you need fine-grained control beyond auto-discovery:

```rust
use loregrep::LoreGrep;

// Enhanced builder with detailed configuration
let mut loregrep = LoreGrep::builder()
    .with_rust_analyzer()                // ✅ Rust analyzer registered successfully
    .with_python_analyzer()              // ✅ Python analyzer registered successfully  
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
    .build()?;                           // 🎆 LoreGrep configured with 2 languages
    
// Builder now validates configuration and provides feedback:
// ✅ LoreGrep instance created successfully!
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

### Tool Execution with File Path Tracking

Loregrep provides 6 standardized tools for code analysis. **Every result includes complete file path information** for better cross-file reference tracking:

```rust
use serde_json::json;

// 1. Search functions by pattern - now with file paths!
let functions = loregrep.execute_tool("search_functions", json!({
    "pattern": "config",
    "limit": 20
})).await?;

// Example response shows file paths for each function:
// {
//   "functions": [
//     {
//       "name": "parse_config",
//       "file_path": "src/config.rs",  ← Always included
//       "start_line": 45,
//       "signature": "pub fn parse_config(path: &str) -> Result<Config>"
//     },
//     {
//       "name": "load_config", 
//       "file_path": "src/utils/loader.rs",  ← Different file
//       "start_line": 12,
//       "signature": "fn load_config() -> Config"
//     }
//   ]
// }

// 2. Search structs/enums by pattern
let structs = loregrep.execute_tool("search_structs", json!({
    "pattern": "Config",
    "limit": 10
})).await?;

// 3. Analyze specific file - shows all elements with their file paths
let analysis = loregrep.execute_tool("analyze_file", json!({
    "file_path": "src/main.rs",
    "include_source": false
})).await?;

// 4. Get file dependencies - imports/exports include file paths
let deps = loregrep.execute_tool("get_dependencies", json!({
    "file_path": "src/lib.rs"
})).await?;

// 5. Find function callers - shows where functions are called from
let callers = loregrep.execute_tool("find_callers", json!({
    "function_name": "parse_config"
})).await?;

// Example response with cross-file calls:
// {
//   "callers": [
//     {
//       "function_name": "parse_config",
//       "file_path": "src/main.rs",       ← Called from main.rs
//       "line_number": 23,
//       "caller_function": "main"
//     },
//     {
//       "function_name": "parse_config", 
//       "file_path": "src/utils/loader.rs", ← Also called from loader.rs
//       "line_number": 45,
//       "caller_function": "load_default_config"
//     }
//   ]
// }

// 6. Get repository tree with file path details
let tree = loregrep.execute_tool("get_repository_tree", json!({
    "include_file_details": true,
    "max_depth": 3
})).await?;
```

### Enhanced Error Handling with Actionable Guidance

```rust
use loregrep::{LoreGrep, LoreGrepError};

// Enhanced error messages provide specific guidance
match loregrep.scan("/invalid/path").await {
    Ok(result) => {
        // Scan now provides comprehensive feedback automatically:
        // 🔍 Starting repository scan for: /invalid/path
        // ⚠️  No files found in the specified path
        // 💡 Check that the path exists and contains supported file types
        println!("Scan completed: {} files processed", result.files_scanned);
    },
    Err(LoreGrepError::IoError(e)) => eprintln!("IO error: {}", e),
    Err(LoreGrepError::ParseError(e)) => eprintln!("Parse error: {}", e), 
    Err(LoreGrepError::ConfigError(e)) => eprintln!("Config error: {}", e),
    Err(e) => eprintln!("Other error: {}", e),
}

// Enhanced error handling for unsupported languages:
// ⚠️  No analyzer available for 'javascript' files. Supported: rust, python
// 💡 Add support with: LoreGrep::builder().with_javascript_analyzer() (coming soon)

// Builder validation warnings:
let loregrep = LoreGrep::builder()
    .build()?;
// ⚠️  Warning: No language analyzers registered!
// 💡 Consider adding: .with_rust_analyzer() or .with_python_analyzer() 
// 📁 Files will be discovered but not analyzed
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

### With AI Libraries - Leveraging File Path Information

```rust
use loregrep::LoreGrep;
use serde_json::json;

pub struct CodeAssistant {
    loregrep: LoreGrep,
    ai_client: AIClient,
}

impl CodeAssistant {
    pub async fn analyze_codebase(&self, query: &str) -> Result<String, Box<dyn std::error::Error>> {
        // Get structured code data with file paths
        let functions = self.loregrep.execute_tool("search_functions", json!({
            "pattern": query,
            "limit": 10
        })).await?;
        
        // AI now knows exactly where each function is located
        // Example: functions.content includes:
        // [
        //   {
        //     "name": "authenticate_user",
        //     "file_path": "src/auth/mod.rs",     ← AI knows this is in auth module
        //     "start_line": 45
        //   },
        //   {
        //     "name": "auth_middleware", 
        //     "file_path": "src/middleware/auth.rs", ← AI knows this is middleware
        //     "start_line": 12
        //   }
        // ]
        
        // Send to AI for contextual analysis
        let response = self.ai_client.analyze_with_context(
            &functions.content,
            "Focus on file organization and cross-module dependencies"
        ).await?;
        Ok(response)
    }
    
    /// Find all files that would be affected by changing a function
    pub async fn impact_analysis(&self, function_name: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // Find where the function is defined
        let definitions = self.loregrep.execute_tool("search_functions", json!({
            "pattern": function_name,
            "limit": 1
        })).await?;
        
        // Find where it's called
        let callers = self.loregrep.execute_tool("find_callers", json!({
            "function_name": function_name
        })).await?;
        
        // Extract all unique file paths affected
        let mut affected_files = std::collections::HashSet::new();
        
        // Parse JSON and extract file_path fields
        // (In real implementation, you'd deserialize to structs)
        // This gives you a complete list of files that would be impacted
        
        Ok(affected_files.into_iter().collect())
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
    let loregrep = Arc::new(
        LoreGrep::auto_discover(".")    // ✅ Zero-configuration setup
            .unwrap()
    );
    
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
        let loregrep = LoreGrep::auto_discover("src")?;
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
| **Python** | ✅        | ✅            | ✅      | ✅    | Full   |
| TypeScript | 🚧        | 🚧            | 🚧      | 🚧    | Planned |
| JavaScript | 🚧        | 🚧            | 🚧      | 🚧    | Planned |

## Advanced Usage

### Custom Tool Development with File Path Awareness

```rust
use loregrep::{ToolSchema, ToolResult};
use serde_json::{json, Value};
use std::collections::HashMap;

// Custom analysis that leverages file path information
impl LoreGrep {
    /// Find functions grouped by their containing modules
    pub async fn analyze_by_module(&self, pattern: &str) -> Result<ToolResult, Box<dyn std::error::Error>> {
        let functions = self.execute_tool("search_functions", json!({
            "pattern": pattern,
            "limit": 100
        })).await?;
        
        // Group functions by their file paths (modules)
        let mut modules: HashMap<String, Vec<Value>> = HashMap::new();
        
        // Parse the function results and group by file_path
        // Each function includes: { "name": "...", "file_path": "src/module/file.rs", ... }
        
        let result = json!({
            "analysis_type": "module_grouping",
            "pattern": pattern,
            "modules": modules,
            "summary": {
                "total_modules": modules.len(),
                "cross_module_functions": "Functions spread across multiple modules"
            }
        });
        
        Ok(ToolResult {
            content: result.to_string(),
            metadata: None,
        })
    }
    
    /// Find potential circular dependencies using file path analysis
    pub async fn check_circular_dependencies(&self) -> Result<ToolResult, Box<dyn std::error::Error>> {
        // Get all imports with their file paths
        let mut potential_cycles = Vec::new();
        
        // Analyze import statements across all files
        // Each import includes: { "module_path": "...", "file_path": "...", ... }
        
        let result = json!({
            "analysis_type": "circular_dependency_check",
            "potential_cycles": potential_cycles,
            "recommendation": "Review these import patterns for circular dependencies"
        });
        
        Ok(ToolResult {
            content: result.to_string(),
            metadata: None,
        })
    }
}

// Usage examples:
let module_analysis = loregrep.analyze_by_module("config").await?;
let dependency_check = loregrep.check_circular_dependencies().await?;
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
- [`performance_demo.rs`](examples/performance_demo.rs) - Advanced usage demonstration
- [`coding_assistant.rs`](examples/coding_assistant.rs) - Full coding assistant implementation

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option. 
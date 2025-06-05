# Loregrep

[![Crates.io](https://img.shields.io/crates/v/loregrep.svg)](https://crates.io/crates/loregrep)
[![PyPI](https://img.shields.io/pypi/v/loregrep.svg)](https://pypi.org/project/loregrep/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/Vasu014/loregrep#license)
[![Build Status](https://img.shields.io/github/workflow/status/Vasu014/loregrep/CI)](https://github.com/Vasu014/loregrep/actions)

**Fast Repository Indexing Library for Coding Assistants**

Loregrep is a Rust library with Python bindings that parses codebases into fast, searchable in-memory indexes. It's designed to provide coding assistants and AI tools with structured access to code functions, structures, dependencies, and call graphs.

**Currently supports Rust and Python with TypeScript/JavaScript coming soon.**

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [What It Does](#what-it-does)
- [Use Cases](#use-cases)
- [Language Support](#language-support)
- [API Examples](#api-examples)
- [Available Tools](#available-tools)
- [Performance](#performance)
- [Architecture](#architecture)
- [Examples](#examples)
- [Development Setup](#development-setup)
- [Contributing](#contributing)
- [License](#license)

## Installation

### Rust Users
```bash
cargo add loregrep
```

### Python Users
```bash
pip install loregrep
```

### System Requirements
- **Rust**: 1.70 or later
- **Python**: 3.8 or later
- **Memory**: 100MB+ available RAM (scales with repository size)
- **OS**: Linux, macOS, Windows

## Documentation

**Looking for language-specific documentation?**

- 🦀 **Rust Developers**: See [README-rust.md](README-rust.md) for Rust-specific API, CLI usage, performance tips, and integration patterns
- 🐍 **Python Developers**: See [README-python.md](README-python.md) for Python-specific examples, async patterns, and AI integration
- 📖 **General Overview**: Continue reading this README for project overview and cross-language examples

## Quick Start

### 30-Second Example

First, clone the loregrep repository to try the examples:

```bash
git clone https://github.com/Vasu014/loregrep.git
cd loregrep
```

**Rust:**
```rust
use loregrep::LoreGrep;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Zero-configuration setup - auto-detects project type
    let mut loregrep = LoreGrep::auto_discover(".")?;
    let scan_result = loregrep.scan(".").await?;
    
    println!("📁 Scanned loregrep repository");
    println!("   {} files analyzed, {} functions found", 
             scan_result.files_scanned, scan_result.functions_found);
    
    // Search for functions containing "main"
    let result = loregrep.execute_tool("search_functions", json!({
        "pattern": "main",
        "limit": 5
    })).await?;
    
    println!("🔍 Found functions:\n{}", result.content);
    Ok(())
}
```

**Python:**
```python
import asyncio
import loregrep
import os

async def main():
    # Zero-configuration setup - auto-detects project type
    loregrep_instance = loregrep.LoreGrep.auto_discover(".")
    scan_result = await loregrep_instance.scan(".")
    
    repo_name = os.path.basename(os.getcwd())
    print(f"📁 Scanned {repo_name} repository")
    print(f"   {scan_result.files_scanned} files analyzed, "
          f"{scan_result.functions_found} functions found")
    
    # Search for functions containing "main"
    result = await loregrep_instance.execute_tool("search_functions", {
        "pattern": "main",
        "limit": 5
    })
    
    print(f"🔍 Found functions:\n{result.content}")

asyncio.run(main())
```

**Expected Output (when run in loregrep repository):**
```
📁 Scanned loregrep repository
   42 files analyzed, 156 functions found
🔍 Found functions:
main (src/main.rs:15) - Entry point for CLI application
cli_main (src/cli_main.rs:8) - CLI entry point wrapper
async_main (src/internal/ai/conversation.rs:45) - Main conversation loop
...
```

### Try Your Own Repository

Once you understand the basics, scan your own projects:

**Rust:**
```rust
// Easiest way - auto-discover project type and scan
let mut loregrep = LoreGrep::auto_discover("/path/to/your/project")?;
let scan_result = loregrep.scan("/path/to/your/project").await?;
println!("📁 Found {} functions in {} files", 
         scan_result.functions_found, scan_result.files_scanned);
```

**Python:**
```python
# Easiest way - auto-discover project type and scan
loregrep_instance = loregrep.LoreGrep.auto_discover("/path/to/your/project")
scan_result = await loregrep_instance.scan("/path/to/your/project")
print(f"📁 Found {scan_result.functions_found} functions in {scan_result.files_scanned} files")
```

## What It Does

- **Parses** code files using tree-sitter for accurate syntax analysis
- **Indexes** functions, structs, imports, exports, and relationships in memory  
- **Stores** complete file path information for every code element for cross-file reference tracking
- **Provides** 6 standardized tools that coding assistants can call to query the codebase
- **Enables** AI systems to understand code structure without re-parsing

### 🎯 **File Path Storage for Better AI Integration**

Every function, struct, import, export, and function call includes its originating file path:

```json
{
  "name": "parse_config",
  "file_path": "src/config.rs",          // ← Always included
  "start_line": 45,
  "signature": "pub fn parse_config(...)"
}
```

**Why This Matters for AI Coding Assistants:**
- **Cross-file Navigation**: AI can track where functions are defined vs. where they're called
- **Impact Analysis**: Understand which files are affected by changes
- **Context Awareness**: AI knows the full location context when suggesting code changes
- **Refactoring Safety**: Track all references across the entire codebase

### Before vs After: File Path Storage Enhancement

**Before (v0.3.2 and earlier):**
```json
{
  "name": "parse_config",
  "signature": "pub fn parse_config(path: &str) -> Result<Config>",
  "line_number": 45
}
```
*AI knows the function exists but not where it's located.*

**After (v0.4.0+):**
```json
{
  "name": "parse_config", 
  "file_path": "src/config.rs",          // ← Now included!
  "signature": "pub fn parse_config(path: &str) -> Result<Config>",
  "start_line": 45,
  "end_line": 52
}
```
*AI knows exactly where the function is defined and can track cross-file relationships.*

**Real-World Impact:**
- **Question**: "How is the Config struct used across the codebase?"
- **Before**: AI could find Config but couldn't tell you it's defined in `src/config.rs` and used in `src/main.rs`, `src/api.rs`, etc.
- **After**: AI can now provide a complete cross-file usage analysis with specific file locations.

## What It's NOT

- ❌ Not an AI tool itself (provides data TO AI systems)
- ❌ Not a traditional code analysis tool (no linting, metrics, complexity analysis)
- ❌ Not a replacement for LSP servers (different use case)

## Use Cases

### Build Your Own Code Assistant
Create AI-powered tools that understand your codebase:
```python
# Your custom coding assistant
assistant = MyCodingAssistant(loregrep_instance)
answer = await assistant.ask("How do I use the Config struct?")
# Assistant can now search functions, analyze files, and understand relationships
```

### Enhance Existing AI Tools
Add structured code understanding to chatbots and AI applications:
- **Before**: AI sees code as plain text
- **After**: AI understands functions, classes, imports, and relationships

### Smart Code Search
Build advanced code search beyond text matching:
```python
# Find all functions that call a specific function
callers = await loregrep.execute_tool("find_callers", {
    "function_name": "parse_config",
    "include_context": True
})
```

### Repository Analysis
Understand codebase structure and complexity:
```python
# Get complete repository overview
tree = await loregrep.execute_tool("get_repository_tree", {
    "include_file_details": True,
    "show_complexity": True
})
```

## Language Support

| Language   | Status | Functions | Structs/Classes | Imports | Calls |
|------------|--------|-----------|-----------------|---------|-------|
| **Rust**   | ✅ Full | ✅ | ✅ | ✅ | ✅ |
| **Python** | ✅ Full | ✅ | ✅ | ✅ | ✅ |
| **TypeScript** | 📋 Planned | 📋 | 📋 | 📋 | 📋 |
| **JavaScript** | 📋 Planned | 📋 | 📋 | 📋 | 📋 |
| **Go** | 📋 Future | 📋 | 📋 | 📋 | 📋 |

*Want to contribute a language analyzer? See [Contributing](#contributing)*

## API Examples

### Setup Options

Loregrep provides three convenient ways to create instances:

1. **`auto_discover()`** - Zero configuration, automatically detects project languages
2. **`builder()`** - Full control with fluent configuration methods
3. **Project presets** - Language-specific optimized configurations (`rust_project()`, `python_project()`, `polyglot_project()`)

### Rust API

```rust
use loregrep::{LoreGrep, ToolSchema};
use serde_json::json;

// Easiest way: zero-configuration auto-discovery
let mut loregrep = LoreGrep::auto_discover(".")?;

// Alternative: configure with builder pattern for fine control
// let mut loregrep = LoreGrep::builder()
//     .with_rust_analyzer()           // ✅ Rust analyzer registered
//     .with_python_analyzer()         // ✅ Python analyzer registered
//     .max_file_size(1024 * 1024)     // 1MB max file size
//     .max_depth(10)                  // Maximum 10 directory levels
//     .file_patterns(vec!["*.rs", "*.py"])  // Include only these files
//     .exclude_patterns(vec!["target/", "node_modules/"])  // Skip these dirs
//     .respect_gitignore(true)        // Honor .gitignore
//     .build()?;

// Scan repository (use "." for current directory)
let scan_result = loregrep.scan(".").await?;
println!("Found {} functions in {} files", 
         scan_result.functions_found, scan_result.files_scanned);

// Get available tools for LLM integration
let tools: Vec<ToolSchema> = LoreGrep::get_tool_definitions();

// Execute tools
let functions = loregrep.execute_tool("search_functions", json!({
    "pattern": "parse.*config",
    "limit": 10,
    "include_private": false
})).await?;

let file_analysis = loregrep.execute_tool("analyze_file", json!({
    "file_path": "src/config.rs",  // Adjust path to match your project
    "include_source": true,
    "show_complexity": true
})).await?;
```

### Python API

```python
import asyncio
import loregrep
import json

async def main():
    # Easiest way: zero-configuration auto-discovery
    loregrep_instance = loregrep.LoreGrep.auto_discover(".")
    
    # Alternative: configure with builder pattern for fine control
    # loregrep_instance = (loregrep.LoreGrep.builder()
    #                     .with_rust_analyzer()        # ✅ Rust analyzer registered
    #                     .with_python_analyzer()      # ✅ Python analyzer registered
    #                     .max_file_size(1024 * 1024)
    #                     .max_depth(10)
    #                     .file_patterns(["*.py", "*.rs", "*.js"])
    #                     .exclude_patterns(["__pycache__/", "node_modules/"])
    #                     .respect_gitignore(True)
    #                     .build())

    # Scan repository (use "." for current directory)
    scan_result = await loregrep_instance.scan(".")
    print(f"Found {scan_result.functions_found} functions in {scan_result.files_scanned} files")

    # Get available tools
    tools = loregrep.LoreGrep.get_tool_definitions()
    for tool in tools:
        print(f"Tool: {tool.name} - {tool.description}")

    # Execute tools
    functions = await loregrep_instance.execute_tool("search_functions", {
        "pattern": "parse.*config",
        "limit": 10,
        "include_private": False
    })

    file_analysis = await loregrep_instance.execute_tool("analyze_file", {
        "file_path": "src/config.py",  # Adjust path to match your project
        "include_source": True,
        "show_complexity": True
    })

asyncio.run(main())
```

## Available Tools

Loregrep provides 6 standardized tools designed for LLM integration:

### 1. search_functions
Find functions by name or pattern across the codebase.

**Input:**
```json
{
    "pattern": "parse.*config",
    "limit": 10,
    "include_private": false,
    "file_pattern": "*.rs"
}
```

**Output:**
```json
{
    "functions": [
        {
            "name": "parse_config",
            "file_path": "src/config.rs",
            "line_number": 45,
            "signature": "pub fn parse_config(path: &str) -> Result<Config>",
            "visibility": "public",
            "complexity": 3,
            "start_line": 45,
            "end_line": 52
        },
        {
            "name": "load_config",
            "file_path": "src/main.rs", 
            "line_number": 23,
            "signature": "fn load_config(file: &Path) -> Config",
            "visibility": "private",
            "complexity": 2,
            "start_line": 23,
            "end_line": 28
        }
    ],
    "total_found": 2
}
```

**Use Case:** Find entry points, locate specific functionality, discover API patterns.

### 2. search_structs
Find structures, classes, and types by name or pattern.

**Input:**
```json
{
    "pattern": "Config",
    "limit": 5,
    "include_fields": true
}
```

**Output:**
```json
{
    "structs": [
        {
            "name": "Config",
            "file_path": "src/config.rs",
            "line_number": 12,
            "fields": [
                {"name": "name", "field_type": "String", "is_public": true},
                {"name": "port", "field_type": "u16", "is_public": true},
                {"name": "debug", "field_type": "bool", "is_public": true}
            ],
            "visibility": "public",
            "start_line": 12,
            "end_line": 16
        },
        {
            "name": "DatabaseConfig",
            "file_path": "src/db/mod.rs",
            "line_number": 8,
            "fields": [
                {"name": "url", "field_type": "String", "is_public": false},
                {"name": "max_connections", "field_type": "usize", "is_public": false}
            ],
            "visibility": "public",
            "start_line": 8,
            "end_line": 12
        }
    ]
}
```

**Use Case:** Understand data structures, find models, discover type definitions.

### 3. analyze_file
Get comprehensive analysis of a specific file.

**Input:**
```json
{
    "file_path": "src/config.rs",
    "include_source": true,
    "show_complexity": true
}
```

**Output:**
```json
{
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
            "is_external": true
        }
    ],
    "exports": [
        {
            "exported_item": "Config",
            "file_path": "src/config.rs",
            "line_number": 16,
            "is_public": true
        }
    ],
    "complexity_score": 15,
    "source_code": "// Full source if requested"
}
```

**Use Case:** Deep dive into specific files, understand file structure, code review.

### 4. get_dependencies
Find imports, exports, and dependencies for a file.

**Input:**
```json
{
    "file_path": "src/main.rs",
    "include_external": true,
    "show_usage": true
}
```

**Output:**
```json
{
    "imports": [
        {
            "module_path": "./config",
            "file_path": "src/main.rs",
            "line_number": 2,
            "imported_items": ["Config"],
            "is_external": false,
            "usage_count": 3
        },
        {
            "module_path": "std::fs",
            "file_path": "src/main.rs", 
            "line_number": 1,
            "imported_items": ["read_to_string"],
            "is_external": true,
            "usage_count": 1
        }
    ],
    "exports": [
        {
            "exported_item": "main",
            "file_path": "src/main.rs",
            "line_number": 8,
            "is_public": true,
            "type": "function"
        }
    ]
}
```

**Use Case:** Understand module relationships, track dependencies, refactoring impact.

### 5. find_callers
Find where specific functions are called.

**Input:**
```json
{
    "function_name": "parse_config",
    "include_context": true,
    "limit": 20
}
```

**Output:**
```json
{
    "callers": [
        {
            "function_name": "parse_config",
            "file_path": "src/main.rs",
            "line_number": 23,
            "column": 17,
            "caller_function": "main",
            "context": "let config = parse_config(&args.config_path)?;",
            "is_method_call": false
        },
        {
            "function_name": "parse_config",
            "file_path": "src/utils/loader.rs",
            "line_number": 45,
            "column": 12,
            "caller_function": "load_default_config",
            "context": "return parse_config(DEFAULT_CONFIG_PATH);",
            "is_method_call": false
        }
    ]
}
```

**Use Case:** Impact analysis, understand function usage, refactoring safety.

### 6. get_repository_tree
Get repository structure and overview.

**Input:**
```json
{
    "include_file_details": true,
    "max_depth": 3,
    "show_stats": true
}
```

**Output:**
```json
{
    "structure": {
        "src/": {
            "main.rs": {
                "functions": 1,
                "lines": 45,
                "file_path": "src/main.rs",
                "function_details": [
                    {"name": "main", "file_path": "src/main.rs", "start_line": 8, "is_public": false}
                ]
            },
            "config.rs": {
                "functions": 3,
                "lines": 120,
                "file_path": "src/config.rs",
                "function_details": [
                    {"name": "parse_config", "file_path": "src/config.rs", "start_line": 45, "is_public": true},
                    {"name": "validate_config", "file_path": "src/config.rs", "start_line": 65, "is_public": false},
                    {"name": "default_config", "file_path": "src/config.rs", "start_line": 85, "is_public": true}
                ]
            }
        }
    },
    "stats": {
        "total_files": 15,
        "total_functions": 67,
        "languages": ["rust"],
        "cross_file_references": 23
    }
}
```

**Use Case:** Repository overview, architecture understanding, documentation generation.


## Architecture

```text
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Code Files    │───▶│   Tree-sitter    │───▶│   In-Memory     │
│  (.rs, .py,     │    │    Parsing       │    │    RepoMap      │
│   .ts, etc.)    │    │                  │    │    Indexes      │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                                          │
                                                          ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│ Coding Assistant│◀───│  6 Query Tools   │◀───│   Fast Lookups  │
│   (Claude, GPT, │    │ (search, analyze,│    │  (functions,    │
│   Cursor, etc.) │    │  dependencies)   │    │   structs, etc.)│
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

### Core Components

- **`LoreGrep`**: Main API facade with builder pattern
- **`RepoMap`**: Fast in-memory indexes with lookup optimization  
- **`RepositoryScanner`**: File discovery with gitignore support
- **Language Analyzers**: Tree-sitter based parsing (currently Rust only)
- **Tool System**: 6 standardized tools for AI integration

### Module Structure

```
src/
├── lib.rs                 # Public API exports and documentation
├── loregrep.rs           # Main LoreGrep struct and builder
├── core/                 # Core types and errors
├── types/                # Data structures (FunctionSignature, etc.)
├── analyzers/            # Language-specific parsers
│   └── rust.rs          # Rust analyzer (tree-sitter based)
├── storage/              # In-memory indexing
│   └── repo_map.rs      # RepoMap with fast lookups
├── scanner/              # File discovery
├── internal/             # Internal CLI and AI implementation
│   ├── cli.rs           # CLI application
│   ├── ai/              # Anthropic client and conversation
│   └── ui/              # Progress indicators and theming
└── cli_main.rs          # CLI binary entry point
```

### Technical Notes

**Memory Management:**
- Indexes built in memory for fast access
- Thread-safe with `Arc<Mutex<>>` design
- Memory usage scales linearly with codebase size
- No external dependencies required at runtime

**Error Handling:**
Uses comprehensive error types from `core::errors::LoreGrepError`:
- IO errors (file access, permissions)
- Parse errors (malformed code)
- Configuration errors
- API errors (for AI integration)

## Examples

The project includes comprehensive examples for different use cases:

### Rust Examples (`examples/`)
- **`basic_scan.rs`** - Simple repository scanning and basic queries
- **`tool_execution.rs`** - Complete LLM tool integration patterns  
- **`file_watcher.rs`** - File watching for automatic re-indexing
- **`coding_assistant.rs`** - Full coding assistant implementation
- **`advanced_queries.rs`** - Complex search and analysis patterns

### Python Examples (`python/examples/`)
- **`basic_usage.py`** - Python API introduction and common patterns
- **`ai_integration.py`** - Integration with OpenAI/Anthropic APIs
- **`web_server.py`** - REST API wrapper for web applications
- **`batch_analysis.py`** - Processing multiple repositories

### Running Examples

**Rust:**
```bash
cargo run --example basic_scan -- /path/to/repo
cargo run --example coding_assistant
```

**Python:**
```bash
cd python/examples
python basic_usage.py
python ai_integration.py
```

## Development Setup

### Prerequisites
- Rust 1.70 or later
- Python 3.8+ (for Python bindings)
- For AI integration tests: Anthropic API key

### Building from Source

```bash
git clone https://github.com/Vasu014/loregrep.git
cd loregrep

# Build Rust library
cargo build --release

# Build Python package (requires maturin)
pip install maturin
maturin develop --features python
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test suites
cargo test public_api_integration  # Public API tests
cargo test cli::tests              # CLI tests
cargo test storage::tests          # Storage/indexing tests

# Run with output
cargo test -- --nocapture

# Test Python bindings
cd python && python -m pytest
```

### Development CLI Usage

The CLI is primarily for development and testing:

```bash
# Build development binary
cargo build --bin loregrep

# Basic commands for testing
./target/debug/loregrep scan .
./target/debug/loregrep search "parse" --type function
./target/debug/loregrep analyze src/main.rs
```

### Known Test Status
- ✅ **60+ tests passing** across core functionality
- ⚠️ **8 pre-existing test failures** in older modules (technical debt)
- ✅ **100% pass rate** on new Phase 3B+ tests

## Contributing

We welcome contributions! Loregrep prioritizes the library API over CLI functionality - focus on core indexing and analysis capabilities that enable AI coding assistants.

### High Priority Areas

#### 1. Language Support (Most Needed)
Help expand beyond Rust to support more programming languages:

- **Python analyzer** (`src/analyzers/python.rs`)
  - Functions, classes, decorators
  - Import/export analysis
  - Method calls and inheritance

- **TypeScript analyzer** (`src/analyzers/typescript.rs`)
  - Interfaces, types, generics
  - ES6+ features, async/await
  - Module system analysis

- **JavaScript analyzer** (`src/analyzers/javascript.rs`)
  - Functions, classes, arrow functions
  - CommonJS and ES6 modules
  - Dynamic features handling

#### 2. Performance Improvements
- Memory optimization for large repositories (>100k files)
- Incremental update detection when files change
- Query result caching improvements
- Parallel parsing optimizations

#### 3. Advanced Analysis Features
- Call graph visualization and analysis
- Dependency impact analysis
- Cross-language project support
- Code complexity metrics

### Development Workflow

1. **Fork and Clone**
   ```bash
   git clone https://github.com/Vasu014/loregrep.git
   cd loregrep
   ```

2. **Create Feature Branch**
   ```bash
   git checkout -b feature/python-analyzer
   ```

3. **Develop and Test**
   ```bash
   cargo build
   cargo test
   cargo clippy
   cargo fmt
   ```

4. **Test Integration**
   ```bash
   # Test CLI
   cargo build --bin loregrep
   ./target/debug/loregrep scan examples/

   # Test public API
   cargo run --example basic_scan
   
   # Test Python bindings (if applicable)
   maturin develop --features python
   cd python && python examples/basic_usage.py
   ```

5. **Submit Pull Request**
   - Include tests for new functionality
   - Update documentation for public API changes
   - Follow existing code style and patterns

### Code Style Guidelines

- Use `rustfmt` for formatting: `cargo fmt`
- Use `clippy` for linting: `cargo clippy`
- Add comprehensive tests for new functionality
- Update documentation for any public API changes
- Follow existing module organization patterns
- Include examples for new features

### Getting Help

- **GitHub Issues**: Bug reports and feature requests
- **GitHub Discussions**: Design discussions and questions
- **Discord**: Real-time chat (link in issues)

## Roadmap

### Language Support
- **TypeScript Analyzer**: Complete TS/JS support including modern features
- **JavaScript Analyzer**: Full JavaScript support with ES6+ features
- **Go Language Support**: Package analysis and interface tracking

### Advanced Analysis Features
- **Call Graph Analysis**: Function call extraction and visualization
- **Dependency Tracking**: Advanced import/export analysis
- **Incremental Updates**: Smart re-indexing when files change

### Performance & Scale
- **Performance Improvements**: 2x improvement in scanning speed for large repositories
- **Memory Optimization**: Improved handling of very large repositories
- **Database Persistence**: Optional disk-based storage for massive codebases
- **Distributed Analysis**: Support for monorepos and multi-service codebases

### Integration & Tooling
- **MCP Server Integration**: Standard Model Context Protocol interface
- **Editor Integrations**: VS Code and IntelliJ plugins
- **API Stability**: Stable public API with semantic versioning

*Want to help with any roadmap item? Check out our [Contributing Guide](#contributing)!*

## License

This project is dual-licensed under either:
- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

Choose the license that best fits your use case. Most users prefer MIT for simplicity.

---

**Ready to get started?** Jump to [Installation](#installation) or try the [Quick Start](#quick-start) example! 
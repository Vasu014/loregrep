# Loregrep

**Fast Repository Indexing Library for Coding Assistants**

Loregrep is a Rust library that parses codebases into fast, searchable in-memory indexes. It's designed to provide coding assistants and AI tools with structured access to code functions, structures, dependencies, and call graphs.

## What It Does

- **Parses** code files using tree-sitter for accurate syntax analysis
- **Indexes** functions, structs, imports, exports, and relationships in memory  
- **Provides** 6 standardized tools that coding assistants can call to query the codebase
- **Enables** AI systems to understand code structure without re-parsing

## What It's NOT

- ❌ Not an AI tool itself (provides data TO AI systems)
- ❌ Not a traditional code analysis tool (no linting, metrics, complexity analysis)

## Current Status

**Language Support:**
- ✅ **Rust** - Full support (functions, structs, imports, calls)
- 📋 **Python, TypeScript, JavaScript, Go** - Roadmap (Coming soon...)

**Core Features:**
- ✅ Repository scanning with gitignore support
- ✅ In-memory indexing with fast lookups
- ✅ 6 tool interface for LLM integration
- ✅ Thread-safe API with builder pattern

**Performance (Typical):**
- Small repos (100 files): <1s analysis, <1MB memory
- Medium repos (1,000 files): <10s analysis, <10MB memory
- Large repos (10,000 files): <60s analysis, <100MB memory

## Development Setup

### Prerequisites
- Rust 1.70 or later
- For AI integration tests: Anthropic API key

### Building
```bash
git clone https://github.com/yourusername/loregrep.git
cd loregrep
cargo build --release
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

## Architecture Overview

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

## Library API for Integrators

The public API is designed for external integration:

```rust
use loregrep::{LoreGrep, ToolSchema};
use serde_json::json;

// Initialize and scan
let mut loregrep = LoreGrep::builder()
    .max_file_size(1024 * 1024)
    .build()?;
    
let scan_result = loregrep.scan("/path/to/repo").await?;

// Get tool definitions for LLM
let tools: Vec<ToolSchema> = LoreGrep::get_tool_definitions();

// Execute tool calls (from LLM)
let result = loregrep.execute_tool("search_functions", json!({
    "pattern": "main",
    "limit": 10
})).await?;
```

### Available Tools for LLM Integration

1. **search_functions** - Find functions by name/pattern
2. **search_structs** - Find structures by name/pattern  
3. **analyze_file** - Get detailed file analysis
4. **get_dependencies** - Find imports/exports for a file
5. **find_callers** - Get function call sites
6. **get_repository_tree** - Get repository structure and overview

## Examples and Integration

The `examples/` directory contains integration patterns:

- **`basic_scan.rs`** - Simple repository scanning
- **`tool_execution.rs`** - LLM tool integration patterns  
- **`file_watcher.rs`** - File watching for automatic re-indexing
- **`coding_assistant.rs`** - Complete coding assistant implementation
- **`basic_usage.rs`** - Public API usage patterns

## Testing Strategy

### Test Structure
```bash
tests/
├── public_api_integration.rs  # Public API tests (18 tests)
├── cli_integration.rs         # CLI functionality tests  
└── test_repos/               # Sample repositories for testing
```

### Running Tests
```bash
# All tests
cargo test

# Specific test categories
cargo test public_api         # Library API tests
cargo test cli               # CLI tests  
cargo test storage           # Storage/indexing tests
cargo test scanner           # File discovery tests

# With environment setup for AI tests
ANTHROPIC_API_KEY=your-key cargo test ai::tests
```

### Known Test Status
- ✅ **60+ tests passing** across core functionality
- ⚠️ **8 pre-existing test failures** in older modules (technical debt)
- ✅ **100% pass rate** on new Phase 3B+ tests

## Contributing

### Areas Needing Help

1. **Language Support** (High Priority)
   - Python analyzer in `src/analyzers/python.rs`
   - TypeScript analyzer in `src/analyzers/typescript.rs`
   - JavaScript and Go analyzers

2. **Performance** (Medium Priority)
   - Memory optimization for large repositories
   - Incremental update detection
   - Query result caching improvements

3. **Advanced Features** (Future)
   - Call graph visualization
   - Dependency impact analysis
   - MCP (Model Context Protocol) server interface

### Development Workflow

1. **Fork and Clone**
   ```bash
   git clone https://github.com/your-username/loregrep.git
   cd loregrep
   ```

2. **Create Feature Branch**
   ```bash
   git checkout -b feature/your-feature-name
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
   ```

### Code Style Guidelines

- Use `rustfmt` for formatting: `cargo fmt`
- Use `clippy` for linting: `cargo clippy`
- Add tests for new functionality
- Update documentation for public API changes
- Follow existing module organization patterns

## Implementation Status

**✅ Completed (Production Ready):**
- Foundation & Core Architecture
- Enhanced In-Memory Storage  
- CLI Foundation
- AI Integration
- Enhanced CLI Experience
- Public API Implementation

**🔄 In Progress:**
- Advanced Analysis Features (call graphs, dependency tracking)

**📋 Planned:**
- MCP Server Architecture
- Multi-Language Support (Python, TypeScript, JavaScript, Go)
- Advanced Features (incremental updates, performance optimization)

## Technical Notes

### Memory Management
- Indexes built in memory for fast access
- Thread-safe with `Arc<Mutex<>>` design
- Memory usage scales linearly with codebase size
- No external dependencies required at runtime

### Performance Considerations
- Scanning parallelized across CPU cores
- Query results cached for repeated access
- Tree-sitter parsers reused to avoid recreation overhead
- Gitignore support to skip irrelevant files

### Error Handling
Uses comprehensive error types from `core::errors::LoreGrepError`:
- IO errors (file access, permissions)
- Parse errors (malformed code)
- Configuration errors
- API errors (for AI integration)

## License

This project is dual-licensed under either:
- MIT License (LICENSE-MIT)
- Apache License 2.0 (LICENSE-APACHE)

## Roadmap

### Language Support
- **Python Analyzer**: Full Python support with functions, classes, imports, and method calls
- **TypeScript/JavaScript Analyzers**: Support for modern JS/TS features including interfaces, types, and ES6+ syntax
- **Go Analyzer**: Package declarations, interfaces, and Go-specific function signatures

### Advanced Analysis Features  
- **Call Graph Analysis**: Function call extraction and visualization across files
- **Dependency Tracking**: Advanced import/export analysis and impact assessment
- **Incremental Updates**: Smart re-indexing when files change to avoid full rescans

### Performance & Optimization
- **Memory Optimization**: Improved handling of large repositories with better memory management
- **Query Performance**: Enhanced caching and lookup optimization for faster results
- **Database Persistence**: Optional disk-based storage for very large codebases

### Integration & Architecture
- **MCP Server Integration**: Standard Model Context Protocol interface for tool calling
- **Editor Integrations**: VS Code, IntelliJ, and other popular editor plugins
- **API Enhancements**: Additional tools and query capabilities for LLM integration

---

**Note for Contributors**: This project prioritizes the library API over CLI functionality. The CLI exists primarily for development and testing. Focus contributions on the core indexing and analysis capabilities that enable AI coding assistants. 
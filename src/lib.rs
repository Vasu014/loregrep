//! # Loregrep: Fast Repository Indexing for Coding Assistants
//!
//! **Loregrep** is a high-performance repository indexing library that parses codebases into 
//! fast, searchable in-memory indexes. It's designed to provide coding assistants and AI tools 
//! with structured access to code functions, structures, dependencies, and call graphs.
//!
//! ## What It Does
//!
//! - **Parses** code files using tree-sitter for accurate syntax analysis
//! - **Indexes** functions, structs, imports, exports, and relationships in memory
//! - **Provides** 6 standardized tools that coding assistants can call to query the codebase
//! - **Enables** AI systems to understand code structure without re-parsing
//!
//! ## What It's NOT
//!
//! - ❌ Not an AI tool itself (provides data TO AI systems)
//! - ❌ Not a traditional code analysis tool (no linting, metrics, complexity analysis)
//!
//! ## Core Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │   Code Files    │───▶│   Tree-sitter    │───▶│   In-Memory     │
//! │  (.rs, .py,     │    │    Parsing       │    │    RepoMap      │
//! │   .ts, etc.)    │    │                  │    │    Indexes      │
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//!                                                          │
//!                                                          ▼
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │ Coding Assistant│◀───│  6 Query Tools   │◀───│   Fast Lookups  │
//! │   (Claude, GPT, │    │ (search, analyze,│    │  (functions,    │
//! │   Cursor, etc.) │    │  dependencies)   │    │   structs, etc.)│
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ### Basic Repository Scanning
//!
//! ```rust
//! use loregrep::LoreGrep;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create and configure the indexing engine
//!     let mut loregrep = LoreGrep::builder()
//!         .max_file_size(1024 * 1024)  // 1MB limit
//!         .max_depth(10)               // Directory depth limit
//!         .build()?;
//!
//!     // Scan a repository to build indexes
//!     let scan_result = loregrep.scan("/path/to/your/repo").await?;
//!     
//!     println!("Indexed {} files with {} functions", 
//!              scan_result.files_processed, 
//!              scan_result.functions_found);
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Integration with Coding Assistants
//!
//! The library provides 6 standardized tools that AI coding assistants can call:
//!
//! ```rust
//! use loregrep::LoreGrep;
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut loregrep = LoreGrep::builder().build()?;
//!     loregrep.scan("/path/to/repo").await?;
//!
//!     // Tool 1: Search for functions
//!     let result = loregrep.execute_tool("search_functions", json!({
//!         "pattern": "parse",
//!         "limit": 20
//!     })).await?;
//!
//!     // Tool 2: Find function callers  
//!     let callers = loregrep.execute_tool("find_callers", json!({
//!         "function_name": "parse_config"
//!     })).await?;
//!
//!     // Tool 3: Analyze specific file
//!     let analysis = loregrep.execute_tool("analyze_file", json!({
//!         "file_path": "src/main.rs"
//!     })).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Available Tools for AI Integration
//!
//! ```rust
//! // Get tool definitions for your AI system
//! let tools = LoreGrep::get_tool_definitions();
//! 
//! // 6 tools available:
//! // 1. search_functions      - Find functions by name/pattern
//! // 2. search_structs        - Find structures by name/pattern  
//! // 3. analyze_file          - Get detailed file analysis
//! // 4. get_dependencies      - Find imports/exports for a file
//! // 5. find_callers          - Get function call sites
//! // 6. get_repository_tree   - Get repository structure and overview
//! ```
//!
//! ## Architecture Overview
//!
//! ### Core Components
//!
//! - **`LoreGrep`**: Main API facade with builder pattern configuration
//! - **`RepoMap`**: Fast in-memory indexes with lookup optimization
//! - **`RepositoryScanner`**: File discovery with gitignore support
//! - **Language Analyzers**: Tree-sitter based parsing (Rust complete, others on roadmap)
//! - **Tool System**: 6 standardized tools for AI integration
//!
//! ### Performance Characteristics
//!
//! - **Scanning**: Typically <2 seconds for medium repositories (1000+ files)
//! - **Queries**: Usually <100ms for most lookup operations
//! - **Memory**: Approximately ~50MB for typical Rust projects
//! - **Concurrency**: Thread-safe with `Arc<Mutex<>>` design
//!
//! ## Language Support
//!
//! | Language   | Status     | Functions | Structs | Imports | Calls |
//! |------------|------------|-----------|---------|---------|-------|
//! | Rust       | ✅ Full    | ✅        | ✅      | ✅      | ✅    |
//! | Python     | 📋 Roadmap | -         | -       | -       | -     |
//! | TypeScript | 📋 Roadmap | -         | -       | -       | -     |
//! | JavaScript | 📋 Roadmap | -         | -       | -       | -     |
//! | Go         | 📋 Roadmap | -         | -       | -       | -     |
//!
//! *Note: Languages marked "📋 Roadmap" are future planned additions. Coming soon...*
//!
//! ## Integration Examples
//!
//! ### With Claude/OpenAI
//!
//! ```rust
//! // Provide tools to your AI client
//! let tools = LoreGrep::get_tool_definitions();
//! 
//! // Send to Claude/OpenAI as available tools
//! // When AI calls a tool, execute it:
//! let result = loregrep.execute_tool(&tool_name, tool_args).await?;
//! ```
//!
//! ### With MCP (Model Context Protocol)
//!
//! ```rust
//! // MCP server integration is planned for future releases
//! // Will provide standard MCP interface for tool calling
//! ```
//!
//! ### File Watching Integration
//!
//! ```rust
//! use notify::{Watcher, RecursiveMode, watcher};
//! use std::sync::mpsc::channel;
//! use std::time::Duration;
//!
//! // Watch for file changes and re-index
//! let (tx, rx) = channel();
//! let mut watcher = watcher(tx, Duration::from_secs(2))?;
//! watcher.watch("/path/to/repo", RecursiveMode::Recursive)?;
//!
//! // Re-scan when files change
//! for event in rx {
//!     if let Ok(event) = event {
//!         loregrep.scan("/path/to/repo").await?;
//!     }
//! }
//! ```
//!
//! ## Configuration Options
//!
//! ```rust
//! let loregrep = LoreGrep::builder()
//!     .max_file_size(2 * 1024 * 1024)     // 2MB file size limit
//!     .max_depth(15)                       // Max directory depth
//!     .file_patterns(vec!["*.rs", "*.py"]) // File extensions to scan
//!     .exclude_patterns(vec!["target/"])   // Directories to skip
//!     .respect_gitignore(true)             // Honor .gitignore files
//!     .build()?;
//! ```
//!
//! ## Thread Safety
//!
//! All operations are thread-safe. Multiple threads can query the same `LoreGrep` instance 
//! concurrently. Scanning operations are synchronized to prevent data races.
//!
//! ```rust
//! use std::sync::Arc;
//! use tokio::task;
//!
//! let loregrep = Arc::new(loregrep);
//! 
//! // Multiple concurrent queries
//! let handles: Vec<_> = (0..10).map(|i| {
//!     let lg = loregrep.clone();
//!     task::spawn(async move {
//!         lg.execute_tool("search_functions", json!({"pattern": "test"})).await
//!     })
//! }).collect();
//! ```
//!
//! ## Error Handling
//!
//! The library uses comprehensive error types for different failure modes:
//!
//! ```rust
//! use loregrep::{LoreGrep, LoreGrepError};
//!
//! match loregrep.scan("/invalid/path").await {
//!     Ok(result) => println!("Success: {:?}", result),
//!     Err(LoreGrepError::Io(e)) => println!("IO error: {}", e),
//!     Err(LoreGrepError::Parse(e)) => println!("Parse error: {}", e),
//!     Err(LoreGrepError::Config(e)) => println!("Config error: {}", e),
//!     Err(e) => println!("Other error: {}", e),
//! }
//! ```
//!
//! ## Use Cases
//!
//! - **AI Code Assistants**: Provide structured code context to LLMs
//! - **Code Search Tools**: Fast symbol and pattern searching
//! - **Refactoring Tools**: Impact analysis and dependency tracking
//! - **Documentation Generators**: Extract API surfaces automatically
//! - **Code Quality Tools**: Analyze code patterns and relationships
//!
//! ## Performance Notes
//!
//! - Indexes are built in memory for fast access
//! - Scanning is parallelized across CPU cores
//! - Query results are cached for repeated access
//! - Memory usage scales linearly with codebase size
//! - No external dependencies required at runtime
//!
//! ## Future Roadmap
//!
//! ### Language Support
//! - **Python Analyzer**: Full Python support with functions, classes, imports, and method calls
//! - **TypeScript/JavaScript Analyzers**: Support for modern JS/TS features including interfaces, types, and ES6+ syntax
//! - **Go Analyzer**: Package declarations, interfaces, and Go-specific function signatures
//!
//! ### Advanced Analysis Features  
//! - **Call Graph Analysis**: Function call extraction and visualization across files
//! - **Dependency Tracking**: Advanced import/export analysis and impact assessment
//! - **Incremental Updates**: Smart re-indexing when files change to avoid full rescans
//!
//! ### Performance & Optimization
//! - **Memory Optimization**: Improved handling of large repositories with better memory management
//! - **Query Performance**: Enhanced caching and lookup optimization for faster results
//! - **Database Persistence**: Optional disk-based storage for very large codebases
//!
//! ### Integration & Architecture
//! - **MCP Server Integration**: Standard Model Context Protocol interface for tool calling
//! - **Editor Integrations**: VS Code, IntelliJ, and other popular editor plugins
//! - **API Enhancements**: Additional tools and query capabilities for LLM integration

// ================================================================================================
// PUBLIC API EXPORTS
// ================================================================================================

// Internal modules (not part of public API)
mod types;
mod analyzers;
mod parser;
mod scanner;
mod storage;
pub(crate) mod internal;

// CLI module (temporary public access for binary, will be refactored in Task 4C.4)
#[doc(hidden)]
pub mod cli_main;

// Public API modules
pub mod core;
mod loregrep;

// ================================================================================================
// CLEAN PUBLIC API EXPORTS
// ================================================================================================

/// Main LoreGrep API - the primary interface for code analysis
///
/// Use [`LoreGrep::builder()`] to create and configure a new instance.
pub use crate::loregrep::{LoreGrep, LoreGrepBuilder};

/// Core types for tool definitions and results
///
/// These types are designed for seamless integration with LLM tool calling systems.
pub use crate::core::types::{ToolSchema, ToolResult, ScanResult};

/// Error handling types
///
/// All operations return `Result<T, LoreGrepError>` for consistent error handling.
pub use crate::core::errors::{LoreGrepError, Result};

/// Current library version
///
/// Useful for version checking and compatibility verification.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ================================================================================================
// RE-EXPORTS FOR COMPATIBILITY
// ================================================================================================

// NOTE: LoreGrepConfig is intentionally not exported as it's an implementation detail.
// Users should configure through the builder pattern instead.
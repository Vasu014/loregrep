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
//! ### Zero-Configuration Auto-Discovery (Recommended)
//!
//! ```rust
//! use loregrep::LoreGrep;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // One-line setup with automatic project detection
//!     let mut loregrep = LoreGrep::auto_discover(".")?;
//!     // 🔍 Detected project languages: rust, python
//!     // ✅ Rust analyzer registered successfully
//!     // ✅ Python analyzer registered successfully  
//!     // 📁 Configuring file patterns for detected languages
//!     // 🎆 LoreGrep configured with 2 language(s): rust, python
//!
//!     // Scan with comprehensive feedback
//!     let scan_result = loregrep.scan(".").await?;
//!     // 🔍 Starting repository scan... 📁 Found X files... 📊 Summary
//!     
//!     println!("Indexed {} files with {} functions", 
//!              scan_result.files_scanned, 
//!              scan_result.functions_found);
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Manual Configuration with Enhanced Builder
//!
//! ```rust
//! use loregrep::LoreGrep;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Full control with enhanced builder pattern
//!     let mut loregrep = LoreGrep::builder()
//!         .with_rust_analyzer()           // ✅ Real-time feedback
//!         .with_python_analyzer()         // ✅ Registration confirmation
//!         .optimize_for_performance()     // 🚀 Speed-optimized preset
//!         .exclude_test_dirs()            // 🚫 Skip test directories
//!         .max_file_size(1024 * 1024)     // 1MB limit
//!         .max_depth(10)                  // Directory depth limit
//!         .build()?;                      // 🎆 Configuration summary
//!
//!     let scan_result = loregrep.scan("/path/to/your/repo").await?;
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
//!     // Option 1: Zero-configuration setup
//!     let mut loregrep = LoreGrep::auto_discover(".")?;
//!     // Auto-detects languages and configures appropriate analyzers
//!     
//!     // Option 2: Manual setup with presets
//!     let mut loregrep = LoreGrep::rust_project(".")?;  // Rust-optimized
//!     // Or: LoreGrep::python_project(".")?  // Python-optimized
//!     // Or: LoreGrep::polyglot_project(".")?  // Multi-language
//!     
//!     // Scan with enhanced feedback
//!     loregrep.scan(".").await?;
//!
//!     // Tool 1: Search for functions (with file path information)
//!     let result = loregrep.execute_tool("search_functions", json!({
//!         "pattern": "parse",
//!         "limit": 20
//!     })).await?;
//!
//!     // Tool 2: Find function callers with cross-file analysis
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
//! ### CLI Interactive Mode
//!
//! ```bash
//! # Start interactive AI-powered query session
//! loregrep query --interactive
//! 
//! # Or run a single query
//! loregrep query "What functions handle authentication?"
//! ```
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
//! ### Enhanced Builder with Convenience Methods
//!
//! ```rust
//! use loregrep::LoreGrep;
//!
//! // Performance-optimized configuration
//! let fast_loregrep = LoreGrep::builder()
//!     .with_rust_analyzer()           // ✅ Analyzer registration feedback
//!     .optimize_for_performance()     // 🚀 512KB limit, depth 8, skip binaries
//!     .exclude_test_dirs()            // 🚫 Skip test directories  
//!     .exclude_vendor_dirs()          // 🚫 Skip vendor/dependencies
//!     .build()?;                      // 🎆 Configuration summary
//!
//! // Comprehensive analysis configuration  
//! let thorough_loregrep = LoreGrep::builder()
//!     .with_all_analyzers()           // ✅ All available language analyzers
//!     .comprehensive_analysis()       // 🔍 5MB limit, depth 20, more file types
//!     .include_config_files()         // ✅ Include TOML, JSON, YAML configs
//!     .build()?;
//!
//! // Traditional manual configuration (still supported)
//! let manual_loregrep = LoreGrep::builder()
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

// PyO3 imports for Python bindings
#[cfg(feature = "python")]
use pyo3::prelude::*;

// ================================================================================================
// CLEAN PUBLIC API EXPORTS
// ================================================================================================

/// Main LoreGrep API - the primary interface for code analysis
///
/// **Quick Start Options:**
/// - [`LoreGrep::auto_discover()`] - Zero-configuration setup with automatic project detection
/// - [`LoreGrep::builder()`] - Full control with enhanced builder pattern
/// - [`LoreGrep::rust_project()`] - Rust-optimized preset
/// - [`LoreGrep::python_project()`] - Python-optimized preset  
/// - [`LoreGrep::polyglot_project()`] - Multi-language preset
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

/// Creates the Python module
#[cfg(feature = "python")]
#[pymodule]
#[pyo3(name = "loregrep")]
fn loregrep_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Register the main high-level API only
    m.add_class::<python_bindings::PyLoreGrep>()?;
    m.add_class::<python_bindings::PyLoreGrepBuilder>()?;
    m.add_class::<python_bindings::PyScanResult>()?;  
    m.add_class::<python_bindings::PyToolResult>()?;
    m.add_class::<python_bindings::PyToolSchema>()?;
    
    // Add module version
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    
    Ok(())
}

#[cfg(feature = "python")]
pub mod python_bindings {
    use super::*;
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    use crate::loregrep::{LoreGrep, LoreGrepBuilder};
    use crate::core::types::{ScanResult, ToolResult};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    /// High-level Python API for LoreGrep - matches the Rust API exactly
    #[pyclass(name = "LoreGrep")]
    pub struct PyLoreGrep {
        inner: Arc<Mutex<LoreGrep>>,
    }

    #[pymethods]
    impl PyLoreGrep {
        /// Create a new LoreGrep builder for manual configuration
        #[staticmethod]
        fn builder() -> PyLoreGrepBuilder {
            PyLoreGrepBuilder {
                inner: LoreGrep::builder(),
            }
        }

        /// Zero-configuration setup with automatic project detection
        #[staticmethod]
        fn auto_discover(path: &str) -> PyResult<PyLoreGrep> {
            let loregrep = LoreGrep::auto_discover(path)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Auto-discovery failed: {}", e)))?;
            
            Ok(PyLoreGrep { inner: Arc::new(Mutex::new(loregrep)) })
        }

        /// Rust-optimized preset configuration
        #[staticmethod]
        fn rust_project(path: &str) -> PyResult<PyLoreGrep> {
            let loregrep = LoreGrep::rust_project(path)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Rust project setup failed: {}", e)))?;
            
            Ok(PyLoreGrep { inner: Arc::new(Mutex::new(loregrep)) })
        }

        /// Python-optimized preset configuration
        #[staticmethod]
        fn python_project(path: &str) -> PyResult<PyLoreGrep> {
            let loregrep = LoreGrep::python_project(path)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Python project setup failed: {}", e)))?;
            
            Ok(PyLoreGrep { inner: Arc::new(Mutex::new(loregrep)) })
        }

        /// Multi-language preset configuration
        #[staticmethod]
        fn polyglot_project(path: &str) -> PyResult<PyLoreGrep> {
            let loregrep = LoreGrep::polyglot_project(path)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Polyglot project setup failed: {}", e)))?;
            
            Ok(PyLoreGrep { inner: Arc::new(Mutex::new(loregrep)) })
        }

        /// Scan a repository and build the index
        fn scan<'py>(&self, py: Python<'py>, path: &str) -> PyResult<Bound<'py, PyAny>> {
            let inner = self.inner.clone();
            let path = path.to_string();
            
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Clone the LoreGrep to avoid holding the mutex guard across await
                let mut loregrep = {
                    let guard = inner.lock()
                        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to acquire lock: {}", e)))?;
                    guard.clone()
                }; // mutex guard is dropped here
                
                let result = loregrep.scan(&path).await
                    .map_err(|e| match e {
                        crate::LoreGrepError::IoError(io_err) => PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("IO error during scan: {}", io_err)),
                        crate::LoreGrepError::AnalysisError(analysis_err) => PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Analysis error: {}", analysis_err)),
                        _ => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Scan failed: {}", e)),
                    })?;
                
                // Update the shared state with the scanned data
                {
                    let mut guard = inner.lock()
                        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to acquire lock for update: {}", e)))?;
                    *guard = loregrep;
                } // mutex guard is dropped here
                
                Ok(PyScanResult {
                    files_scanned: result.files_scanned,
                    functions_found: result.functions_found,
                    structs_found: result.structs_found,
                    errors: Vec::new(), // TODO: Collect actual errors from scan
                    duration_ms: result.duration_ms,
                })
            })
        }

        /// Execute one of the 6 AI tools
        fn execute_tool<'py>(&self, py: Python<'py>, tool_name: &str, args: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
            let inner = self.inner.clone();
            let tool_name = tool_name.to_string();
            
            // Convert PyDict to serde_json::Value with better error handling
            let args_json: Value = pythonize::depythonize(args)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid tool arguments - could not convert to JSON: {}", e)))?;
            
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Clone the LoreGrep to avoid holding the mutex guard across await
                let loregrep = {
                    let guard = inner.lock()
                        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to acquire lock: {}", e)))?;
                    guard.clone()
                }; // mutex guard is dropped here
                    
                let result = loregrep.execute_tool(&tool_name, args_json).await
                    .map_err(|e| match e {
                        crate::LoreGrepError::ToolError(tool_err) => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Tool '{}' execution failed: {}", tool_name, tool_err)),
                        crate::LoreGrepError::JsonError(json_err) => PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Tool JSON error: {}", json_err)),
                        crate::LoreGrepError::IoError(io_err) => PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("Tool IO error: {}", io_err)),
                        _ => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Tool execution failed: {}", e)),
                    })?;
                
                // Convert ToolResult to Python-compatible format with better error handling
                let metadata_str = serde_json::to_string(&result.data)
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to serialize tool result metadata: {}", e)))?;
                
                let content = if result.success {
                    serde_json::to_string(&result.data)
                        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to serialize tool result data: {}", e)))?
                } else {
                    result.error.unwrap_or_else(|| "Unknown tool error".to_string())
                };
                
                Ok(PyToolResult {
                    content,
                    metadata: metadata_str,
                })
            })
        }

        /// Get available tool definitions for AI systems
        #[staticmethod]
        fn get_tool_definitions() -> Vec<PyToolSchema> {
            LoreGrep::get_tool_definitions()
                .iter()
                .map(|schema| {
                    PyToolSchema {
                        name: schema.name.clone(),
                        description: schema.description.clone(),
                        parameters: serde_json::to_string(&schema.input_schema).unwrap_or_else(|_| "{}".to_string()),
                    }
                })
                .collect()
        }

        /// Get current version
        #[staticmethod]
        fn version() -> &'static str {
            env!("CARGO_PKG_VERSION")
        }

        fn __repr__(&self) -> String {
            "LoreGrep(configured and ready for repository analysis)".to_string()
        }
    }

    /// Python wrapper for LoreGrepBuilder - enables fluent configuration
    #[pyclass(name = "LoreGrepBuilder")]
    pub struct PyLoreGrepBuilder {
        inner: LoreGrepBuilder,
    }

    #[pymethods]
    impl PyLoreGrepBuilder {
        /// Set maximum file size to process
        fn max_file_size(mut slf: PyRefMut<Self>, size: u64) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().max_file_size(size);
            slf
        }

        /// Set maximum directory depth to scan
        fn max_depth(mut slf: PyRefMut<Self>, depth: u32) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().max_depth(depth);
            slf
        }

        /// Set file patterns to include
        fn file_patterns(mut slf: PyRefMut<Self>, patterns: Vec<String>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().file_patterns(patterns);
            slf
        }

        /// Set patterns to exclude
        fn exclude_patterns(mut slf: PyRefMut<Self>, patterns: Vec<String>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().exclude_patterns(patterns);
            slf
        }

        /// Enable or disable gitignore respect
        fn respect_gitignore(mut slf: PyRefMut<Self>, respect: bool) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().respect_gitignore(respect);
            slf
        }

        /// Add Rust language analyzer with feedback
        fn with_rust_analyzer(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().with_rust_analyzer();
            slf
        }

        /// Add Python language analyzer with feedback
        fn with_python_analyzer(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().with_python_analyzer();
            slf
        }

        /// Enable all available analyzers
        fn with_all_analyzers(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().with_all_analyzers();
            slf
        }

        /// Optimize for performance (speed-focused configuration)
        fn optimize_for_performance(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().optimize_for_performance();
            slf
        }

        /// Comprehensive analysis (thorough configuration)
        fn comprehensive_analysis(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().comprehensive_analysis();
            slf
        }

        /// Exclude common build directories
        fn exclude_common_build_dirs(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().exclude_common_build_dirs();
            slf
        }

        /// Exclude test directories
        fn exclude_test_dirs(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().exclude_test_dirs();
            slf
        }

        /// Exclude vendor/dependency directories
        fn exclude_vendor_dirs(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().exclude_vendor_dirs();
            slf
        }

        /// Include common source files
        fn include_source_files(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().include_source_files();
            slf
        }

        /// Include configuration files
        fn include_config_files(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().include_config_files();
            slf
        }

        /// Build the configured LoreGrep instance
        fn build(slf: PyRefMut<Self>) -> PyResult<PyLoreGrep> {
            let loregrep = slf.inner.clone().build()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Build failed: {}", e)))?;
            
            Ok(PyLoreGrep { inner: Arc::new(Mutex::new(loregrep)) })
        }

        fn __repr__(&self) -> String {
            "LoreGrepBuilder(configurable repository analyzer)".to_string()
        }
    }

    /// Python wrapper for ScanResult
    #[pyclass(name = "ScanResult")]
    pub struct PyScanResult {
        #[pyo3(get)]
        pub files_scanned: usize,
        #[pyo3(get)]
        pub functions_found: usize,
        #[pyo3(get)]
        pub structs_found: usize,
        #[pyo3(get)]
        pub errors: Vec<String>,
        #[pyo3(get)]
        pub duration_ms: u64,
    }

    #[pymethods]
    impl PyScanResult {
        fn __repr__(&self) -> String {
            format!(
                "ScanResult(files={}, functions={}, structs={}, duration={}ms)",
                self.files_scanned, self.functions_found, self.structs_found, self.duration_ms
            )
        }
    }

    /// Python wrapper for ToolResult
    #[pyclass(name = "ToolResult")]
    pub struct PyToolResult {
        #[pyo3(get)]
        pub content: String,
        #[pyo3(get)]
        pub metadata: String,
    }

    #[pymethods]
    impl PyToolResult {
        fn __repr__(&self) -> String {
            format!("ToolResult(content_len={})", self.content.len())
        }
    }

    /// Python wrapper for ToolSchema
    #[pyclass(name = "ToolSchema")]
    pub struct PyToolSchema {
        #[pyo3(get)]
        pub name: String,
        #[pyo3(get)]
        pub description: String,
        #[pyo3(get)]
        pub parameters: String,
    }

    #[pymethods]
    impl PyToolSchema {
        fn __repr__(&self) -> String {
            format!("ToolSchema(name='{}')", self.name)
        }
    }
}

// Re-export Python types when python feature is enabled
#[cfg(feature = "python")]
pub use python_bindings::*; 
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
//! - **Provides** standardized tools that coding assistants can call to query the codebase
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
//! │ Coding Assistant│◀───│   Query Tools    │◀───│   Fast Lookups  │
//! │   (Claude, GPT, │    │ (search, analyze,│    │  (functions,    │
//! │   Cursor, etc.) │    │  dependencies)   │    │   structs, etc.)│
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ### Zero-Configuration Auto-Discovery (Recommended)
//!
//! ```ignore
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
//! ```ignore
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
//! The library provides standardized tools that AI coding assistants can call:
//!
//! ```ignore
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
//! ```ignore
//! // Get tool definitions for your AI system
//! let tools = LoreGrep::get_tool_definitions();
//!
//! // Current tool set (get_tool_definitions() is always authoritative):
//! // 1. search_functions      - Find functions by name/pattern
//! // 2. search_structs        - Find structures by name/pattern
//! // 3. analyze_file          - Get detailed file analysis
//! // 4. get_dependencies      - Find imports/exports for a file
//! // 5. find_callers          - Get direct function call sites
//! // 6. trace_callers         - Trace transitive (upstream) callers via the call graph
//! // 7. analyze_impact        - Compute change blast radius via the call graph
//! // 8. get_repository_tree   - Get repository structure and overview
//! // 9. find_importers        - Find files that import a given file
//! // 10. get_dependency_graph - Resolved import graph plus import cycles
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
//! - **Tool System**: standardized tools for AI integration
//!
//! ### Design Characteristics
//!
//! - **Architecture**: Fast in-memory indexing with tree-sitter parsing
//! - **Concurrency**: Thread-safe with `Arc<Mutex<>>` design
//! - **Scalability**: Memory usage scales linearly with codebase size
//!
//! ## Language Support
//!
//! | Language   | Status     | Functions | Structs | Imports | Calls |
//! |------------|------------|-----------|---------|---------|-------|
//! | Rust       | ✅ Full    | ✅        | ✅      | ✅      | ✅    |
//! | Python     | ✅ Full    | ✅        | ✅      | ✅      | ✅    |
//! | TypeScript | ✅ Full    | ✅        | ✅      | ✅      | ✅    |
//! | JavaScript | 📋 Roadmap | -         | -       | -       | -     |
//! | Go         | 📋 Roadmap | -         | -       | -       | -     |
//!
//! *Note: Languages marked "📋 Roadmap" are future planned additions.*
//!
//! ## Integration Examples
//!
//! ### With Claude/OpenAI
//!
//! ```ignore
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
//! ```ignore
//! // MCP server integration is planned for future releases
//! // Will provide standard MCP interface for tool calling
//! ```
//!
//! ## Configuration Options
//!
//! ### Enhanced Builder with Convenience Methods
//!
//! ```ignore
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
//! ```ignore
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
//! ```ignore
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
//! - **JavaScript Analyzer**: Support for modern JS features including ES6+ syntax (TypeScript/TSX is already supported)
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
mod analyzers;
pub(crate) mod internal;
mod parser;
mod scanner;
mod storage;
mod types;

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
pub use crate::core::types::{ScanResult, ToolResult, ToolSchema};

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
    m.add_class::<python_bindings::PyIndexCoverage>()?;

    // Add module version
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}

#[cfg(feature = "python")]
pub mod python_bindings {
    use super::*;
    use crate::core::types::ScanResult;
    use crate::loregrep::{LoreGrep, LoreGrepBuilder};
    use pyo3::types::PyDict;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    // ============================================================================
    // API PARITY MAP  (Rust `LoreGrep`/`LoreGrepBuilder` -> Python)
    //
    // These bindings are meant to be a COMPLETE mirror of the Rust public API.
    // Every `pub fn` on `LoreGrep` and `LoreGrepBuilder` in `src/loregrep.rs` is
    // either bound below or listed here with the reason it is not. If you add a
    // `pub fn` there, add it here too — this list is the audit trail that stops
    // the bindings from silently drifting behind the Rust surface again.
    //
    // Deliberately NOT exposed:
    //
    //   LoreGrep::save_index
    //   LoreGrep::load_index
    //   LoreGrep::load_index_if_fresh
    //   LoreGrep::is_cache_fresh
    //   LoreGrep::cache_path_for   (and whatever else this group becomes)
    //       The on-disk index cache. Deferred, not rejected: these signatures are
    //       actively being reworked, so binding them now would bind an API about
    //       to change. Bind them as a group once they settle.
    //
    //   (`with_go_analyzer` and `cache_ttl` are absent from BOTH surfaces now —
    //   they were no-ops, and a bound no-op tells an agent a feature exists and
    //   then fails silently. See the comments at their former sites in
    //   `src/loregrep.rs`. Re-binding them when they do something is additive.)
    //
    //   LoreGrep::coverage_handle
    //       Returns a `CoverageHandle`, internal plumbing that exists so the tool
    //       layer can share the coverage cell with `LoreGrep`. A Python host has
    //       nothing to wire it into; `index_coverage()` gives it the same data as
    //       a value.
    //
    // Everything else is bound below.
    // ============================================================================

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
            let loregrep = LoreGrep::auto_discover(path).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Auto-discovery failed: {}",
                    e
                ))
            })?;

            Ok(PyLoreGrep {
                inner: Arc::new(Mutex::new(loregrep)),
            })
        }

        /// Rust-optimized preset configuration
        #[staticmethod]
        fn rust_project(path: &str) -> PyResult<PyLoreGrep> {
            let loregrep = LoreGrep::rust_project(path).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Rust project setup failed: {}",
                    e
                ))
            })?;

            Ok(PyLoreGrep {
                inner: Arc::new(Mutex::new(loregrep)),
            })
        }

        /// Python-optimized preset configuration
        #[staticmethod]
        fn python_project(path: &str) -> PyResult<PyLoreGrep> {
            let loregrep = LoreGrep::python_project(path).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Python project setup failed: {}",
                    e
                ))
            })?;

            Ok(PyLoreGrep {
                inner: Arc::new(Mutex::new(loregrep)),
            })
        }

        /// Multi-language preset configuration
        #[staticmethod]
        fn polyglot_project(path: &str) -> PyResult<PyLoreGrep> {
            let loregrep = LoreGrep::polyglot_project(path).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Polyglot project setup failed: {}",
                    e
                ))
            })?;

            Ok(PyLoreGrep {
                inner: Arc::new(Mutex::new(loregrep)),
            })
        }

        /// Scan a repository and build the index
        fn scan<'py>(&self, py: Python<'py>, path: &str) -> PyResult<Bound<'py, PyAny>> {
            let inner = self.inner.clone();
            let path = path.to_string();

            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Clone the LoreGrep to avoid holding the mutex guard across await
                let mut loregrep = {
                    let guard = inner.lock().map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                            "Failed to acquire lock: {}",
                            e
                        ))
                    })?;
                    guard.clone()
                }; // mutex guard is dropped here

                let result = loregrep.scan(&path).await.map_err(|e| match e {
                    crate::LoreGrepError::IoError(io_err) => {
                        PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                            "IO error during scan: {}",
                            io_err
                        ))
                    }
                    crate::LoreGrepError::AnalysisError(analysis_err) => {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                            "Analysis error: {}",
                            analysis_err
                        ))
                    }
                    _ => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Scan failed: {}",
                        e
                    )),
                })?;

                // Update the shared state with the scanned data
                {
                    let mut guard = inner.lock().map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                            "Failed to acquire lock for update: {}",
                            e
                        ))
                    })?;
                    *guard = loregrep;
                } // mutex guard is dropped here

                Ok(PyScanResult::from_scan_result(result))
            })
        }

        /// Execute one of the AI tools (see `get_tool_definitions`)
        fn execute_tool<'py>(
            &self,
            py: Python<'py>,
            tool_name: &str,
            args: &Bound<'py, PyDict>,
        ) -> PyResult<Bound<'py, PyAny>> {
            let inner = self.inner.clone();
            let tool_name = tool_name.to_string();

            // Convert PyDict to serde_json::Value with better error handling
            let args_json: Value = pythonize::depythonize(args).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Invalid tool arguments - could not convert to JSON: {}",
                    e
                ))
            })?;

            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                // Clone the LoreGrep to avoid holding the mutex guard across await
                let loregrep = {
                    let guard = inner.lock().map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                            "Failed to acquire lock: {}",
                            e
                        ))
                    })?;
                    guard.clone()
                }; // mutex guard is dropped here

                let result =
                    loregrep
                        .execute_tool(&tool_name, args_json)
                        .await
                        .map_err(|e| match e {
                            crate::LoreGrepError::ToolError(tool_err) => {
                                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                                    "Tool '{}' execution failed: {}",
                                    tool_name, tool_err
                                ))
                            }
                            crate::LoreGrepError::JsonError(json_err) => {
                                PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                                    "Tool JSON error: {}",
                                    json_err
                                ))
                            }
                            crate::LoreGrepError::IoError(io_err) => {
                                PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                                    "Tool IO error: {}",
                                    io_err
                                ))
                            }
                            _ => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                                "Tool execution failed: {}",
                                e
                            )),
                        })?;

                // Convert ToolResult to Python-compatible format with better error handling
                let metadata_str = serde_json::to_string(&result.data).map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to serialize tool result metadata: {}",
                        e
                    ))
                })?;

                let content = if result.success {
                    serde_json::to_string(&result.data).map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                            "Failed to serialize tool result data: {}",
                            e
                        ))
                    })?
                } else {
                    result
                        .error
                        .unwrap_or_else(|| "Unknown tool error".to_string())
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
                .map(|schema| PyToolSchema {
                    name: schema.name.clone(),
                    description: schema.description.clone(),
                    parameters: serde_json::to_string(&schema.input_schema)
                        .unwrap_or_else(|_| "{}".to_string()),
                })
                .collect()
        }

        /// Get current version
        #[staticmethod]
        fn version() -> &'static str {
            env!("CARGO_PKG_VERSION")
        }

        /// Repository statistics for the current in-memory index.
        ///
        /// Unlike `scan()` this does not touch the filesystem; `duration_ms` is
        /// always 0.
        fn get_stats(&self) -> PyResult<PyScanResult> {
            let loregrep = self.lock()?;
            let stats = loregrep.get_stats().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to read repository stats: {}",
                    e
                ))
            })?;
            Ok(PyScanResult::from_scan_result(stats))
        }

        /// How much of the repository the current index covers.
        ///
        /// Consult this before reporting "not found" to a user or an agent: a
        /// `max_files` limit can truncate the index, and an empty result set
        /// from a truncated index does not mean the symbol does not exist.
        fn index_coverage(&self) -> PyResult<PyIndexCoverage> {
            let coverage = self.lock()?.index_coverage();
            Ok(PyIndexCoverage {
                files_indexed: coverage.files_indexed,
                files_discovered: coverage.files_discovered,
                truncated: coverage.truncated,
                note: coverage.note(),
            })
        }

        /// Whether a repository has been scanned into this index.
        fn is_scanned(&self) -> PyResult<bool> {
            Ok(self.lock()?.is_scanned())
        }

        /// Record the analysis root on the index.
        ///
        /// A scan sets this itself, so this is only needed by hosts that
        /// populate an index by other means.
        fn set_scan_root(&self, root: &str) -> PyResult<()> {
            self.lock()?.set_scan_root(root);
            Ok(())
        }

        /// The first indexed file path that no longer exists on disk, if any.
        ///
        /// A cheap staleness probe: `None` means every indexed path still
        /// resolves under the recorded analysis root.
        fn first_missing_indexed_path(&self) -> PyResult<Option<String>> {
            Ok(self.lock()?.first_missing_indexed_path())
        }

        /// Reset the in-memory index to empty, including its coverage.
        ///
        /// `scan()` replaces the index wholesale, so this is not required before
        /// a rescan; use it to release the memory of an index you are done with
        /// while keeping the configured instance alive.
        fn clear_index(&self) -> PyResult<()> {
            self.lock()?.clear_index();
            Ok(())
        }

        fn __repr__(&self) -> String {
            "LoreGrep(configured and ready for repository analysis)".to_string()
        }
    }

    impl PyLoreGrep {
        /// Lock the shared instance, turning a poisoned mutex into a Python
        /// exception rather than a panic across the FFI boundary.
        fn lock(&self) -> PyResult<std::sync::MutexGuard<'_, LoreGrep>> {
            self.inner.lock().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to acquire lock: {}",
                    e
                ))
            })
        }
    }

    /// Python wrapper for LoreGrepBuilder - enables fluent configuration
    #[pyclass(name = "LoreGrepBuilder")]
    pub struct PyLoreGrepBuilder {
        inner: LoreGrepBuilder,
    }

    #[pymethods]
    impl PyLoreGrepBuilder {
        /// Create a builder with the default configuration
        ///
        /// Equivalent to `LoreGrep.builder()`.
        #[new]
        fn new() -> Self {
            PyLoreGrepBuilder {
                inner: LoreGrepBuilder::new(),
            }
        }

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

        /// Set maximum directory depth to unlimited
        fn unlimited_depth(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().unlimited_depth();
            slf
        }

        /// Set the maximum number of files to index
        ///
        /// A scan that hits this limit produces a TRUNCATED index; check
        /// `LoreGrep.index_coverage()` before treating an empty result as
        /// "does not exist".
        fn max_files(mut slf: PyRefMut<Self>, limit: usize) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().max_files(limit);
            slf
        }

        /// Set file patterns to include
        fn file_patterns(mut slf: PyRefMut<Self>, patterns: Vec<String>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().file_patterns(patterns);
            slf
        }

        /// Set file patterns to include (same as `file_patterns`)
        fn include_patterns(mut slf: PyRefMut<Self>, patterns: Vec<String>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().include_patterns(patterns);
            slf
        }

        /// Enable or disable following symbolic links while scanning
        fn follow_symlinks(mut slf: PyRefMut<Self>, follow: bool) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().follow_symlinks(follow);
            slf
        }

        /// Configure file patterns from a list of language names
        ///
        /// e.g. `["rust", "python"]` -> `*.rs`, `*.py`, ...
        fn configure_patterns_for_languages(
            mut slf: PyRefMut<Self>,
            languages: Vec<String>,
        ) -> PyRefMut<Self> {
            slf.inner = slf
                .inner
                .clone()
                .configure_patterns_for_languages(&languages);
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

        /// Add TypeScript/TSX language analyzer
        fn with_typescript_analyzer(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
            slf.inner = slf.inner.clone().with_typescript_analyzer();
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
            let loregrep = slf.inner.clone().build().map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Build failed: {}", e))
            })?;

            Ok(PyLoreGrep {
                inner: Arc::new(Mutex::new(loregrep)),
            })
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
        #[pyo3(get)]
        pub languages: Vec<String>,
    }

    impl PyScanResult {
        fn from_scan_result(result: ScanResult) -> Self {
            PyScanResult {
                files_scanned: result.files_scanned,
                functions_found: result.functions_found,
                structs_found: result.structs_found,
                errors: Vec::new(), // TODO: Collect actual errors from scan
                duration_ms: result.duration_ms,
                languages: result.languages,
            }
        }
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

    /// Python wrapper for IndexCoverage - how much of the repository is indexed
    #[pyclass(name = "IndexCoverage")]
    pub struct PyIndexCoverage {
        /// Files actually analyzed and stored in the index
        #[pyo3(get)]
        pub files_indexed: usize,
        /// Files discovery found before any `max_files` limit was applied
        #[pyo3(get)]
        pub files_discovered: usize,
        /// True when a limit stopped the scan short of everything discovered
        #[pyo3(get)]
        pub truncated: bool,
        /// A human-readable coverage note, or None when the index is complete
        #[pyo3(get)]
        pub note: Option<String>,
    }

    #[pymethods]
    impl PyIndexCoverage {
        fn __repr__(&self) -> String {
            format!(
                "IndexCoverage(indexed={}, discovered={}, truncated={})",
                self.files_indexed,
                self.files_discovered,
                // Python spelling: this repr is read in a Python REPL.
                if self.truncated { "True" } else { "False" }
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

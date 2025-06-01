//! # LoreGrep - AI-Powered Code Analysis Library
//!
//! LoreGrep is an in-memory code repository analysis library designed for integration into coding assistants 
//! and LLM-powered development tools. It provides a tool-based interface that can be easily integrated into 
//! any AI assistant's tool calling system.
//!
//! ## Core Design Principles
//!
//! 1. **Tool-Based Interface**: All functionality exposed through LLM-compatible tool definitions
//! 2. **Host-Managed Scanning**: Repository scanning is controlled by the host application, not the LLM
//! 3. **Language Agnostic**: Extensible architecture supporting multiple programming languages
//! 4. **Memory Efficient**: Fast in-memory indexing optimized for code analysis
//! 5. **Type Safe**: Strong typing with comprehensive error handling
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use loregrep::{LoreGrep, ToolSchema};
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize LoreGrep
//! let mut loregrep = LoreGrep::builder()
//!     .with_rust_analyzer()
//!     .max_files(10000)
//!     .build()?;
//!
//! // Scan repository (host-managed)
//! let scan_result = loregrep.scan("/path/to/repo").await?;
//! println!("Scanned {} files", scan_result.files_scanned);
//!
//! // Get tool definitions for LLM
//! let tools = LoreGrep::get_tool_definitions();
//! let tools_json = serde_json::to_string_pretty(&tools)?;
//!
//! // Execute tool calls from LLM
//! let result = loregrep.execute_tool("search_functions", json!({
//!     "pattern": "handle_.*",
//!     "limit": 10
//! })).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Tool-Based Interface
//!
//! LoreGrep exposes 6 tools for LLM consumption:
//!
//! - **search_functions**: Search for functions by pattern across the analyzed codebase
//! - **search_structs**: Search for structs/classes by pattern across the analyzed codebase  
//! - **analyze_file**: Analyze a specific file to extract functions, structs, imports, etc.
//! - **get_dependencies**: Get import/export dependencies for a file
//! - **find_callers**: Find all locations where a specific function is called
//! - **get_repository_tree**: Get complete repository information and directory structure
//!
//! ## Integration with Coding Assistants
//!
//! ```rust,no_run
//! use loregrep::LoreGrep;
//! use serde_json::Value;
//!
//! pub struct CodingAssistant {
//!     loregrep: LoreGrep,
//! }
//!
//! impl CodingAssistant {
//!     pub async fn initialize(project_path: &str) -> loregrep::Result<Self> {
//!         // Initialize and scan
//!         let mut loregrep = LoreGrep::builder().build()?;
//!         loregrep.scan(project_path).await?;
//!         
//!         Ok(Self { loregrep })
//!     }
//!     
//!     pub async fn handle_llm_tool_call(&self, tool_name: &str, params: Value) -> loregrep::Result<Value> {
//!         // Execute tool and return result
//!         let result = self.loregrep.execute_tool(tool_name, params).await?;
//!         Ok(serde_json::to_value(result)?)
//!     }
//!     
//!     pub async fn refresh_index(&mut self, path: &str) -> loregrep::Result<()> {
//!         // Rescan when files change
//!         self.loregrep.scan(path).await?;
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Memory Usage**: ~10KB per analyzed file
//! - **Scan Speed**: ~1000 files/second on modern hardware
//! - **Query Speed**: <1ms for most queries on repos with <10k files
//! - **Thread Safety**: All operations are thread-safe
//!
//! ## Error Handling
//!
//! All operations return `Result<T, LoreGrepError>` for consistent error handling:
//!
//! ```rust,no_run
//! use loregrep::{LoreGrep, LoreGrepError};
//! use serde_json::json;
//!
//! # async fn example() {
//! # let loregrep = LoreGrep::builder().build().unwrap();
//! match loregrep.execute_tool("search_functions", json!({"pattern": "test"})).await {
//!     Ok(result) => {
//!         if result.success {
//!             // Process result.data
//!             println!("Success: {}", result.data);
//!         } else {
//!             // Handle tool-specific error in result.error
//!             eprintln!("Tool error: {:?}", result.error);
//!         }
//!     }
//!     Err(LoreGrepError::ToolError(e)) => {
//!         eprintln!("Tool execution failed: {}", e);
//!     }
//!     Err(e) => {
//!         eprintln!("System error: {}", e);
//!     }
//! }
//! # }
//! ```

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
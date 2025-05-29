use async_trait::async_trait;
use tree_sitter::Tree;
use crate::types::{
    Result, FileAnalysis, PartialAnalysis, FunctionSignature, StructSignature, 
    ImportStatement, ExportStatement, FunctionCall
};

#[async_trait]
pub trait LanguageAnalyzer: Send + Sync {
    /// The language this analyzer supports
    fn language(&self) -> &'static str;
    
    /// File extensions supported by this analyzer
    fn file_extensions(&self) -> &[&'static str];
    
    /// Whether this language supports async functions
    fn supports_async(&self) -> bool;
    
    /// Analyze a complete file and return comprehensive analysis
    async fn analyze_file(&self, content: &str, file_path: &str) -> Result<FileAnalysis>;
    
    /// Extract function signatures from a parsed tree
    fn extract_functions(&self, tree: &Tree, source: &str) -> Result<Vec<FunctionSignature>>;
    
    /// Extract struct/class definitions from a parsed tree
    fn extract_structs(&self, tree: &Tree, source: &str) -> Result<Vec<StructSignature>>;
    
    /// Extract import statements from a parsed tree
    fn extract_imports(&self, tree: &Tree, source: &str) -> Result<Vec<ImportStatement>>;
    
    /// Extract export statements from a parsed tree
    fn extract_exports(&self, tree: &Tree, source: &str) -> Result<Vec<ExportStatement>>;
    
    /// Extract function calls from a parsed tree
    fn extract_function_calls(&self, tree: &Tree, source: &str) -> Result<Vec<FunctionCall>>;
    
    /// Extract as much as possible when normal parsing fails
    fn extract_with_fallback(&self, content: &str, file_path: &str) -> PartialAnalysis;
} 
use serde::{Serialize, Deserialize};
use crate::types::{FunctionSignature, StructSignature, ImportStatement, ExportStatement, FunctionCall};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub file_path: String,
    pub language: String,
    pub imports: Vec<ImportStatement>,
    pub exports: Vec<ExportStatement>,
    pub functions: Vec<FunctionSignature>,
    pub structs: Vec<StructSignature>,
    pub function_calls: Vec<FunctionCall>,
    pub content_hash: String,
    pub last_modified: std::time::SystemTime,
    pub parse_errors: Vec<String>,
}

impl TreeNode {
    pub fn new(file_path: String, language: String) -> Self {
        Self {
            file_path,
            language,
            imports: Vec::new(),
            exports: Vec::new(),
            functions: Vec::new(),
            structs: Vec::new(),
            function_calls: Vec::new(),
            content_hash: String::new(),
            last_modified: std::time::SystemTime::now(),
            parse_errors: Vec::new(),
        }
    }

    /// Convert to JSON string for easy display/storage
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// Get summary stats for terminal display
    pub fn summary(&self) -> String {
        format!(
            "File: {} | Language: {} | Functions: {} | Structs: {} | Imports: {} | Exports: {}",
            self.file_path,
            self.language,
            self.functions.len(),
            self.structs.len(),
            self.imports.len(),
            self.exports.len()
        )
    }
    
    /// Get formatted function list for terminal display
    pub fn format_functions(&self) -> Vec<String> {
        self.functions.iter().map(|f| f.format()).collect()
    }
    
    /// Get formatted struct list for terminal display
    pub fn format_structs(&self) -> Vec<String> {
        self.structs.iter().map(|s| s.format()).collect()
    }

    /// Check if the file has parse errors
    pub fn has_errors(&self) -> bool {
        !self.parse_errors.is_empty()
    }

    /// Add a parse error
    pub fn add_error(&mut self, error: String) {
        self.parse_errors.push(error);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    pub tree_node: TreeNode,
    pub analysis_duration_ms: u64,
    pub success: bool,
}

impl FileAnalysis {
    pub fn new(tree_node: TreeNode, analysis_duration_ms: u64) -> Self {
        Self {
            success: !tree_node.has_errors(),
            tree_node,
            analysis_duration_ms,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PartialAnalysis {
    pub file_path: String,
    pub language: String,
    pub functions: Vec<FunctionSignature>,
    pub structs: Vec<StructSignature>,
    pub errors: Vec<String>,
    pub fallback_used: bool,
}

impl PartialAnalysis {
    pub fn new(file_path: String, language: String) -> Self {
        Self {
            file_path,
            language,
            functions: Vec::new(),
            structs: Vec::new(),
            errors: Vec::new(),
            fallback_used: false,
        }
    }

    pub fn with_fallback(mut self) -> Self {
        self.fallback_used = true;
        self
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }
} 
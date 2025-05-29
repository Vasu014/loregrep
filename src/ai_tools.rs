use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use anyhow::{Result, Context};

use crate::anthropic::ToolSchema;
use crate::storage::memory::RepoMap;
use crate::scanner::discovery::RepositoryScanner;
use crate::analyzers::{rust::RustAnalyzer, traits::LanguageAnalyzer};

pub struct LocalAnalysisTools {
    repo_map: Arc<RepoMap>,
    scanner: RepositoryScanner,
    rust_analyzer: RustAnalyzer,
}

impl LocalAnalysisTools {
    pub fn new(
        repo_map: Arc<RepoMap>,
        scanner: RepositoryScanner,
        rust_analyzer: RustAnalyzer,
    ) -> Self {
        Self {
            repo_map,
            scanner,
            rust_analyzer,
        }
    }

    pub fn get_tool_schemas(&self) -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "scan_repository".to_string(),
                description: "Scan a repository directory to analyze all code files and build an index".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the repository directory to scan"
                        },
                        "include_patterns": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "File patterns to include (e.g., ['*.rs', '*.py'])"
                        },
                        "exclude_patterns": {
                            "type": "array", 
                            "items": {"type": "string"},
                            "description": "File patterns to exclude (e.g., ['target/', '*.test.js'])"
                        }
                    },
                    "required": ["path"]
                }),
            },
            ToolSchema {
                name: "search_functions".to_string(),
                description: "Search for functions by name pattern or regex across the analyzed codebase".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern or regex to match function names"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return",
                            "default": 20
                        },
                        "language": {
                            "type": "string",
                            "description": "Filter by programming language (optional)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolSchema {
                name: "search_structs".to_string(),
                description: "Search for structs/classes by name pattern across the analyzed codebase".to_string(),
                input_schema: json!({
                    "type": "object", 
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Search pattern or regex to match struct/class names"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return",
                            "default": 20
                        },
                        "language": {
                            "type": "string",
                            "description": "Filter by programming language (optional)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolSchema {
                name: "analyze_file".to_string(),
                description: "Analyze a specific file to extract its functions, structs, imports, and other code elements".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to analyze"
                        },
                        "include_content": {
                            "type": "boolean",
                            "description": "Whether to include file content in the response",
                            "default": false
                        }
                    },
                    "required": ["file_path"]
                }),
            },
            ToolSchema {
                name: "get_dependencies".to_string(),
                description: "Get import/export dependencies for a file or analyze dependency relationships".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to analyze dependencies for"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
            ToolSchema {
                name: "find_callers".to_string(),
                description: "Find all locations where a specific function is called across the codebase".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "function_name": {
                            "type": "string",
                            "description": "Name of the function to find callers for"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return",
                            "default": 50
                        }
                    },
                    "required": ["function_name"]
                }),
            },
            ToolSchema {
                name: "get_repository_overview".to_string(),
                description: "Get a high-level overview of the repository including file counts, languages, and structure".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "include_file_list": {
                            "type": "boolean",
                            "description": "Whether to include a list of all files",
                            "default": false
                        }
                    }
                }),
            },
        ]
    }

    pub async fn execute_tool(&self, tool_name: &str, input: Value) -> Result<ToolResult> {
        match tool_name {
            "scan_repository" => self.scan_repository(input).await,
            "search_functions" => self.search_functions(input).await,
            "search_structs" => self.search_structs(input).await,
            "analyze_file" => self.analyze_file(input).await,
            "get_dependencies" => self.get_dependencies(input).await,
            "find_callers" => self.find_callers(input).await,
            "get_repository_overview" => self.get_repository_overview(input).await,
            _ => Ok(ToolResult::error(format!("Unknown tool: {}", tool_name))),
        }
    }

    async fn scan_repository(&self, input: Value) -> Result<ToolResult> {
        let scan_input: ScanRepositoryInput = serde_json::from_value(input)
            .context("Invalid scan_repository input")?;

        // Note: In a real implementation, we would actually scan here
        // For now, we'll return information about what would be scanned
        let result = json!({
            "status": "success",
            "message": format!("Repository scan initiated for path: {}", scan_input.path),
            "path": scan_input.path,
            "include_patterns": scan_input.include_patterns.unwrap_or_default(),
            "exclude_patterns": scan_input.exclude_patterns.unwrap_or_default(),
            "note": "Actual scanning implementation would go here"
        });

        Ok(ToolResult::success(result))
    }

    async fn search_functions(&self, input: Value) -> Result<ToolResult> {
        let search_input: SearchFunctionsInput = serde_json::from_value(input)
            .context("Invalid search_functions input")?;

        let results = self.repo_map.find_functions(&search_input.pattern);
        let limited_results: Vec<_> = results.items
            .into_iter()
            .take(search_input.limit.unwrap_or(20))
            .collect();

        let result = json!({
            "status": "success",
            "pattern": search_input.pattern,
            "results": limited_results,
            "count": limited_results.len()
        });

        Ok(ToolResult::success(result))
    }

    async fn search_structs(&self, input: Value) -> Result<ToolResult> {
        let search_input: SearchStructsInput = serde_json::from_value(input)
            .context("Invalid search_structs input")?;

        let results = self.repo_map.find_structs(&search_input.pattern);
        let limited_results: Vec<_> = results.items
            .into_iter()
            .take(search_input.limit.unwrap_or(20))
            .collect();

        let result = json!({
            "status": "success",
            "pattern": search_input.pattern,
            "results": limited_results,
            "count": limited_results.len()
        });

        Ok(ToolResult::success(result))
    }

    async fn analyze_file(&self, input: Value) -> Result<ToolResult> {
        let analyze_input: AnalyzeFileInput = serde_json::from_value(input)
            .context("Invalid analyze_file input")?;

        // Try to read the file and analyze it
        match tokio::fs::read_to_string(&analyze_input.file_path).await {
            Ok(content) => {
                let file_analysis = self.rust_analyzer.analyze_file(&content, &analyze_input.file_path).await?;
                
                let mut result = json!({
                    "status": "success",
                    "file_path": analyze_input.file_path,
                    "analysis": file_analysis.tree_node
                });

                if analyze_input.include_content.unwrap_or(false) {
                    result.as_object_mut().unwrap().insert("content".to_string(), json!(content));
                }

                Ok(ToolResult::success(result))
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "file_path": analyze_input.file_path,
                    "error": format!("Failed to read file: {}", e)
                });
                Ok(ToolResult::error_with_data(result))
            }
        }
    }

    async fn get_dependencies(&self, input: Value) -> Result<ToolResult> {
        let deps_input: GetDependenciesInput = serde_json::from_value(input)
            .context("Invalid get_dependencies input")?;

        let dependencies = self.repo_map.get_file_dependencies(&deps_input.file_path);

        let result = json!({
            "status": "success",
            "file_path": deps_input.file_path,
            "dependencies": dependencies
        });

        Ok(ToolResult::success(result))
    }

    async fn find_callers(&self, input: Value) -> Result<ToolResult> {
        let callers_input: FindCallersInput = serde_json::from_value(input)
            .context("Invalid find_callers input")?;

        let callers = self.repo_map.find_function_callers(&callers_input.function_name);
        let limited_callers: Vec<_> = callers
            .into_iter()
            .take(callers_input.limit.unwrap_or(50))
            .collect();

        let result = json!({
            "status": "success",
            "function_name": callers_input.function_name,
            "callers": limited_callers,
            "count": limited_callers.len()
        });

        Ok(ToolResult::success(result))
    }

    async fn get_repository_overview(&self, input: Value) -> Result<ToolResult> {
        let overview_input: GetRepositoryOverviewInput = serde_json::from_value(input).unwrap_or_default();

        let metadata = self.repo_map.get_metadata();
        let total_files = self.repo_map.file_count();
        let languages: Vec<String> = metadata.languages.iter().cloned().collect();

        let mut result = json!({
            "status": "success",
            "total_files": total_files,
            "languages": languages,
            "metadata": metadata
        });

        if overview_input.include_file_list.unwrap_or(false) {
            let files: Vec<_> = self.repo_map.get_all_files()
                .iter()
                .map(|f| f.file_path.clone())
                .collect();
            result.as_object_mut().unwrap().insert("files".to_string(), json!(files));
        }

        Ok(ToolResult::success(result))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub data: Value,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(data: Value) -> Self {
        Self {
            success: true,
            data,
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: json!({}),
            error: Some(message),
        }
    }

    pub fn error_with_data(data: Value) -> Self {
        Self {
            success: false,
            data,
            error: None,
        }
    }
}

// Input types for tool functions
#[derive(Debug, Deserialize)]
struct ScanRepositoryInput {
    path: String,
    include_patterns: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SearchFunctionsInput {
    pattern: String,
    limit: Option<usize>,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchStructsInput {
    pattern: String,
    limit: Option<usize>,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeFileInput {
    file_path: String,
    include_content: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GetDependenciesInput {
    file_path: String,
}

#[derive(Debug, Deserialize)]
struct FindCallersInput {
    function_name: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
struct GetRepositoryOverviewInput {
    include_file_list: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Helper to create minimal test instances
    fn create_test_repo_map() -> Arc<RepoMap> {
        Arc::new(RepoMap::new())
    }

    fn create_test_scanner() -> RepositoryScanner {
        // Create with minimal config
        let config = crate::scanner::discovery::FileScanningConfig {
            include_patterns: vec!["*.rs".to_string()],
            exclude_patterns: vec![],
            max_file_size: 1024 * 1024,
            follow_symlinks: false,
            max_depth: Some(10),
        };
        RepositoryScanner::new(&config, None).unwrap()
    }

    fn create_test_analyzer() -> RustAnalyzer {
        RustAnalyzer::new().unwrap()
    }

    fn create_mock_tools() -> LocalAnalysisTools {
        let repo_map = create_test_repo_map();
        let scanner = create_test_scanner();
        let rust_analyzer = create_test_analyzer();
        
        LocalAnalysisTools::new(repo_map, scanner, rust_analyzer)
    }

    #[test]
    fn test_tool_schemas_creation() {
        let tools = create_mock_tools();
        let schemas = tools.get_tool_schemas();
        
        assert_eq!(schemas.len(), 7);
        
        let tool_names: Vec<_> = schemas.iter().map(|s| &s.name).collect();
        assert!(tool_names.contains(&&"scan_repository".to_string()));
        assert!(tool_names.contains(&&"search_functions".to_string()));
        assert!(tool_names.contains(&&"search_structs".to_string()));
        assert!(tool_names.contains(&&"analyze_file".to_string()));
        assert!(tool_names.contains(&&"get_dependencies".to_string()));
        assert!(tool_names.contains(&&"find_callers".to_string()));
        assert!(tool_names.contains(&&"get_repository_overview".to_string()));
    }

    #[tokio::test]
    async fn test_scan_repository_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "path": "/test/path",
            "include_patterns": ["*.rs"],
            "exclude_patterns": ["target/"]
        });

        let result = tools.execute_tool("scan_repository", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert_eq!(result.data["path"], "/test/path");
    }

    #[tokio::test]
    async fn test_search_functions_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "pattern": "test_*",
            "limit": 10
        });

        let result = tools.execute_tool("search_functions", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert_eq!(result.data["pattern"], "test_*");
    }

    #[tokio::test]
    async fn test_get_repository_overview_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "include_file_list": false
        });

        let result = tools.execute_tool("get_repository_overview", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert!(result.data.get("total_files").is_some());
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let tools = create_mock_tools();
        let input = json!({});

        let result = tools.execute_tool("unknown_tool", input).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Unknown tool"));
    }

    #[test]
    fn test_tool_result_creation() {
        let success_result = ToolResult::success(json!({"key": "value"}));
        assert!(success_result.success);
        assert_eq!(success_result.data["key"], "value");
        assert!(success_result.error.is_none());

        let error_result = ToolResult::error("Test error".to_string());
        assert!(!error_result.success);
        assert!(error_result.error.is_some());
        assert_eq!(error_result.error.unwrap(), "Test error");
    }

    #[tokio::test]
    async fn test_analyze_file_with_nonexistent_file() {
        let tools = create_mock_tools();
        let input = json!({
            "file_path": "/nonexistent/file.rs",
            "include_content": false
        });

        let result = tools.execute_tool("analyze_file", input).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.data["status"], "error");
        assert!(result.data["error"].as_str().unwrap().contains("Failed to read file"));
    }

    #[tokio::test]
    async fn test_find_callers_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "function_name": "test_function",
            "limit": 25
        });

        let result = tools.execute_tool("find_callers", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert_eq!(result.data["function_name"], "test_function");
    }
} 
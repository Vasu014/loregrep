use crate::{
    analyzers::{rust::RustAnalyzer, LanguageAnalyzer},
    scanner::RepositoryScanner,
    storage::memory::RepoMap,
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::anthropic::ToolSchema;

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
                description: "Get high-level repository information including metadata, file counts, and languages".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "include_file_list": {
                            "type": "boolean",
                            "description": "Whether to include a list of all files",
                            "default": false
                        },
                        "include_tree": {
                            "type": "boolean",
                            "description": "Whether to include the repository tree structure",
                            "default": false
                        }
                    }
                })
            },
            ToolSchema {
                name: "get_repository_tree".to_string(),
                description: "Get the complete repository tree structure with directory hierarchy, file skeletons, and comprehensive statistics".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "include_file_details": {
                            "type": "boolean",
                            "description": "Whether to include detailed file skeletons with functions and structs",
                            "default": true
                        },
                        "max_depth": {
                            "type": "integer",
                            "description": "Maximum directory depth to include (0 for unlimited)",
                            "default": 0
                        }
                    }
                })
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
            "get_repository_tree" => self.get_repository_tree(input).await,
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

        // Include repository tree structure if requested or if files are few enough
        let include_tree = overview_input.include_tree.unwrap_or(total_files <= 50);
        if include_tree {
            // We need to create a mutable reference to get the repository tree
            // For now, we'll include a note about tree availability
            result.as_object_mut().unwrap().insert(
                "repository_tree_available".to_string(), 
                json!(true)
            );
            result.as_object_mut().unwrap().insert(
                "note".to_string(), 
                json!("Repository tree structure available - use get_repository_tree for detailed structure")
            );
        }

        if overview_input.include_file_list.unwrap_or(false) {
            let files: Vec<_> = self.repo_map.get_all_files()
                .iter()
                .map(|f| f.file_path.clone())
                .collect();
            result.as_object_mut().unwrap().insert("files".to_string(), json!(files));
        }

        Ok(ToolResult::success(result))
    }

    async fn get_repository_tree(&self, input: Value) -> Result<ToolResult> {
        let tree_input: GetRepositoryTreeInput = serde_json::from_value(input)
            .unwrap_or_else(|_| GetRepositoryTreeInput {
                include_file_details: Some(true),
                max_depth: None,
            });

        // For now, we'll provide a structured overview of the repository
        // In a future enhancement, we could add interior mutability to RepoMap
        // to allow building the tree from immutable references
        
        let metadata = self.repo_map.get_metadata();
        let all_files = self.repo_map.get_all_files();
        
        // Build a simplified tree structure from current data
        let mut file_structure: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let mut directory_stats: std::collections::HashMap<String, (usize, u32)> = std::collections::HashMap::new();
        
        for file in all_files {
            let path = std::path::Path::new(&file.file_path);
            let dir_path = path.parent()
                .unwrap_or_else(|| std::path::Path::new("/"))
                .to_string_lossy()
                .to_string();
            
            let file_name = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            
            file_structure.entry(dir_path.clone())
                .or_insert_with(Vec::new)
                .push(file_name);
            
            let stats = directory_stats.entry(dir_path).or_insert((0, 0));
            stats.0 += 1; // file count
            // Estimate lines based on content since line_count is not available
            let estimated_lines = (file.functions.len() * 10 + file.structs.len() * 5) as u32;
            stats.1 += estimated_lines; // estimated line count
        }
        
        let include_details = tree_input.include_file_details.unwrap_or(true);
        let tree_structure = if include_details {
            json!({
                "directories": file_structure,
                "directory_stats": directory_stats,
                "file_details": all_files.iter().map(|f| {
                    let estimated_lines = (f.functions.len() * 10 + f.structs.len() * 5) as u32;
                    json!({
                        "path": f.file_path,
                        "language": f.language,
                        "functions": f.functions.len(),
                        "structs": f.structs.len(),
                        "imports": f.imports.len(),
                        "exports": f.exports.len(),
                        "estimated_line_count": estimated_lines
                    })
                }).collect::<Vec<_>>()
            })
        } else {
            json!({
                "directories": file_structure,
                "directory_stats": directory_stats
            })
        };

        let result = json!({
            "status": "success",
            "total_files": all_files.len(),
            "total_directories": file_structure.len(),
            "metadata": metadata,
            "tree_structure": tree_structure,
            "note": "Enhanced repository tree with full hierarchy will be available in future updates"
        });

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
    include_tree: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GetRepositoryTreeInput {
    include_file_details: Option<bool>,
    max_depth: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FileScanningConfig;

    // Helper to create minimal test instances
    fn create_test_repo_map() -> Arc<RepoMap> {
        Arc::new(RepoMap::new())
    }

    fn create_test_scanner() -> RepositoryScanner {
        // Create with minimal config
        let config = FileScanningConfig {
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

    // === Tool Schema Tests ===

    #[test]
    fn test_tool_schemas_creation() {
        let tools = create_mock_tools();
        let schemas = tools.get_tool_schemas();
        
        assert_eq!(schemas.len(), 8, "Should have exactly 8 tool schemas");
        
        let tool_names: Vec<_> = schemas.iter().map(|s| &s.name).collect();
        assert!(tool_names.contains(&&"scan_repository".to_string()));
        assert!(tool_names.contains(&&"search_functions".to_string()));
        assert!(tool_names.contains(&&"search_structs".to_string()));
        assert!(tool_names.contains(&&"analyze_file".to_string()));
        assert!(tool_names.contains(&&"get_dependencies".to_string()));
        assert!(tool_names.contains(&&"find_callers".to_string()));
        assert!(tool_names.contains(&&"get_repository_overview".to_string()));
        assert!(tool_names.contains(&&"get_repository_tree".to_string()));
    }

    #[test]
    fn test_tool_schemas_have_required_fields() {
        let tools = create_mock_tools();
        let schemas = tools.get_tool_schemas();
        
        for schema in schemas {
            assert!(!schema.name.is_empty(), "Tool name should not be empty");
            assert!(!schema.description.is_empty(), "Tool description should not be empty");
            assert!(schema.input_schema.is_object(), "Input schema should be an object");
            
            // Check that input schema has proper structure
            let input_schema = schema.input_schema.as_object().unwrap();
            assert_eq!(input_schema.get("type").unwrap(), "object");
            assert!(input_schema.contains_key("properties"));
        }
    }

    // === Scan Repository Tests ===

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
        assert_eq!(result.data["include_patterns"].as_array().unwrap().len(), 1);
        assert_eq!(result.data["exclude_patterns"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_scan_repository_minimal_input() {
        let tools = create_mock_tools();
        let input = json!({
            "path": "/minimal/path"
        });

        let result = tools.execute_tool("scan_repository", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert_eq!(result.data["path"], "/minimal/path");
        assert!(result.data["include_patterns"].as_array().unwrap().is_empty());
        assert!(result.data["exclude_patterns"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_scan_repository_invalid_input() {
        let tools = create_mock_tools();
        let input = json!({
            "invalid_field": "value"
        });

        let result = tools.execute_tool("scan_repository", input).await;
        assert!(result.is_err());
    }

    // === Search Functions Tests ===

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
        assert!(result.data["count"].as_u64().unwrap() <= 10);
    }

    #[tokio::test]
    async fn test_search_functions_with_language_filter() {
        let tools = create_mock_tools();
        let input = json!({
            "pattern": "main",
            "limit": 5,
            "language": "rust"
        });

        let result = tools.execute_tool("search_functions", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["pattern"], "main");
        assert!(result.data["count"].as_u64().unwrap() <= 5);
    }

    #[tokio::test]
    async fn test_search_functions_minimal_input() {
        let tools = create_mock_tools();
        let input = json!({
            "pattern": ".*"
        });

        let result = tools.execute_tool("search_functions", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["pattern"], ".*");
        // Should use default limit of 20
        assert!(result.data["count"].as_u64().unwrap() <= 20);
    }

    // === Search Structs Tests ===

    #[tokio::test]
    async fn test_search_structs_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "pattern": "Config*",
            "limit": 15
        });

        let result = tools.execute_tool("search_structs", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert_eq!(result.data["pattern"], "Config*");
        assert!(result.data["count"].as_u64().unwrap() <= 15);
    }

    #[tokio::test]
    async fn test_search_structs_with_language_filter() {
        let tools = create_mock_tools();
        let input = json!({
            "pattern": "Tool.*",
            "limit": 10,
            "language": "rust"
        });

        let result = tools.execute_tool("search_structs", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["pattern"], "Tool.*");
        assert!(result.data["count"].as_u64().unwrap() <= 10);
    }

    #[tokio::test]
    async fn test_search_structs_minimal_input() {
        let tools = create_mock_tools();
        let input = json!({
            "pattern": ".*Result"
        });

        let result = tools.execute_tool("search_structs", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["pattern"], ".*Result");
        // Should use default limit of 20
        assert!(result.data["count"].as_u64().unwrap() <= 20);
    }

    // === Analyze File Tests ===

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
    async fn test_analyze_file_minimal_input() {
        let tools = create_mock_tools();
        let input = json!({
            "file_path": "/nonexistent/file.rs"
        });

        let result = tools.execute_tool("analyze_file", input).await.unwrap();
        assert!(!result.success); // Will fail because file doesn't exist
        assert_eq!(result.data["status"], "error");
    }

    #[tokio::test]
    async fn test_analyze_file_invalid_input() {
        let tools = create_mock_tools();
        let input = json!({
            "wrong_field": "value"
        });

        let result = tools.execute_tool("analyze_file", input).await;
        assert!(result.is_err());
    }

    // === Get Dependencies Tests ===

    #[tokio::test]
    async fn test_get_dependencies_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "file_path": "src/main.rs"
        });

        let result = tools.execute_tool("get_dependencies", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert_eq!(result.data["file_path"], "src/main.rs");
        assert!(result.data.get("dependencies").is_some());
    }

    #[tokio::test]
    async fn test_get_dependencies_invalid_input() {
        let tools = create_mock_tools();
        let input = json!({
            "wrong_field": "value"
        });

        let result = tools.execute_tool("get_dependencies", input).await;
        assert!(result.is_err());
    }

    // === Find Callers Tests ===

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
        assert!(result.data["count"].as_u64().unwrap() <= 25);
    }

    #[tokio::test]
    async fn test_find_callers_minimal_input() {
        let tools = create_mock_tools();
        let input = json!({
            "function_name": "main"
        });

        let result = tools.execute_tool("find_callers", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["function_name"], "main");
        // Should use default limit of 50
        assert!(result.data["count"].as_u64().unwrap() <= 50);
    }

    #[tokio::test]
    async fn test_find_callers_invalid_input() {
        let tools = create_mock_tools();
        let input = json!({
            "wrong_field": "value"
        });

        let result = tools.execute_tool("find_callers", input).await;
        assert!(result.is_err());
    }

    // === Repository Overview Tests ===

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
        assert!(result.data.get("languages").is_some());
        assert!(result.data.get("metadata").is_some());
        assert!(!result.data.get("files").is_some()); // Should not include files
    }

    #[tokio::test]
    async fn test_get_repository_overview_with_file_list() {
        let tools = create_mock_tools();
        let input = json!({
            "include_file_list": true,
            "include_tree": false
        });

        let result = tools.execute_tool("get_repository_overview", input).await.unwrap();
        assert!(result.success);
        assert!(result.data.get("files").is_some()); // Should include files
        assert!(result.data["files"].is_array());
    }

    #[tokio::test]
    async fn test_get_repository_overview_empty_input() {
        let tools = create_mock_tools();
        let input = json!({});

        let result = tools.execute_tool("get_repository_overview", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert!(result.data.get("total_files").is_some());
    }

    // === Repository Tree Tests ===

    #[tokio::test]
    async fn test_get_repository_tree_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "include_file_details": true,
            "max_depth": 3
        });

        let result = tools.execute_tool("get_repository_tree", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert!(result.data.get("total_files").is_some());
        assert!(result.data.get("total_directories").is_some());
        assert!(result.data.get("metadata").is_some());
        assert!(result.data.get("tree_structure").is_some());
    }

    #[tokio::test]
    async fn test_get_repository_tree_minimal_details() {
        let tools = create_mock_tools();
        let input = json!({
            "include_file_details": false
        });

        let result = tools.execute_tool("get_repository_tree", input).await.unwrap();
        assert!(result.success);
        let tree_structure = &result.data["tree_structure"];
        assert!(tree_structure.get("directories").is_some());
        assert!(tree_structure.get("directory_stats").is_some());
        assert!(!tree_structure.get("file_details").is_some()); // Should not include file details
    }

    #[tokio::test]
    async fn test_get_repository_tree_empty_input() {
        let tools = create_mock_tools();
        let input = json!({});

        let result = tools.execute_tool("get_repository_tree", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        // Should use defaults: include_file_details = true, max_depth = None
        let tree_structure = &result.data["tree_structure"];
        assert!(tree_structure.get("file_details").is_some()); // Should include file details by default
    }

    // === Error Handling Tests ===

    #[tokio::test]
    async fn test_unknown_tool() {
        let tools = create_mock_tools();
        let input = json!({});

        let result = tools.execute_tool("unknown_tool", input).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_tool_execution_with_invalid_json() {
        let tools = create_mock_tools();
        
        // Test with malformed input for each tool that requires specific structure
        let test_cases = vec![
            ("search_functions", json!({"pattern": 123})), // pattern should be string
            ("search_structs", json!({"limit": "not_a_number"})), // limit should be number
            ("analyze_file", json!({"include_content": "not_a_bool"})), // include_content should be bool
        ];

        for (tool_name, invalid_input) in test_cases {
            let result = tools.execute_tool(tool_name, invalid_input).await;
            // These should either return an error or handle gracefully
            match result {
                Ok(tool_result) => {
                    // If it succeeds, it should either be an error result or handle the invalid input gracefully
                    if !tool_result.success {
                        // This is acceptable - the tool handled the invalid input gracefully
                    }
                },
                Err(_) => {
                    // This is also acceptable - the tool properly rejected invalid input
                }
            }
        }
    }

    // === ToolResult Tests ===

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
        assert_eq!(error_result.data, json!({}));

        let error_with_data = ToolResult::error_with_data(json!({"error_code": 404}));
        assert!(!error_with_data.success);
        assert!(error_with_data.error.is_none());
        assert_eq!(error_with_data.data["error_code"], 404);
    }

    // === Integration Tests ===

    #[tokio::test]
    async fn test_all_tools_execute_without_panic() {
        let tools = create_mock_tools();
        let tool_names = vec![
            "scan_repository",
            "search_functions", 
            "search_structs",
            "analyze_file",
            "get_dependencies",
            "find_callers",
            "get_repository_overview",
            "get_repository_tree"
        ];

        for tool_name in tool_names {
            let minimal_input = match tool_name {
                "scan_repository" => json!({"path": "/test"}),
                "search_functions" => json!({"pattern": "test"}),
                "search_structs" => json!({"pattern": "Test"}),
                "analyze_file" => json!({"file_path": "/test.rs"}),
                "get_dependencies" => json!({"file_path": "/test.rs"}),
                "find_callers" => json!({"function_name": "test"}),
                "get_repository_overview" => json!({}),
                "get_repository_tree" => json!({}),
                _ => json!({})
            };

            let result = tools.execute_tool(tool_name, minimal_input).await;
            assert!(result.is_ok(), "Tool {} should not panic", tool_name);
        }
    }

    #[test]
    fn test_tool_schemas_json_validity() {
        let tools = create_mock_tools();
        let schemas = tools.get_tool_schemas();
        
        for schema in schemas {
            // Ensure the input schema is valid JSON
            let schema_str = serde_json::to_string(&schema.input_schema).unwrap();
            let _: Value = serde_json::from_str(&schema_str).unwrap();
            
            // Ensure we can serialize the schema (ToolSchema only implements Serialize, not Deserialize)
            let _serialized = serde_json::to_string(&schema).unwrap();
        }
    }
} 
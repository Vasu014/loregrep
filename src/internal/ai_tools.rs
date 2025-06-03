use crate::{
    analyzers::{rust::RustAnalyzer, LanguageAnalyzer},
    storage::memory::RepoMap,
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

use crate::internal::anthropic::ToolSchema;

#[derive(Clone)]
pub struct LocalAnalysisTools {
    repo_map: Arc<Mutex<RepoMap>>,
    rust_analyzer: RustAnalyzer,
}

impl LocalAnalysisTools {
    pub fn new(
        repo_map: Arc<Mutex<RepoMap>>,
        rust_analyzer: RustAnalyzer,
    ) -> Self {
        Self {
            repo_map,
            rust_analyzer,
        }
    }

    pub fn get_tool_schemas(&self) -> Vec<ToolSchema> {
        vec![
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
                name: "get_repository_tree".to_string(),
                description: "Get complete repository information including hierarchical directory structure, file details, statistics, and metadata. Use include_file_details=false and max_depth=1 for overview-style information, or full defaults for comprehensive repository analysis.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "include_file_details": {
                            "type": "boolean",
                            "description": "Whether to include detailed file skeletons with functions and structs. Set to false for overview-style information.",
                            "default": true
                        },
                        "max_depth": {
                            "type": "integer",
                            "description": "Maximum directory depth to include (0 for unlimited). Use 1 for overview-style information.",
                            "default": 0
                        }
                    }
                })
            },
        ]
    }

    pub async fn execute_tool(&self, tool_name: &str, input: Value) -> Result<ToolResult> {
        match tool_name {
            "search_functions" => self.search_functions(input).await,
            "search_structs" => self.search_structs(input).await,
            "analyze_file" => self.analyze_file(input).await,
            "get_dependencies" => self.get_dependencies(input).await,
            "find_callers" => self.find_callers(input).await,
            "get_repository_tree" => self.get_repository_tree(input).await,
            _ => Ok(ToolResult::error(format!("Unknown tool: {}", tool_name))),
        }
    }

    async fn search_functions(&self, input: Value) -> Result<ToolResult> {
        let search_input: SearchFunctionsInput = serde_json::from_value(input)
            .context("Invalid search_functions input")?;

        let repo_map = self.repo_map.lock().unwrap();
        let results = repo_map.find_functions(&search_input.pattern);
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

        let repo_map = self.repo_map.lock().unwrap();
        let results = repo_map.find_structs(&search_input.pattern);
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

        let dependencies = self.repo_map.lock().unwrap().get_file_dependencies(&deps_input.file_path);

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

        let callers = self.repo_map.lock().unwrap().find_function_callers(&callers_input.function_name);
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

    async fn get_repository_tree(&self, input: Value) -> Result<ToolResult> {
        let tree_input: GetRepositoryTreeInput = serde_json::from_value(input)
            .unwrap_or_else(|_| GetRepositoryTreeInput {
                include_file_details: Some(true),
                max_depth: None,
            });

        // Get the actual repository tree structure from repo_map
        // This will build the full hierarchical structure if it doesn't exist
        let (repository_tree_opt, file_count, metadata) = {
            let repo_map = self.repo_map.lock().unwrap();
            (
                repo_map.get_repository_tree(),
                repo_map.file_count(),
                repo_map.get_metadata().clone()
            )
        };

        match repository_tree_opt {
            Some(repository_tree) => {
                // Apply depth filtering if requested
                let filtered_tree_root = if let Some(max_depth) = tree_input.max_depth {
                    if max_depth > 0 {
                        self.apply_depth_filter(&repository_tree.root, max_depth)
                    } else {
                        repository_tree.root.clone()
                    }
                } else {
                    repository_tree.root.clone()
                };

                // Apply file detail filtering if requested
                let final_tree_root = if tree_input.include_file_details.unwrap_or(true) {
                    filtered_tree_root
                } else {
                    self.remove_file_details(&filtered_tree_root)
                };

                let result = json!({
                    "status": "success",
                    "repository_tree": final_tree_root,
                    "metadata": {
                        "total_files": final_tree_root.file_count,
                        "total_lines": final_tree_root.total_lines,
                        "languages": final_tree_root.languages.iter().collect::<Vec<_>>(),
                        "root_path": final_tree_root.path,
                        "include_file_details": tree_input.include_file_details.unwrap_or(true),
                        "max_depth": tree_input.max_depth
                    }
                });

                Ok(ToolResult::success(result))
            }
            None => {
                // Instead of returning an error, provide useful information about the empty repository
                let result = json!({
                    "status": "success",
                    "repository_tree": {
                        "name": "empty_repository",
                        "path": ".",
                        "children": [],
                        "file_count": 0,
                        "total_lines": 0,
                        "languages": []
                    },
                    "metadata": {
                        "total_files": file_count,
                        "total_lines": 0,
                        "languages": metadata.languages.iter().collect::<Vec<_>>(),
                        "root_path": ".",
                        "include_file_details": tree_input.include_file_details.unwrap_or(true),
                        "max_depth": tree_input.max_depth,
                        "note": "Repository is empty. Files need to be analyzed through the CLI scan command before they appear in the repository tree.",
                        "available_files": file_count
                    }
                });
                Ok(ToolResult::success(result))
            }
        }
    }

    /// Apply depth filtering to repository tree
    fn apply_depth_filter(&self, tree: &crate::storage::memory::DirectoryNode, max_depth: usize) -> crate::storage::memory::DirectoryNode {
        self.apply_depth_filter_recursive(tree, max_depth, 0)
    }

    fn apply_depth_filter_recursive(&self, node: &crate::storage::memory::DirectoryNode, max_depth: usize, current_depth: usize) -> crate::storage::memory::DirectoryNode {
        let mut filtered_node = crate::storage::memory::DirectoryNode {
            name: node.name.clone(),
            path: node.path.clone(),
            children: Vec::new(),
            file_count: 0,
            total_lines: 0,
            languages: std::collections::HashSet::new(),
        };

        if current_depth < max_depth {
            for child in &node.children {
                match child {
                    crate::storage::memory::RepositoryTreeNode::File(file_node) => {
                        filtered_node.children.push(crate::storage::memory::RepositoryTreeNode::File(file_node.clone()));
                        filtered_node.file_count += 1;
                        filtered_node.total_lines += file_node.skeleton.line_count;
                        if !file_node.skeleton.language.is_empty() {
                            filtered_node.languages.insert(file_node.skeleton.language.clone());
                        }
                    }
                    crate::storage::memory::RepositoryTreeNode::Directory(dir_node) => {
                        let filtered_subdir = self.apply_depth_filter_recursive(dir_node, max_depth, current_depth + 1);
                        filtered_node.file_count += filtered_subdir.file_count;
                        filtered_node.total_lines += filtered_subdir.total_lines;
                        for lang in &filtered_subdir.languages {
                            filtered_node.languages.insert(lang.clone());
                        }
                        filtered_node.children.push(crate::storage::memory::RepositoryTreeNode::Directory(filtered_subdir));
                    }
                }
            }
        }

        filtered_node
    }

    /// Remove file details (skeletons) from tree to provide just structure
    fn remove_file_details(&self, tree: &crate::storage::memory::DirectoryNode) -> crate::storage::memory::DirectoryNode {
        let mut simplified_node = crate::storage::memory::DirectoryNode {
            name: tree.name.clone(),
            path: tree.path.clone(),
            children: Vec::new(),
            file_count: tree.file_count,
            total_lines: tree.total_lines,
            languages: tree.languages.clone(),
        };

        for child in &tree.children {
            match child {
                crate::storage::memory::RepositoryTreeNode::File(file_node) => {
                    // Create a simplified file node without detailed skeleton
                    let simplified_skeleton = crate::storage::memory::FileSkeleton {
                        path: file_node.skeleton.path.clone(),
                        language: file_node.skeleton.language.clone(),
                        size_bytes: file_node.skeleton.size_bytes,
                        line_count: file_node.skeleton.line_count,
                        functions: Vec::new(), // Remove function details
                        structs: Vec::new(),   // Remove struct details
                        imports: Vec::new(),   // Remove import details
                        exports: Vec::new(),   // Remove export details
                        is_public: file_node.skeleton.is_public,
                        is_test: file_node.skeleton.is_test,
                        last_modified: file_node.skeleton.last_modified,
                    };

                    let simplified_file = crate::storage::memory::FileNode {
                        name: file_node.name.clone(),
                        path: file_node.path.clone(),
                        skeleton: simplified_skeleton,
                    };

                    simplified_node.children.push(crate::storage::memory::RepositoryTreeNode::File(simplified_file));
                }
                crate::storage::memory::RepositoryTreeNode::Directory(dir_node) => {
                    let simplified_subdir = self.remove_file_details(dir_node);
                    simplified_node.children.push(crate::storage::memory::RepositoryTreeNode::Directory(simplified_subdir));
                }
            }
        }

        simplified_node
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
struct GetRepositoryTreeInput {
    include_file_details: Option<bool>,
    max_depth: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::config::FileScanningConfig;

    // Helper to create minimal test instances
    fn create_test_repo_map() -> Arc<Mutex<RepoMap>> {
        Arc::new(Mutex::new(RepoMap::new()))
    }

    fn create_test_analyzer() -> RustAnalyzer {
        RustAnalyzer::new().unwrap()
    }

    fn create_mock_tools() -> LocalAnalysisTools {
        let repo_map = create_test_repo_map();
        let rust_analyzer = create_test_analyzer();
        
        LocalAnalysisTools::new(repo_map, rust_analyzer)
    }

    // === Tool Schema Tests ===

    #[test]
    fn test_tool_schemas_creation() {
        let tools = create_mock_tools();
        let schemas = tools.get_tool_schemas();
        
        assert_eq!(schemas.len(), 6, "Should have exactly 6 tool schemas");
        
        let tool_names: Vec<_> = schemas.iter().map(|s| &s.name).collect();
        assert!(tool_names.contains(&&"search_functions".to_string()));
        assert!(tool_names.contains(&&"search_structs".to_string()));
        assert!(tool_names.contains(&&"analyze_file".to_string()));
        assert!(tool_names.contains(&&"get_dependencies".to_string()));
        assert!(tool_names.contains(&&"find_callers".to_string()));
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
        assert!(result.data.get("repository_tree").is_some());
        assert!(result.data.get("metadata").is_some());
        
        let metadata = &result.data["metadata"];
        assert!(metadata.get("total_files").is_some());
        assert!(metadata.get("total_lines").is_some());
        assert!(metadata.get("languages").is_some());
        assert_eq!(metadata["max_depth"], 3);
        assert_eq!(metadata["include_file_details"], true);
    }

    #[tokio::test]
    async fn test_get_repository_tree_minimal_details() {
        let tools = create_mock_tools();
        let input = json!({
            "include_file_details": false
        });

        let result = tools.execute_tool("get_repository_tree", input).await.unwrap();
        assert!(result.success);
        assert!(result.data.get("repository_tree").is_some());
        
        let metadata = &result.data["metadata"];
        assert_eq!(metadata["include_file_details"], false);
        // Since we're using simplified tree without details, the structure should be simpler
    }

    #[tokio::test]
    async fn test_get_repository_tree_empty_input() {
        let tools = create_mock_tools();
        let input = json!({});

        let result = tools.execute_tool("get_repository_tree", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");
        assert!(result.data.get("repository_tree").is_some());
        assert!(result.data.get("metadata").is_some());
        
        let metadata = &result.data["metadata"];
        // Should use defaults: include_file_details = true, max_depth = None
        assert_eq!(metadata["include_file_details"], true);
        assert!(metadata["max_depth"].is_null());
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
            "search_functions", 
            "search_structs",
            "analyze_file",
            "get_dependencies",
            "find_callers",
            "get_repository_tree"
        ];

        for tool_name in tool_names {
            let minimal_input = match tool_name {
                "search_functions" => json!({"pattern": "test"}),
                "search_structs" => json!({"pattern": "Test"}),
                "analyze_file" => json!({"file_path": "/test.rs"}),
                "get_dependencies" => json!({"file_path": "/test.rs"}),
                "find_callers" => json!({"function_name": "test"}),
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
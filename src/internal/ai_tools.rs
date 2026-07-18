use crate::{analyzers::registry::RegistryHandle, storage::memory::RepoMap, types::TypeKind};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use crate::core::types::ToolSchema;

#[derive(Clone)]
pub struct LocalAnalysisTools {
    repo_map: Arc<Mutex<RepoMap>>,
    /// Handle into the language-analyzer registry. Analysis is dispatched to the
    /// analyzer that matches each file's language, rather than a single
    /// hard-coded analyzer.
    registry: RegistryHandle,
}

impl LocalAnalysisTools {
    pub fn new(repo_map: Arc<Mutex<RepoMap>>, registry: RegistryHandle) -> Self {
        Self { repo_map, registry }
    }

    pub fn get_tool_schemas(&self) -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "search_functions".to_string(),
                description: "Look up functions by name or regex and return their structured signature: parameters, return type, visibility, async/const/static flags, the owning type (impl/class) if it is a method, and the definition's file path and line range.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Name or regex pattern to match function names"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return",
                            "default": 20
                        },
                        "language": {
                            "type": "string",
                            "description": "Filter by programming language (optional)"
                        },
                        "owner": {
                            "type": "string",
                            "description": "Filter to methods whose owning type (impl block / class) has this exact name (optional)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolSchema {
                name: "search_structs".to_string(),
                description: "Look up named types — structs, enums, traits, classes, abstract classes, interfaces, and type aliases — by name or regex and return their fields, generics, visibility, declared supertypes (supertraits / base classes / extends+implements), the `kind` of each, and definition location.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Name or regex pattern to match type names"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return",
                            "default": 20
                        },
                        "language": {
                            "type": "string",
                            "description": "Filter by programming language (optional)"
                        },
                        "kind": {
                            "type": "string",
                            "enum": ["struct", "enum", "trait", "class", "abstract_class", "interface", "type_alias"],
                            "description": "Filter to a single kind of type (optional)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolSchema {
                name: "analyze_file".to_string(),
                description: "Get a file's structural skeleton — its functions, structs, imports, exports, and calls with signatures and line numbers — without reading the whole file into context.".to_string(),
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
                description: "Get a file's import/export edges from the dependency graph — what it depends on and what it exposes — without reading the file.".to_string(),
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
                description: "Return the direct call sites of a function across the repository, each with file path and line, resolved from the parsed call graph — call expressions only (not comments, string literals, imports, or the definition). For the full transitive upstream chain, see trace_callers.".to_string(),
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
                description: "Get a structural map of the repository — directories, files, and per-file symbol skeletons — for fast orientation without reading files. Use include_file_details=false and max_depth=1 for an overview, or full defaults for a comprehensive map.".to_string(),
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
            ToolSchema {
                name: "trace_callers".to_string(),
                description: "Return the transitive (upstream) callers of a function — every function that reaches the target through the call graph, across files, each with its depth. Where find_callers returns one hop, this walks the graph to the full upstream chain. Depth is bounded by max_depth (0 = unlimited). Each caller carries a `resolution`: \"exact\" (single-definition, confirmed) or \"name_ambiguous\" (reached via a name with more than one definition, so it is a candidate — the specific target is unresolved). `exact_count`/`ambiguous_count` split the total, and `ambiguous_names` lists the colliding names with their candidate definition files.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "function_name": {
                            "type": "string",
                            "description": "Name of the function whose transitive callers to trace"
                        },
                        "max_depth": {
                            "type": "integer",
                            "description": "Maximum number of hops to walk up the call graph (0 = unlimited)",
                            "default": 0
                        }
                    },
                    "required": ["function_name"]
                }),
            },
            ToolSchema {
                name: "analyze_impact".to_string(),
                description: "Return the change blast radius of a function: every function and file that transitively depends on it, resolved through the call graph. `transitive_functions` and `affected_files` count only exact (confirmed) callers; `ambiguous_functions` and `candidate_files` hold candidates reached via multiply-defined names, and `ambiguous_names` explains each collision — so the headline impact is never inflated by unconfirmed links.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "function_name": {
                            "type": "string",
                            "description": "Name of the function whose change impact to compute"
                        }
                    },
                    "required": ["function_name"]
                }),
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
            "trace_callers" => self.trace_callers(input).await,
            "analyze_impact" => self.analyze_impact(input).await,
            "get_repository_tree" => self.get_repository_tree(input).await,
            _ => Ok(ToolResult::error(format!("Unknown tool: {}", tool_name))),
        }
    }

    async fn search_functions(&self, input: Value) -> Result<ToolResult> {
        let search_input: SearchFunctionsInput =
            serde_json::from_value(input).context("Invalid search_functions input")?;

        let repo_map = self.repo_map.lock().unwrap();
        let results = repo_map.find_functions(&search_input.pattern);
        // Apply the optional language filter: keep only functions defined in a
        // file whose analyzer language matches (case-insensitive). Each result's
        // owning file is looked up in the repo map to recover its language, since
        // FunctionSignature does not carry the language directly. A filter for a
        // language absent from the repo therefore yields an empty set.
        let language_filter = search_input.language.as_deref();
        let owner_filter = search_input.owner.as_deref();
        let limited_results: Vec<_> = results
            .items
            .into_iter()
            .filter(|func| match language_filter {
                Some(lang) => repo_map
                    .get_file(&func.file_path)
                    .map(|node| node.language.eq_ignore_ascii_case(lang))
                    .unwrap_or(false),
                None => true,
            })
            // Optional owner filter: keep only methods of the named type.
            .filter(|func| match owner_filter {
                Some(owner) => func.owner.as_deref() == Some(owner),
                None => true,
            })
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
        let search_input: SearchStructsInput =
            serde_json::from_value(input).context("Invalid search_structs input")?;

        let repo_map = self.repo_map.lock().unwrap();
        let results = repo_map.find_structs(&search_input.pattern);
        // Apply the optional language filter (see search_functions above).
        let language_filter = search_input.language.as_deref();
        // Parse the optional kind filter once. An unrecognized kind string yields
        // Some(None) -> matches nothing (honest: the requested kind does not exist),
        // versus None -> no kind filter at all.
        let kind_filter: Option<Option<TypeKind>> =
            search_input.kind.as_deref().map(parse_type_kind);
        let limited_results: Vec<_> = results
            .items
            .into_iter()
            .filter(|struct_def| match language_filter {
                Some(lang) => repo_map
                    .get_file(&struct_def.file_path)
                    .map(|node| node.language.eq_ignore_ascii_case(lang))
                    .unwrap_or(false),
                None => true,
            })
            // Optional kind filter: keep only types whose kind matches.
            .filter(|struct_def| match &kind_filter {
                Some(parsed) => *parsed == Some(struct_def.kind),
                None => true,
            })
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
        let analyze_input: AnalyzeFileInput =
            serde_json::from_value(input).context("Invalid analyze_file input")?;

        // Resolve the analyzer for this file's language via the registry. If no
        // analyzer is registered for the file's extension, report a clear error
        // instead of silently analyzing it with the wrong language.
        let analyzer = match self
            .registry
            .get_analyzer_for_path(&analyze_input.file_path)
        {
            Some(analyzer) => analyzer,
            None => {
                return Ok(ToolResult::error(format!(
                    "unsupported language for {}",
                    analyze_input.file_path
                )));
            }
        };

        // Try to read the file and analyze it
        match tokio::fs::read_to_string(&analyze_input.file_path).await {
            Ok(content) => {
                let file_analysis = analyzer
                    .analyze_file(&content, &analyze_input.file_path)
                    .await?;

                let tree_node = &file_analysis.tree_node;
                let mut result = json!({
                    "status": "success",
                    "file_path": analyze_input.file_path,
                    // Expose the analysis fields at the top level so consumers
                    // (CLI display, public API callers) can read them directly.
                    "language": tree_node.language,
                    "functions": tree_node.functions,
                    "structs": tree_node.structs,
                    "imports": tree_node.imports,
                    "exports": tree_node.exports,
                    "function_calls": tree_node.function_calls
                });

                if analyze_input.include_content.unwrap_or(false) {
                    result
                        .as_object_mut()
                        .unwrap()
                        .insert("content".to_string(), json!(content));
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
        let deps_input: GetDependenciesInput =
            serde_json::from_value(input).context("Invalid get_dependencies input")?;

        let dependencies = self
            .repo_map
            .lock()
            .unwrap()
            .get_file_dependencies(&deps_input.file_path);

        let result = json!({
            "status": "success",
            "file_path": deps_input.file_path,
            "dependencies": dependencies
        });

        Ok(ToolResult::success(result))
    }

    async fn find_callers(&self, input: Value) -> Result<ToolResult> {
        let callers_input: FindCallersInput =
            serde_json::from_value(input).context("Invalid find_callers input")?;

        let callers_json: Vec<Value> = {
            let repo_map = self.repo_map.lock().unwrap();
            repo_map
                .find_function_callers(&callers_input.function_name)
                .into_iter()
                .take(callers_input.limit.unwrap_or(50))
                .map(|cs| {
                    // Preserve every CallSite field, then add an owner-qualified
                    // display for the enclosing caller (e.g. "Loader::load").
                    let mut v = serde_json::to_value(&cs).unwrap_or_else(|_| json!({}));
                    let caller_display = cs
                        .caller_function
                        .as_deref()
                        .map(|name| repo_map.qualified_function_name(&cs.file_path, name));
                    v["caller_display"] = json!(caller_display);
                    v
                })
                .collect()
        };

        let result = json!({
            "status": "success",
            "function_name": callers_input.function_name,
            "callers": callers_json,
            "count": callers_json.len()
        });

        Ok(ToolResult::success(result))
    }

    async fn trace_callers(&self, input: Value) -> Result<ToolResult> {
        let trace_input: TraceCallersInput =
            serde_json::from_value(input).context("Invalid trace_callers input")?;

        let max_depth = trace_input.max_depth.unwrap_or(0);
        let (callers_json, callers_len, ambiguous_names) = {
            let repo_map = self.repo_map.lock().unwrap();
            let callers = repo_map.transitive_callers(&trace_input.function_name, max_depth);
            let ambiguous_names =
                Self::ambiguous_names_summary(&repo_map, &trace_input.function_name, &callers);
            let callers_json: Vec<Value> = callers
                .iter()
                .map(|c| {
                    json!({
                        "function_name": c.function_name,
                        // Owner-qualified display (e.g. "Loader::load") when the
                        // caller is a method; bare name otherwise.
                        "display_name": repo_map.qualified_function_name(&c.file_path, &c.function_name),
                        "file_path": c.file_path,
                        "depth": c.depth,
                        // "name_ambiguous": reached by expanding a name with >1
                        // definition, so which definition it calls is unresolved — a
                        // candidate, not a confirmed caller. "exact": single-definition.
                        "resolution": if c.ambiguous { "name_ambiguous" } else { "exact" }
                    })
                })
                .collect();
            (callers_json, callers.len(), ambiguous_names)
        };

        let max_depth_reached = callers_json
            .iter()
            .filter_map(|c| c["depth"].as_u64())
            .max()
            .unwrap_or(0);
        let ambiguous_count = callers_json
            .iter()
            .filter(|c| c["resolution"] == "name_ambiguous")
            .count();

        let result = json!({
            "status": "success",
            "function_name": trace_input.function_name,
            "callers": callers_json,
            "count": callers_len,
            "exact_count": callers_len - ambiguous_count,
            "ambiguous_count": ambiguous_count,
            "max_depth_reached": max_depth_reached,
            "ambiguous_names": ambiguous_names
        });

        Ok(ToolResult::success(result))
    }

    /// Build the `ambiguous_names` summary: for every name in the traced chain
    /// (the target plus each caller) that has more than one definition, report the
    /// name, its definition count, and up to `AMBIGUOUS_CANDIDATE_CAP` candidate
    /// files. These are the collisions that make some callers candidates rather
    /// than confirmed links. Empty when the chain has no name collisions.
    fn ambiguous_names_summary(
        repo_map: &RepoMap,
        target: &str,
        callers: &[crate::storage::memory::TransitiveCaller],
    ) -> Vec<Value> {
        const AMBIGUOUS_CANDIDATE_CAP: usize = 5;
        let mut names: Vec<&str> = Vec::with_capacity(callers.len() + 1);
        names.push(target);
        names.extend(callers.iter().map(|c| c.function_name.as_str()));
        names.sort_unstable();
        names.dedup();

        names
            .into_iter()
            .filter_map(|name| {
                let files = repo_map.function_definition_files(name);
                let total = files.len();
                if total <= 1 {
                    return None;
                }
                let candidates: Vec<String> = files.into_iter().take(AMBIGUOUS_CANDIDATE_CAP).collect();
                Some(json!({
                    "name": name,
                    "definition_count": total,
                    "candidate_files": candidates,
                    "candidates_truncated": total > AMBIGUOUS_CANDIDATE_CAP
                }))
            })
            .collect()
    }

    async fn analyze_impact(&self, input: Value) -> Result<ToolResult> {
        let impact_input: AnalyzeImpactInput =
            serde_json::from_value(input).context("Invalid analyze_impact input")?;

        let (direct_callers, transitive, ambiguous_names) = {
            let repo_map = self.repo_map.lock().unwrap();
            // Direct callers = distinct enclosing functions one hop up.
            let direct: std::collections::HashSet<String> = repo_map
                .find_function_callers(&impact_input.function_name)
                .into_iter()
                .filter_map(|c| c.caller_function)
                .collect();
            let transitive = repo_map.transitive_callers(&impact_input.function_name, 0);
            let ambiguous_names =
                Self::ambiguous_names_summary(&repo_map, &impact_input.function_name, &transitive);
            (direct.len(), transitive, ambiguous_names)
        };

        // Split the transitive set by resolution: exact callers are confirmed to
        // reach the target; ambiguous ones are candidates (reached via a
        // multiply-defined name). Keep them separate so the headline count is not
        // inflated by unconfirmed links.
        let ambiguous_functions = transitive.iter().filter(|c| c.ambiguous).count();
        let transitive_functions = transitive.len() - ambiguous_functions;

        // Affected files, split the same way (a file may hold both kinds; it is
        // "confirmed" affected if any exact caller lives there).
        let mut confirmed_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut candidate_only: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for c in &transitive {
            if c.ambiguous {
                candidate_only.insert(c.file_path.clone());
            } else {
                confirmed_files.insert(c.file_path.clone());
            }
        }
        // A file with an exact caller is confirmed, not candidate-only.
        candidate_only.retain(|f| !confirmed_files.contains(f));
        let affected_files: Vec<String> = confirmed_files.iter().cloned().collect();
        let candidate_files: Vec<String> = candidate_only.iter().cloned().collect();
        let affected_file_count = affected_files.len();

        let mut summary = format!(
            "Changing `{}` transitively affects {} function{} across {} file{}",
            impact_input.function_name,
            transitive_functions,
            if transitive_functions == 1 { "" } else { "s" },
            affected_file_count,
            if affected_file_count == 1 { "" } else { "s" }
        );
        if ambiguous_functions > 0 {
            summary.push_str(&format!(
                "; {} additional candidate caller{} via name collisions (see ambiguous_names)",
                ambiguous_functions,
                if ambiguous_functions == 1 { "" } else { "s" }
            ));
        }

        let result = json!({
            "status": "success",
            "function_name": impact_input.function_name,
            "direct_callers": direct_callers,
            "transitive_functions": transitive_functions,
            "ambiguous_functions": ambiguous_functions,
            "affected_files": affected_files,
            "affected_file_count": affected_file_count,
            "candidate_files": candidate_files,
            "ambiguous_names": ambiguous_names,
            "summary": summary
        });

        Ok(ToolResult::success(result))
    }

    async fn get_repository_tree(&self, input: Value) -> Result<ToolResult> {
        let tree_input: GetRepositoryTreeInput =
            serde_json::from_value(input).unwrap_or_else(|_| GetRepositoryTreeInput {
                include_file_details: Some(true),
                max_depth: None,
            });

        // Get the actual repository tree structure from repo_map
        // This will build the full hierarchical structure if it doesn't exist
        let (repository_tree_opt, file_count, metadata) = {
            let mut repo_map = self.repo_map.lock().unwrap();

            // Ensure repository tree is built if needed
            if let Err(e) = repo_map.build_repository_tree_if_needed() {
                eprintln!("Warning: Failed to build repository tree: {}", e);
            }

            (
                repo_map.get_repository_tree(),
                repo_map.file_count(),
                repo_map.get_metadata().clone(),
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
    fn apply_depth_filter(
        &self,
        tree: &crate::storage::memory::DirectoryNode,
        max_depth: usize,
    ) -> crate::storage::memory::DirectoryNode {
        self.apply_depth_filter_recursive(tree, max_depth, 0)
    }

    fn apply_depth_filter_recursive(
        &self,
        node: &crate::storage::memory::DirectoryNode,
        max_depth: usize,
        current_depth: usize,
    ) -> crate::storage::memory::DirectoryNode {
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
                        filtered_node.children.push(
                            crate::storage::memory::RepositoryTreeNode::File(file_node.clone()),
                        );
                        filtered_node.file_count += 1;
                        filtered_node.total_lines += file_node.skeleton.line_count;
                        if !file_node.skeleton.language.is_empty() {
                            filtered_node
                                .languages
                                .insert(file_node.skeleton.language.clone());
                        }
                    }
                    crate::storage::memory::RepositoryTreeNode::Directory(dir_node) => {
                        let filtered_subdir = self.apply_depth_filter_recursive(
                            dir_node,
                            max_depth,
                            current_depth + 1,
                        );
                        filtered_node.file_count += filtered_subdir.file_count;
                        filtered_node.total_lines += filtered_subdir.total_lines;
                        for lang in &filtered_subdir.languages {
                            filtered_node.languages.insert(lang.clone());
                        }
                        filtered_node.children.push(
                            crate::storage::memory::RepositoryTreeNode::Directory(filtered_subdir),
                        );
                    }
                }
            }
        }

        filtered_node
    }

    /// Remove file details (skeletons) from tree to provide just structure
    fn remove_file_details(
        &self,
        tree: &crate::storage::memory::DirectoryNode,
    ) -> crate::storage::memory::DirectoryNode {
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

                    simplified_node.children.push(
                        crate::storage::memory::RepositoryTreeNode::File(simplified_file),
                    );
                }
                crate::storage::memory::RepositoryTreeNode::Directory(dir_node) => {
                    let simplified_subdir = self.remove_file_details(dir_node);
                    simplified_node.children.push(
                        crate::storage::memory::RepositoryTreeNode::Directory(simplified_subdir),
                    );
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

/// Parse a `kind` filter string (case-insensitive, `_`/space/`-` insensitive) into
/// a `TypeKind`. Returns `None` for an unrecognized string so callers can treat it
/// as "matches nothing" rather than silently ignoring the filter.
fn parse_type_kind(s: &str) -> Option<TypeKind> {
    let normalized: String = s
        .chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(|c| c.to_lowercase())
        .collect();
    match normalized.as_str() {
        "struct" => Some(TypeKind::Struct),
        "enum" => Some(TypeKind::Enum),
        "trait" => Some(TypeKind::Trait),
        "class" => Some(TypeKind::Class),
        "abstractclass" => Some(TypeKind::AbstractClass),
        "interface" => Some(TypeKind::Interface),
        "typealias" => Some(TypeKind::TypeAlias),
        _ => None,
    }
}

// Input types for tool functions
#[derive(Debug, Deserialize)]
struct SearchFunctionsInput {
    pattern: String,
    limit: Option<usize>,
    language: Option<String>,
    /// Keep only methods whose owning type (impl/class) equals this name.
    owner: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchStructsInput {
    pattern: String,
    limit: Option<usize>,
    language: Option<String>,
    /// Keep only types of this kind (struct/enum/trait/class/abstract_class/
    /// interface/type_alias).
    kind: Option<String>,
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

#[derive(Debug, Deserialize)]
struct TraceCallersInput {
    function_name: String,
    max_depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeImpactInput {
    function_name: String,
}

#[derive(Debug, Deserialize, Default)]
struct GetRepositoryTreeInput {
    include_file_details: Option<bool>,
    max_depth: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::registry::{DefaultLanguageRegistry, LanguageAnalyzerRegistry};
    use crate::analyzers::rust::RustAnalyzer;
    use crate::internal::config::FileScanningConfig;

    // Helper to create minimal test instances
    fn create_test_repo_map() -> Arc<Mutex<RepoMap>> {
        Arc::new(Mutex::new(RepoMap::new()))
    }

    // Build a registry handle with the Rust analyzer registered, mirroring the
    // default configuration used throughout the crate.
    fn create_test_registry() -> RegistryHandle {
        let mut registry = DefaultLanguageRegistry::new();
        registry
            .register(Box::new(RustAnalyzer::new().unwrap()))
            .unwrap();
        RegistryHandle::new(&registry)
    }

    fn create_mock_tools() -> LocalAnalysisTools {
        let repo_map = create_test_repo_map();
        LocalAnalysisTools::new(repo_map, create_test_registry())
    }

    // === Tool Schema Tests ===

    #[test]
    fn test_tool_schemas_creation() {
        let tools = create_mock_tools();
        let schemas = tools.get_tool_schemas();

        assert_eq!(schemas.len(), 8, "Should have exactly 8 tool schemas");

        let tool_names: Vec<_> = schemas.iter().map(|s| &s.name).collect();
        assert!(tool_names.contains(&&"search_functions".to_string()));
        assert!(tool_names.contains(&&"search_structs".to_string()));
        assert!(tool_names.contains(&&"analyze_file".to_string()));
        assert!(tool_names.contains(&&"get_dependencies".to_string()));
        assert!(tool_names.contains(&&"find_callers".to_string()));
        assert!(tool_names.contains(&&"trace_callers".to_string()));
        assert!(tool_names.contains(&&"analyze_impact".to_string()));
        assert!(tool_names.contains(&&"get_repository_tree".to_string()));

        // The graph tools' descriptions must explain the ambiguity vocabulary so an
        // agent knows candidate callers are not confirmed (P0-4).
        let desc_of = |name: &str| {
            schemas
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.description.to_lowercase())
                .unwrap()
        };
        assert!(desc_of("trace_callers").contains("ambiguous"));
        assert!(desc_of("trace_callers").contains("candidate"));
        assert!(desc_of("analyze_impact").contains("candidate"));
    }

    #[test]
    fn test_tool_schemas_have_required_fields() {
        let tools = create_mock_tools();
        let schemas = tools.get_tool_schemas();

        for schema in schemas {
            assert!(!schema.name.is_empty(), "Tool name should not be empty");
            assert!(
                !schema.description.is_empty(),
                "Tool description should not be empty"
            );
            assert!(
                schema.input_schema.is_object(),
                "Input schema should be an object"
            );

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
        assert!(
            result.data["error"]
                .as_str()
                .unwrap()
                .contains("Failed to read file")
        );
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

    // === Trace Callers / Analyze Impact Tests ===

    // Build tools whose repo_map contains a multi-level chain a -> b -> c -> leaf
    // across separate files, so graph traversal has something to walk.
    fn create_tools_with_chain() -> LocalAnalysisTools {
        use crate::types::{FunctionCall, FunctionSignature, TreeNode};
        let repo_map = Arc::new(Mutex::new(RepoMap::new()));
        {
            let mut rm = repo_map.lock().unwrap();
            let mut mk = |fname: &str, calls: &str| {
                let path = format!("/src/{}.rs", fname);
                let mut node = TreeNode::new(path.clone(), "rust".to_string());
                node.functions.push(
                    FunctionSignature::new(fname.to_string(), path.clone()).with_location(1, 10),
                );
                node.function_calls
                    .push(FunctionCall::new(calls.to_string(), path.clone(), 5));
                node.content_hash = format!("h_{}", fname);
                node
            };
            rm.add_file(mk("a", "b")).unwrap();
            rm.add_file(mk("b", "c")).unwrap();
            rm.add_file(mk("c", "leaf")).unwrap();
        }
        LocalAnalysisTools::new(repo_map, create_test_registry())
    }

    #[tokio::test]
    async fn test_trace_callers_tool() {
        let tools = create_tools_with_chain();
        let input = json!({ "function_name": "leaf" });

        let result = tools.execute_tool("trace_callers", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["function_name"], "leaf");
        // c (depth 1), b (depth 2), a (depth 3)
        assert_eq!(result.data["count"].as_u64().unwrap(), 3);
        assert_eq!(result.data["max_depth_reached"].as_u64().unwrap(), 3);
        let callers = result.data["callers"].as_array().unwrap();
        assert_eq!(callers[0]["function_name"], "c");
        assert_eq!(callers[0]["depth"].as_u64().unwrap(), 1);
        assert_eq!(callers[2]["function_name"], "a");
    }

    #[tokio::test]
    async fn test_trace_callers_max_depth() {
        let tools = create_tools_with_chain();
        let input = json!({ "function_name": "leaf", "max_depth": 1 });

        let result = tools.execute_tool("trace_callers", input).await.unwrap();
        assert_eq!(result.data["count"].as_u64().unwrap(), 1);
        assert_eq!(result.data["callers"][0]["function_name"], "c");
    }

    #[tokio::test]
    async fn test_analyze_impact_tool() {
        let tools = create_tools_with_chain();
        let input = json!({ "function_name": "leaf" });

        let result = tools.execute_tool("analyze_impact", input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.data["direct_callers"].as_u64().unwrap(), 1);
        assert_eq!(result.data["transitive_functions"].as_u64().unwrap(), 3);
        assert_eq!(result.data["affected_file_count"].as_u64().unwrap(), 3);
        let files = result.data["affected_files"].as_array().unwrap();
        assert_eq!(files.len(), 3);
        assert!(
            result.data["summary"]
                .as_str()
                .unwrap()
                .contains("transitively affects 3 functions across 3 files")
        );
    }

    #[tokio::test]
    async fn test_trace_callers_invalid_input() {
        let tools = create_mock_tools();
        let input = json!({ "wrong_field": "value" });
        let result = tools.execute_tool("trace_callers", input).await;
        assert!(result.is_err());
    }

    // Two files each define `load` with a caller chain into it, so tracing `load`
    // produces name-ambiguous direct callers (P0-4 tool-shape coverage).
    fn create_tools_with_collision() -> LocalAnalysisTools {
        use crate::types::{FunctionCall, FunctionSignature, TreeNode};
        let repo_map = Arc::new(Mutex::new(RepoMap::new()));
        {
            let mut rm = repo_map.lock().unwrap();
            let mut mk = |file: &str, caller: &str| {
                let path = format!("/src/{}.rs", file);
                let mut node = TreeNode::new(path.clone(), "rust".to_string());
                node.functions.push(
                    FunctionSignature::new("load".to_string(), path.clone()).with_location(1, 3),
                );
                node.functions.push(
                    FunctionSignature::new(caller.to_string(), path.clone()).with_location(5, 10),
                );
                node.function_calls
                    .push(FunctionCall::new("load".to_string(), path.clone(), 7));
                node.content_hash = format!("h_{}", file);
                node
            };
            rm.add_file(mk("a", "caller_a")).unwrap();
            rm.add_file(mk("b", "caller_b")).unwrap();
        }
        LocalAnalysisTools::new(repo_map, create_test_registry())
    }

    // A repo with a struct + enum and an owned method `Loader::load` that calls
    // parse_config, plus a free function. Exercises the P1-5 surfacing: kind/owner
    // filters and owner-qualified caller display.
    fn create_tools_with_owned_types() -> LocalAnalysisTools {
        use crate::types::{FunctionCall, FunctionSignature, StructSignature, TreeNode, TypeKind};
        let repo_map = Arc::new(Mutex::new(RepoMap::new()));
        {
            let mut rm = repo_map.lock().unwrap();
            let path = "/src/loader.rs".to_string();
            let mut node = TreeNode::new(path.clone(), "rust".to_string());
            node.structs.push(
                StructSignature::new("Config".to_string(), path.clone())
                    .with_kind(TypeKind::Struct),
            );
            node.structs.push(
                StructSignature::new("ConfigError".to_string(), path.clone())
                    .with_kind(TypeKind::Enum),
            );
            node.functions.push(
                FunctionSignature::new("load".to_string(), path.clone())
                    .with_owner("Loader")
                    .with_location(1, 10),
            );
            node.functions.push(
                FunctionSignature::new("helper".to_string(), path.clone()).with_location(12, 15),
            );
            // load() calls parse_config() at line 5 (inside load's span).
            node.function_calls
                .push(FunctionCall::new("parse_config".to_string(), path.clone(), 5));
            node.content_hash = "h_loader".to_string();
            rm.add_file(node).unwrap();
        }
        LocalAnalysisTools::new(repo_map, create_test_registry())
    }

    #[tokio::test]
    async fn test_search_structs_kind_filter() {
        let tools = create_tools_with_owned_types();

        // The task's acceptance: search_structs {pattern: "ConfigError", kind: "enum"}
        // returns the enum, with its kind surfaced.
        let result = tools
            .execute_tool(
                "search_structs",
                json!({ "pattern": "ConfigError", "kind": "enum" }),
            )
            .await
            .unwrap();
        assert_eq!(result.data["count"].as_u64().unwrap(), 1);
        assert_eq!(result.data["results"][0]["name"], "ConfigError");
        assert_eq!(result.data["results"][0]["kind"], "enum");

        // The same pattern with the wrong kind excludes it (not silently ignored).
        let as_struct = tools
            .execute_tool(
                "search_structs",
                json!({ "pattern": "ConfigError", "kind": "struct" }),
            )
            .await
            .unwrap();
        assert_eq!(as_struct.data["count"].as_u64().unwrap(), 0);

        // An unrecognized kind matches nothing rather than ignoring the filter.
        let bogus = tools
            .execute_tool(
                "search_structs",
                json!({ "pattern": "ConfigError", "kind": "widget" }),
            )
            .await
            .unwrap();
        assert_eq!(bogus.data["count"].as_u64().unwrap(), 0);

        // Struct search without a kind filter still surfaces the struct's kind.
        let plain = tools
            .execute_tool("search_structs", json!({ "pattern": "Config" }))
            .await
            .unwrap();
        assert_eq!(plain.data["results"][0]["name"], "Config");
        assert_eq!(plain.data["results"][0]["kind"], "struct");
    }

    #[tokio::test]
    async fn test_search_functions_owner_filter() {
        let tools = create_tools_with_owned_types();

        let owned = tools
            .execute_tool(
                "search_functions",
                json!({ "pattern": "load", "owner": "Loader" }),
            )
            .await
            .unwrap();
        assert_eq!(owned.data["count"].as_u64().unwrap(), 1);
        assert_eq!(owned.data["results"][0]["name"], "load");
        assert_eq!(owned.data["results"][0]["owner"], "Loader");

        // A non-matching owner yields nothing.
        let none = tools
            .execute_tool(
                "search_functions",
                json!({ "pattern": "load", "owner": "Nope" }),
            )
            .await
            .unwrap();
        assert_eq!(none.data["count"].as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_trace_callers_shows_owner_qualified_display() {
        let tools = create_tools_with_owned_types();
        let result = tools
            .execute_tool("trace_callers", json!({ "function_name": "parse_config" }))
            .await
            .unwrap();
        let callers = result.data["callers"].as_array().unwrap();
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0]["function_name"], "load");
        // The owning type is rendered into display_name.
        assert_eq!(callers[0]["display_name"], "Loader::load");
    }

    #[tokio::test]
    async fn test_find_callers_caller_display_qualified() {
        let tools = create_tools_with_owned_types();
        let result = tools
            .execute_tool("find_callers", json!({ "function_name": "parse_config" }))
            .await
            .unwrap();
        let callers = result.data["callers"].as_array().unwrap();
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0]["caller_function"], "load");
        assert_eq!(callers[0]["caller_display"], "Loader::load");
    }

    #[tokio::test]
    async fn test_trace_callers_reports_ambiguity() {
        let tools = create_tools_with_collision();
        let result = tools
            .execute_tool("trace_callers", json!({ "function_name": "load" }))
            .await
            .unwrap();
        assert!(result.success);

        let callers = result.data["callers"].as_array().unwrap();
        assert_eq!(callers.len(), 2);
        // Every resolution value is from the closed set, and both direct callers of
        // the multiply-defined `load` are flagged name_ambiguous.
        for c in callers {
            let res = c["resolution"].as_str().unwrap();
            assert!(res == "exact" || res == "name_ambiguous", "resolution: {res}");
            assert_eq!(res, "name_ambiguous");
        }

        let count = result.data["count"].as_u64().unwrap();
        let exact = result.data["exact_count"].as_u64().unwrap();
        let ambiguous = result.data["ambiguous_count"].as_u64().unwrap();
        assert_eq!(exact + ambiguous, count);
        assert_eq!(ambiguous, 2);

        // ambiguous_names explains the `load` collision with its candidate files.
        let names = result.data["ambiguous_names"].as_array().unwrap();
        let load_entry = names
            .iter()
            .find(|n| n["name"] == "load")
            .expect("load listed as ambiguous");
        assert_eq!(load_entry["definition_count"].as_u64().unwrap(), 2);
        assert_eq!(load_entry["candidate_files"].as_array().unwrap().len(), 2);
        assert_eq!(load_entry["candidates_truncated"], false);
    }

    #[tokio::test]
    async fn test_analyze_impact_splits_ambiguous_counts() {
        let tools = create_tools_with_collision();
        let result = tools
            .execute_tool("analyze_impact", json!({ "function_name": "load" }))
            .await
            .unwrap();
        assert!(result.success);

        // Both callers are candidates, so the confirmed count is 0 and the headline
        // is not inflated by unconfirmed links.
        assert_eq!(result.data["transitive_functions"].as_u64().unwrap(), 0);
        assert_eq!(result.data["ambiguous_functions"].as_u64().unwrap(), 2);
        assert_eq!(result.data["affected_file_count"].as_u64().unwrap(), 0);
        assert_eq!(result.data["candidate_files"].as_array().unwrap().len(), 2);
        assert!(!result.data["ambiguous_names"].as_array().unwrap().is_empty());
        assert!(
            result.data["summary"]
                .as_str()
                .unwrap()
                .contains("candidate caller")
        );
    }

    // === Repository Tree Tests ===

    #[tokio::test]
    async fn test_get_repository_tree_tool() {
        let tools = create_mock_tools();
        let input = json!({
            "include_file_details": true,
            "max_depth": 3
        });

        let result = tools
            .execute_tool("get_repository_tree", input)
            .await
            .unwrap();
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

        let result = tools
            .execute_tool("get_repository_tree", input)
            .await
            .unwrap();
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

        let result = tools
            .execute_tool("get_repository_tree", input)
            .await
            .unwrap();
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
                }
                Err(_) => {
                    // This is also acceptable - the tool properly rejected invalid input
                }
            }
        }
    }

    #[tokio::test]
    async fn test_repository_tree_builds_with_scanned_files() {
        // Test that the repository tree tool properly builds the tree when files are present
        let repo_map = create_test_repo_map();
        let tools = LocalAnalysisTools::new(repo_map.clone(), create_test_registry());

        // Add some test files to the repo map
        {
            let mut map = repo_map.lock().unwrap();

            // Create a test file with functions and structs
            let mut tree_node =
                crate::types::TreeNode::new("/test/example.rs".to_string(), "rust".to_string());
            tree_node
                .functions
                .push(crate::types::FunctionSignature::new(
                    "test_function".to_string(),
                    "/test/example.rs".to_string(),
                ));
            tree_node.structs.push(crate::types::StructSignature::new(
                "TestStruct".to_string(),
                "/test/example.rs".to_string(),
            ));

            map.add_file(tree_node).unwrap();

            // Add another test file in a different directory
            let mut tree_node2 = crate::types::TreeNode::new(
                "/test/subdir/another.rs".to_string(),
                "rust".to_string(),
            );
            tree_node2
                .functions
                .push(crate::types::FunctionSignature::new(
                    "another_function".to_string(),
                    "/test/subdir/another.rs".to_string(),
                ));

            map.add_file(tree_node2).unwrap();

            // Verify files were added
            assert_eq!(map.file_count(), 2);
        }

        // Test the get_repository_tree tool
        let input = json!({
            "include_file_details": true
        });

        let result = tools
            .execute_tool("get_repository_tree", input)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.data["status"], "success");

        // Verify that repository_tree is not the empty repository placeholder
        let repo_tree = &result.data["repository_tree"];
        assert_ne!(repo_tree["name"], "empty_repository");

        // Verify metadata shows the correct number of files
        let metadata = &result.data["metadata"];
        assert!(metadata["total_files"].as_u64().unwrap() > 0);

        // Verify that the note about empty repository is not present
        assert!(metadata.get("note").is_none());

        // Verify the repository tree structure is correct
        assert_eq!(metadata["root_path"], "/test");
        assert!(
            metadata["languages"]
                .as_array()
                .unwrap()
                .contains(&json!("rust"))
        );
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
            "trace_callers",
            "analyze_impact",
            "get_repository_tree",
        ];

        for tool_name in tool_names {
            let minimal_input = match tool_name {
                "search_functions" => json!({"pattern": "test"}),
                "search_structs" => json!({"pattern": "Test"}),
                "analyze_file" => json!({"file_path": "/test.rs"}),
                "get_dependencies" => json!({"file_path": "/test.rs"}),
                "find_callers" => json!({"function_name": "test"}),
                "trace_callers" => json!({"function_name": "test"}),
                "analyze_impact" => json!({"function_name": "test"}),
                "get_repository_tree" => json!({}),
                _ => json!({}),
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

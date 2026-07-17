use anyhow::{Context, Result};
use serde_json;
use std::path::Path;
use std::time::Instant;
use tracing::info;

// Use public API instead of direct internal access
use crate::{
    LoreGrep,
    core::types::ScanResult as PublicScanResult,
    internal::{
        cli_types::{AnalyzeArgs, ExecToolArgs, ScanArgs, SearchArgs},
        config::CliConfig,
    },
};

/// Lightweight search result used for plain, machine-facing output.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub result_type: String,
    pub content: String,
    pub file_path: String,
    pub line: Option<u32>,
    pub context: Option<String>,
}

impl SearchResult {
    pub fn new(result_type: String, content: String, file_path: String, line: Option<u32>) -> Self {
        Self {
            result_type,
            content,
            file_path,
            line,
            context: None,
        }
    }

    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

pub struct CliApp {
    config: CliConfig,
    loregrep: LoreGrep,
    verbose: bool,
}

impl CliApp {
    pub async fn new(config: CliConfig, verbose: bool, _colors_enabled: bool) -> Result<Self> {
        info!("Initializing Loregrep CLI");

        // Create LoreGrep instance using public API
        let mut builder = LoreGrep::builder()
            .with_all_analyzers() // Rust, Python, TypeScript/TSX
            .max_files(10000) // Default max files
            .cache_ttl(config.cache.ttl_hours * 3600) // Convert hours to seconds
            .include_patterns(config.file_scanning.include_patterns.clone())
            .exclude_patterns(config.file_scanning.exclude_patterns.clone())
            .max_file_size(config.file_scanning.max_file_size)
            .follow_symlinks(config.file_scanning.follow_symlinks);

        // Configure depth limit
        if let Some(depth) = config.file_scanning.max_depth {
            builder = builder.max_depth(depth);
        } else {
            builder = builder.unlimited_depth();
        }

        let loregrep = builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create LoreGrep instance: {}", e))?;

        // Create cache directory if it doesn't exist
        if config.cache.enabled {
            if let Some(parent) = config.cache.path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("Failed to create cache directory")?;
            }
        }

        if verbose {
            eprintln!("LoreGrep initialized with public API");
        }

        Ok(Self {
            config,
            loregrep,
            verbose,
        })
    }

    pub async fn scan(&mut self, args: ScanArgs) -> Result<()> {
        let start_time = Instant::now();

        // Show absolute path for clarity
        let abs_path = args
            .path
            .canonicalize()
            .unwrap_or_else(|_| args.path.clone());

        if self.verbose {
            eprintln!("Scanning directory: {}", abs_path.display());
            eprintln!(
                "Include patterns: {:?}",
                self.config.file_scanning.include_patterns
            );
            eprintln!(
                "Exclude patterns: {:?}",
                self.config.file_scanning.exclude_patterns
            );
        }

        // Use public API to scan the repository
        let scan_result = self
            .loregrep
            .scan(&args.path.to_string_lossy())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scan repository: {}", e))?;

        // Display scan results using public API data
        self.print_public_scan_results(&scan_result);

        // Cache results if enabled
        if args.cache && self.config.cache.enabled {
            self.save_cache(&args.path).await?;
        }

        if self.verbose {
            eprintln!("Total scan time: {:?}", start_time.elapsed());
        }

        Ok(())
    }

    /// Execute a single analysis tool and print its `ToolResult` as JSON to stdout.
    /// Scans the target path first to populate the index; all diagnostics go to stderr,
    /// so stdout carries only the JSON result (for agent/tool consumption).
    pub async fn exec_tool(&mut self, args: ExecToolArgs) -> Result<()> {
        self.loregrep
            .scan(&args.path.to_string_lossy())
            .await
            .map_err(|e| anyhow::anyhow!("Scan failed: {}", e))?;

        let params: serde_json::Value = serde_json::from_str(&args.params)
            .map_err(|e| anyhow::anyhow!("Invalid --params JSON: {}", e))?;

        let result = self
            .loregrep
            .execute_tool(&args.tool, params)
            .await
            .map_err(|e| anyhow::anyhow!("Tool execution failed: {}", e))?;

        println!("{}", serde_json::to_string_pretty(&result)?);

        if !result.success {
            std::process::exit(1);
        }
        Ok(())
    }

    pub async fn search(&self, args: SearchArgs) -> Result<()> {
        if !self.loregrep.is_scanned() {
            eprintln!("Repository not scanned. Run 'scan' first to populate data.");
            return Ok(());
        }

        let start_time = Instant::now();

        if self.verbose {
            eprintln!("Query: {}", args.query);
            eprintln!("Search type: {}", args.r#type);
            eprintln!(
                "Fuzzy matching: {}",
                if args.fuzzy { "enabled" } else { "disabled" }
            );
        }

        // Perform search using public API tools
        let results =
            match args.r#type.as_str() {
                "function" | "func" => {
                    let tool_result = self
                        .loregrep
                        .execute_tool(
                            "search_functions",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit
                            }),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("Function search failed: {}", e))?;

                    if tool_result.success {
                        self.convert_tool_result_to_search_results(tool_result.data, "function")
                    } else {
                        eprintln!("Search failed: {:?}", tool_result.error);
                        Vec::new()
                    }
                }
                "struct" => {
                    let tool_result = self
                        .loregrep
                        .execute_tool(
                            "search_structs",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit
                            }),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("Struct search failed: {}", e))?;

                    if tool_result.success {
                        self.convert_tool_result_to_search_results(tool_result.data, "struct")
                    } else {
                        eprintln!("Search failed: {:?}", tool_result.error);
                        Vec::new()
                    }
                }
                "all" => {
                    let mut all_results = Vec::new();

                    // Search functions
                    if let Ok(func_result) = self
                        .loregrep
                        .execute_tool(
                            "search_functions",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit / 2
                            }),
                        )
                        .await
                    {
                        if func_result.success {
                            all_results.extend(self.convert_tool_result_to_search_results(
                                func_result.data,
                                "function",
                            ));
                        }
                    }

                    // Search structs
                    if let Ok(struct_result) = self
                        .loregrep
                        .execute_tool(
                            "search_structs",
                            serde_json::json!({
                                "pattern": args.query,
                                "limit": args.limit / 2
                            }),
                        )
                        .await
                    {
                        if struct_result.success {
                            all_results.extend(self.convert_tool_result_to_search_results(
                                struct_result.data,
                                "struct",
                            ));
                        }
                    }

                    all_results
                }
                _ => {
                    eprintln!(
                        "Unknown search type: {}. Available types: function, struct, all",
                        args.r#type
                    );
                    return Ok(());
                }
            };

        // Print results (data goes to stdout)
        for result in &results {
            match result.line {
                Some(line) => println!("{}:{}: {}", result.file_path, line, result.content),
                None => println!("{}: {}", result.file_path, result.content),
            }
        }

        if self.verbose && !results.is_empty() {
            eprintln!("Search completed in {:?}", start_time.elapsed());
        }

        Ok(())
    }

    pub async fn analyze(&mut self, args: AnalyzeArgs) -> Result<()> {
        if !args.file.exists() {
            eprintln!("Path not found: {}", args.file.display());
            return Ok(());
        }

        let start_time = Instant::now();

        if args.file.is_dir() {
            // Directory analysis - analyze all files in the directory
            if self.verbose {
                eprintln!("Analyzing directory: {}", args.file.display());
                eprintln!("Output format: {}", args.format);
            }

            // There is no dedicated "analyze_directory" tool; the repository
            // tree tool is the natural fit for a directory overview.
            let tool_result = self
                .loregrep
                .execute_tool(
                    "get_repository_tree",
                    serde_json::json!({
                        "include_file_details": true,
                        "max_depth": 0
                    }),
                )
                .await
                .map_err(|e| anyhow::anyhow!("Directory analysis failed: {}", e))?;

            if !tool_result.success {
                eprintln!("Analysis failed: {:?}", tool_result.error);
                return Ok(());
            }

            // Display directory results
            self.display_directory_analysis(&tool_result.data, &args);

            if self.verbose {
                eprintln!("Directory analysis completed in {:?}", start_time.elapsed());
            }
        } else {
            // Single file analysis
            if self.verbose {
                eprintln!("Analyzing file: {}", args.file.display());
                eprintln!("Output format: {}", args.format);
            }

            // Use public API to analyze file
            let tool_result = self
                .loregrep
                .execute_tool(
                    "analyze_file",
                    serde_json::json!({
                        "file_path": args.file.to_string_lossy(),
                        "include_source": true
                    }),
                )
                .await
                .map_err(|e| anyhow::anyhow!("File analysis failed: {}", e))?;

            if !tool_result.success {
                eprintln!("Analysis failed: {:?}", tool_result.error);
                return Ok(());
            }

            // Display results based on format
            match args.format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&tool_result.data)
                        .context("Failed to serialize analysis to JSON")?;
                    println!("{}", json);
                }
                "text" => {
                    self.display_tool_analysis_text(&tool_result.data, &args);
                }
                "tree" => {
                    self.display_tool_analysis_tree(&tool_result.data);
                }
                _ => {
                    eprintln!("Unknown output format: {}", args.format);
                    return Ok(());
                }
            }

            if self.verbose {
                eprintln!("Analysis completed in {:?}", start_time.elapsed());
            }
        }

        Ok(())
    }

    pub async fn show_config(&self) -> Result<()> {
        let config_json = serde_json::to_string_pretty(&self.config)
            .context("Failed to serialize configuration")?;

        println!("{}", config_json);

        Ok(())
    }

    // Helper methods for public API conversion
    fn print_public_scan_results(&self, scan_result: &PublicScanResult) {
        println!("Files scanned: {}", scan_result.files_scanned);
        println!("Functions found: {}", scan_result.functions_found);
        println!("Structs found: {}", scan_result.structs_found);
        println!("Duration: {}ms", scan_result.duration_ms);

        if !scan_result.languages.is_empty() {
            println!("Languages: {:?}", scan_result.languages);
        }
    }

    fn convert_tool_result_to_search_results(
        &self,
        data: serde_json::Value,
        result_type: &str,
    ) -> Vec<SearchResult> {
        let mut results = Vec::new();

        // The search tools return an object of the shape
        // `{ "status": .., "pattern": .., "results": [ ..items.. ], "count": N }`,
        // so read the items out of the `results` array (not the top-level value).
        if let Some(items) = data.get("results").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    let file_path = item
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    // Items are serialized `FunctionSignature`/`StructSignature`,
                    // which expose `start_line`/`end_line` rather than `line_number`.
                    let line_number = item
                        .get("start_line")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32);

                    let signature = match result_type {
                        "function" => {
                            let params = item
                                .get("parameters")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            let return_type = item
                                .get("return_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if return_type.is_empty() {
                                format!("fn {}(...) [{}params]", name, params)
                            } else {
                                format!("fn {}(...) -> {} [{}params]", name, return_type, params)
                            }
                        }
                        "struct" => {
                            let fields = item
                                .get("fields")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            format!("struct {} {{ {}fields }}", name, fields)
                        }
                        _ => name.to_string(),
                    };

                    results.push(SearchResult::new(
                        result_type.to_string(),
                        signature,
                        file_path,
                        line_number,
                    ));
                }
            }
        }

        results
    }

    fn display_tool_analysis_text(&self, data: &serde_json::Value, args: &AnalyzeArgs) {
        if let Some(file_path) = data.get("file_path").and_then(|v| v.as_str()) {
            println!("File: {}", file_path);
        }

        if let Some(language) = data.get("language").and_then(|v| v.as_str()) {
            println!("Language: {}", language);
        }

        // Display functions
        if args.functions || (!args.structs && !args.imports) {
            if let Some(functions) = data.get("functions").and_then(|v| v.as_array()) {
                if !functions.is_empty() {
                    println!("Functions:");
                    for func in functions {
                        if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                            let params = func
                                .get("parameters")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            let return_type = func
                                .get("return_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            println!(
                                "  fn {}({} params) -> {}",
                                name,
                                params,
                                if return_type.is_empty() {
                                    "()"
                                } else {
                                    return_type
                                }
                            );
                        }
                    }
                }
            }
        }

        // Display structs
        if args.structs || (!args.functions && !args.imports) {
            if let Some(structs) = data.get("structs").and_then(|v| v.as_array()) {
                if !structs.is_empty() {
                    println!("Structs:");
                    for struct_item in structs {
                        if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                            let fields = struct_item
                                .get("fields")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            println!("  struct {} {{ {} fields }}", name, fields);
                        }
                    }
                }
            }
        }
    }

    fn display_tool_analysis_tree(&self, data: &serde_json::Value) {
        if let Some(file_path) = data.get("file_path").and_then(|v| v.as_str()) {
            println!("{}", file_path);

            if let Some(functions) = data.get("functions").and_then(|v| v.as_array()) {
                for func in functions {
                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                        println!("  fn {}", name);
                    }
                }
            }

            if let Some(structs) = data.get("structs").and_then(|v| v.as_array()) {
                for struct_item in structs {
                    if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                        println!("  struct {}", name);
                    }
                }
            }
        }
    }

    /// Collect all `File` nodes from a `get_repository_tree` directory node,
    /// walking directories recursively. Each returned value is the `FileNode`
    /// JSON object (with a `skeleton` field).
    fn collect_tree_files<'a>(node: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            for child in children {
                match child.get("type").and_then(|v| v.as_str()) {
                    Some("File") => out.push(child),
                    Some("Directory") => Self::collect_tree_files(child, out),
                    _ => {}
                }
            }
        }
    }

    fn display_directory_analysis(&self, data: &serde_json::Value, args: &AnalyzeArgs) {
        match args.format.as_str() {
            "json" => {
                let json = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
                println!("{}", json);
            }
            "text" => {
                let tree = data
                    .get("repository_tree")
                    .unwrap_or(&serde_json::Value::Null);
                let mut files = Vec::new();
                Self::collect_tree_files(tree, &mut files);

                let mut total_functions = 0;
                let mut total_structs = 0;

                for file_data in &files {
                    let skeleton = file_data
                        .get("skeleton")
                        .unwrap_or(&serde_json::Value::Null);

                    let file_path = skeleton
                        .get("path")
                        .and_then(|v| v.as_str())
                        .or_else(|| file_data.get("path").and_then(|v| v.as_str()))
                        .unwrap_or("unknown");
                    println!("File: {}", file_path);

                    if let Some(language) = skeleton.get("language").and_then(|v| v.as_str()) {
                        if !language.is_empty() {
                            println!("Language: {}", language);
                        }
                    }

                    // Display functions
                    if args.functions || (!args.structs && !args.imports) {
                        if let Some(functions) =
                            skeleton.get("functions").and_then(|v| v.as_array())
                        {
                            if !functions.is_empty() {
                                println!("Functions:");
                                for func in functions {
                                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                        let params = func
                                            .get("parameter_count")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        let return_type = func
                                            .get("return_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");

                                        println!(
                                            "  fn {}({} params) -> {}",
                                            name,
                                            params,
                                            if return_type.is_empty() {
                                                "()"
                                            } else {
                                                return_type
                                            }
                                        );
                                        total_functions += 1;
                                    }
                                }
                            }
                        }
                    }

                    // Display structs
                    if args.structs || (!args.functions && !args.imports) {
                        if let Some(structs) = skeleton.get("structs").and_then(|v| v.as_array()) {
                            if !structs.is_empty() {
                                println!("Structs:");
                                for struct_item in structs {
                                    if let Some(name) =
                                        struct_item.get("name").and_then(|v| v.as_str())
                                    {
                                        let fields = struct_item
                                            .get("field_count")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        println!("  struct {} {{ {} fields }}", name, fields);
                                        total_structs += 1;
                                    }
                                }
                            }
                        }
                    }

                    println!(); // Blank line between files
                }

                // Summary
                println!(
                    "Summary: {} functions, {} structs across {} files",
                    total_functions,
                    total_structs,
                    files.len()
                );
            }
            "tree" => {
                let tree = data
                    .get("repository_tree")
                    .unwrap_or(&serde_json::Value::Null);

                if let Some(root_path) = data
                    .get("metadata")
                    .and_then(|m| m.get("root_path"))
                    .and_then(|v| v.as_str())
                    .or_else(|| tree.get("path").and_then(|v| v.as_str()))
                {
                    println!("{}", root_path);
                }

                let mut files = Vec::new();
                Self::collect_tree_files(tree, &mut files);

                for file_data in &files {
                    let skeleton = file_data
                        .get("skeleton")
                        .unwrap_or(&serde_json::Value::Null);
                    let file_name = file_data
                        .get("name")
                        .and_then(|v| v.as_str())
                        .or_else(|| skeleton.get("path").and_then(|v| v.as_str()))
                        .unwrap_or("unknown");
                    println!("  {}", file_name);

                    if let Some(functions) = skeleton.get("functions").and_then(|v| v.as_array()) {
                        for func in functions {
                            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                println!("    fn {}", name);
                            }
                        }
                    }

                    if let Some(structs) = skeleton.get("structs").and_then(|v| v.as_array()) {
                        for struct_item in structs {
                            if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                                println!("    struct {}", name);
                            }
                        }
                    }
                }
            }
            _ => {
                eprintln!("Unknown output format: {}", args.format);
            }
        }
    }

    // Helper methods

    async fn save_cache(&self, _root_path: &Path) -> Result<()> {
        // Cache operations would be implemented here
        // For now, this is a placeholder
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::test;

    fn create_test_config() -> CliConfig {
        CliConfig::default()
    }

    fn create_test_rust_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let file_path = dir.path().join(name);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    #[test]
    async fn test_cli_app_creation() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await;
        assert!(app.is_ok());
    }

    #[test]
    async fn test_analyze_simple_rust_file() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = r#"
pub fn hello_world() -> String {
    "Hello, World!".to_string()
}

pub struct TestStruct {
    pub name: String,
    pub value: i32,
}

use std::collections::HashMap;
"#;
        let file_path = create_test_rust_file(&temp_dir, "test.rs", rust_content);

        let config = create_test_config();
        let app = CliApp::new(config, false, false).await.unwrap();

        // Use public API to analyze file
        let result = app
            .loregrep
            .execute_tool(
                "analyze_file",
                serde_json::json!({
                    "file_path": file_path.to_string_lossy(),
                    "include_source": false
                }),
            )
            .await;

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.success);

        // Check that we got analysis data
        assert!(tool_result.data.get("language").is_some());
        assert!(tool_result.data.get("functions").is_some());
        assert!(tool_result.data.get("structs").is_some());
    }

    #[test]
    async fn test_scan_directory() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple Rust files
        create_test_rust_file(&temp_dir, "main.rs", "fn main() {}");
        create_test_rust_file(&temp_dir, "lib.rs", "pub fn lib_func() {}");
        create_test_rust_file(&temp_dir, "utils.rs", "pub struct Utils {}");

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let scan_args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            include: vec![],
            exclude: vec![],
            follow_symlinks: false,
            cache: false,
        };

        let result = app.scan(scan_args).await;
        assert!(result.is_ok());

        // Check that repository was scanned using public API
        assert!(app.loregrep.is_scanned());
        let stats = app.loregrep.get_stats().unwrap();
        assert!(stats.files_scanned > 0);
    }

    #[test]
    async fn test_analyze_command() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = r#"
pub fn test_function(x: i32, y: String) -> bool {
    x > 0 && !y.is_empty()
}

struct PrivateStruct {
    field: String,
}
"#;
        let file_path = create_test_rust_file(&temp_dir, "test.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: file_path,
            format: "text".to_string(),
            functions: true,
            structs: true,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_search_empty_repo_map() {
        let config = create_test_config();
        let app = CliApp::new(config, false, false).await.unwrap();

        let search_args = SearchArgs {
            query: "test".to_string(),
            path: std::path::PathBuf::from("."),
            r#type: "function".to_string(),
            limit: 10,
            fuzzy: false,
        };

        let result = app.search(search_args).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_config_display() {
        let config = create_test_config();
        let app = CliApp::new(config, false, false).await.unwrap();

        let result = app.show_config().await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_convert_tool_result_reads_results_array() {
        // Regression test: search tools return an OBJECT with a `results`
        // array (not a bare array), and items carry `start_line` (not
        // `line_number`). This test would fail against the old code that read
        // `data.as_array()` / `line_number`.
        let config = create_test_config();
        let app = CliApp::new(config, false, false).await.unwrap();

        let tool_data = serde_json::json!({
            "status": "success",
            "pattern": "foo",
            "results": [
                {
                    "name": "foo_bar",
                    "file_path": "/src/foo.rs",
                    "start_line": 42,
                    "end_line": 50,
                    "parameters": [{"name": "x"}, {"name": "y"}],
                    "return_type": "bool"
                }
            ],
            "count": 1
        });

        let results = app.convert_tool_result_to_search_results(tool_data, "function");
        assert_eq!(results.len(), 1, "should read items from the results array");
        assert_eq!(results[0].file_path, "/src/foo.rs");
        assert_eq!(
            results[0].line,
            Some(42),
            "line should come from start_line"
        );
        assert!(results[0].content.contains("foo_bar"));
    }

    #[test]
    async fn test_exec_tool_scans_and_executes() {
        // exec-tool should scan the given path then run the named tool and succeed.
        let temp_dir = TempDir::new().unwrap();
        create_test_rust_file(
            &temp_dir,
            "sample.rs",
            "pub fn exec_target() -> i32 { 1 }\n",
        );

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let args = ExecToolArgs {
            tool: "search_functions".to_string(),
            params: r#"{"pattern":"exec_target"}"#.to_string(),
            path: temp_dir.path().to_path_buf(),
        };
        // Success path returns Ok (prints JSON to stdout; no process exit).
        assert!(app.exec_tool(args).await.is_ok());

        // The exec-tool scan populated the index; the tool found the function.
        let result = app
            .loregrep
            .execute_tool(
                "search_functions",
                serde_json::json!({"pattern": "exec_target"}),
            )
            .await
            .unwrap();
        assert!(result.success);
        let count = result
            .data
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(count >= 1, "expected exec-tool scan to index exec_target");
    }

    #[tokio::test]
    async fn test_analyze_directory_produces_output() {
        // Regression test for the `analyze <directory>` path, which used to call
        // a nonexistent `analyze_directory` tool. It now routes to
        // `get_repository_tree`; verify the whole path succeeds and that the
        // returned tree contains the scanned file with its symbols.
        let temp_dir = TempDir::new().unwrap();
        let rust_content = r#"
pub fn dir_function() -> i32 {
    7
}

pub struct DirStruct {
    pub field: String,
}
"#;
        create_test_rust_file(&temp_dir, "sample.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        // Scan first so the repository tree is populated.
        let scan_args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            include: vec![],
            exclude: vec![],
            follow_symlinks: false,
            cache: false,
        };
        app.scan(scan_args).await.unwrap();

        // The analyze-directory branch should complete successfully.
        let analyze_args = AnalyzeArgs {
            file: temp_dir.path().to_path_buf(),
            format: "text".to_string(),
            functions: true,
            structs: true,
            imports: false,
        };
        assert!(app.analyze(analyze_args).await.is_ok());

        // Exercise the underlying tool call and assert non-empty output.
        let tool_result = app
            .loregrep
            .execute_tool(
                "get_repository_tree",
                serde_json::json!({
                    "include_file_details": true,
                    "max_depth": 0
                }),
            )
            .await
            .unwrap();
        assert!(tool_result.success);

        let tree = tool_result
            .data
            .get("repository_tree")
            .expect("repository_tree present");
        let mut files = Vec::new();
        CliApp::collect_tree_files(tree, &mut files);
        assert!(
            !files.is_empty(),
            "directory analysis should surface at least one file"
        );

        // The scanned file should expose its function and struct.
        let has_symbols = files.iter().any(|f| {
            let skeleton = f.get("skeleton");
            let funcs = skeleton
                .and_then(|s| s.get("functions"))
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            let structs = skeleton
                .and_then(|s| s.get("structs"))
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            funcs || structs
        });
        assert!(
            has_symbols,
            "scanned file should carry functions/structs in its skeleton"
        );
    }

    #[test]
    async fn test_analyze_nonexistent_file() {
        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: std::path::PathBuf::from("nonexistent.rs"),
            format: "text".to_string(),
            functions: false,
            structs: false,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok()); // Should handle gracefully
    }

    #[test]
    async fn test_analyze_json_format() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = "pub fn simple() {}";
        let file_path = create_test_rust_file(&temp_dir, "simple.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: file_path,
            format: "json".to_string(),
            functions: false,
            structs: false,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_analyze_tree_format() {
        let temp_dir = TempDir::new().unwrap();
        let rust_content = "pub fn simple() {}";
        let file_path = create_test_rust_file(&temp_dir, "simple.rs", rust_content);

        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();

        let analyze_args = AnalyzeArgs {
            file: file_path,
            format: "tree".to_string(),
            functions: false,
            structs: false,
            imports: false,
        };

        let result = app.analyze(analyze_args).await;
        assert!(result.is_ok());
    }
}

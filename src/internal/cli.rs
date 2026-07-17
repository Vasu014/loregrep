use anyhow::{Context, Result};
use serde_json;
use std::path::Path;
use std::time::Instant;
use tracing::info;

// Use public API instead of direct internal access
use crate::{
    core::types::ScanResult as PublicScanResult,
    internal::{
        cli_types::{AnalyzeArgs, ScanArgs, SearchArgs},
        config::CliConfig,
    },
    types::{ExportStatement, FunctionSignature, ImportStatement, StructSignature},
    LoreGrep,
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
            .with_rust_analyzer()
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

            // Use public API to analyze directory
            let tool_result = self
                .loregrep
                .execute_tool(
                    "analyze_directory",
                    serde_json::json!({
                        "directory_path": args.file.to_string_lossy(),
                        "include_source": true
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

        // Handle the tool result data based on type
        if let Some(items) = data.as_array() {
            for item in items {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    let file_path = item
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let line_number = item
                        .get("line_number")
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

    fn display_directory_analysis(&self, data: &serde_json::Value, args: &AnalyzeArgs) {
        match args.format.as_str() {
            "json" => {
                let json = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
                println!("{}", json);
            }
            "text" => {
                if let Some(files) = data.get("files").and_then(|v| v.as_array()) {
                    let mut total_functions = 0;
                    let mut total_structs = 0;

                    for file_data in files {
                        if let Some(file_path) = file_data.get("file_path").and_then(|v| v.as_str())
                        {
                            println!("File: {}", file_path);

                            if let Some(language) =
                                file_data.get("language").and_then(|v| v.as_str())
                            {
                                println!("Language: {}", language);
                            }

                            // Display functions
                            if args.functions || (!args.structs && !args.imports) {
                                if let Some(functions) =
                                    file_data.get("functions").and_then(|v| v.as_array())
                                {
                                    if !functions.is_empty() {
                                        println!("Functions:");
                                        for func in functions {
                                            if let Some(name) =
                                                func.get("name").and_then(|v| v.as_str())
                                            {
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
                                                total_functions += 1;
                                            }
                                        }
                                    }
                                }
                            }

                            // Display structs
                            if args.structs || (!args.functions && !args.imports) {
                                if let Some(structs) =
                                    file_data.get("structs").and_then(|v| v.as_array())
                                {
                                    if !structs.is_empty() {
                                        println!("Structs:");
                                        for struct_item in structs {
                                            if let Some(name) =
                                                struct_item.get("name").and_then(|v| v.as_str())
                                            {
                                                let fields = struct_item
                                                    .get("fields")
                                                    .and_then(|v| v.as_array())
                                                    .map(|arr| arr.len())
                                                    .unwrap_or(0);
                                                println!(
                                                    "  struct {} {{ {} fields }}",
                                                    name, fields
                                                );
                                                total_structs += 1;
                                            }
                                        }
                                    }
                                }
                            }

                            println!(); // Blank line between files
                        }
                    }

                    // Summary
                    println!(
                        "Summary: {} functions, {} structs across {} files",
                        total_functions,
                        total_structs,
                        files.len()
                    );
                }
            }
            "tree" => {
                if let Some(directory_path) = data.get("directory_path").and_then(|v| v.as_str()) {
                    println!("{}", directory_path);

                    if let Some(files) = data.get("files").and_then(|v| v.as_array()) {
                        for file_data in files {
                            if let Some(file_path) =
                                file_data.get("file_path").and_then(|v| v.as_str())
                            {
                                let file_name = std::path::Path::new(file_path)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or(file_path);
                                println!("  {}", file_name);

                                if let Some(functions) =
                                    file_data.get("functions").and_then(|v| v.as_array())
                                {
                                    for func in functions {
                                        if let Some(name) =
                                            func.get("name").and_then(|v| v.as_str())
                                        {
                                            println!("    fn {}", name);
                                        }
                                    }
                                }

                                if let Some(structs) =
                                    file_data.get("structs").and_then(|v| v.as_array())
                                {
                                    for struct_item in structs {
                                        if let Some(name) =
                                            struct_item.get("name").and_then(|v| v.as_str())
                                        {
                                            println!("    struct {}", name);
                                        }
                                    }
                                }
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

    fn convert_function_results(&self, functions: Vec<&FunctionSignature>) -> Vec<SearchResult> {
        functions
            .into_iter()
            .map(|func| {
                let context = if func.start_line > 0 && func.end_line > 0 {
                    Some(format!("{}-{}", func.start_line, func.end_line))
                } else {
                    None
                };

                SearchResult::new(
                    "function".to_string(),
                    func.format(),
                    "".to_string(), // file_path would be set by caller
                    Some(func.start_line),
                )
                .with_context(context.unwrap_or_default())
            })
            .collect()
    }

    fn convert_struct_results(&self, structs: Vec<&StructSignature>) -> Vec<SearchResult> {
        structs
            .into_iter()
            .map(|struct_def| {
                let context = if struct_def.start_line > 0 && struct_def.end_line > 0 {
                    Some(format!("{}-{}", struct_def.start_line, struct_def.end_line))
                } else {
                    None
                };

                SearchResult::new(
                    "struct".to_string(),
                    struct_def.format(),
                    "".to_string(), // file_path would be set by caller
                    Some(struct_def.start_line),
                )
                .with_context(context.unwrap_or_default())
            })
            .collect()
    }

    fn convert_import_results(&self, imports: Vec<&ImportStatement>) -> Vec<SearchResult> {
        imports
            .into_iter()
            .map(|import| {
                SearchResult::new(
                    "import".to_string(),
                    format!("use {};", import.module_path),
                    "".to_string(), // file_path would be set by caller
                    Some(import.line_number),
                )
            })
            .collect()
    }

    fn convert_export_results(&self, exports: Vec<&ExportStatement>) -> Vec<SearchResult> {
        exports
            .into_iter()
            .map(|export| {
                SearchResult::new(
                    "export".to_string(),
                    format!("pub {}", export.exported_item),
                    "".to_string(), // file_path would be set by caller
                    Some(export.line_number),
                )
            })
            .collect()
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
    async fn test_convert_function_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();

        let func = FunctionSignature::new("test_func".to_string(), "/test/file.rs".to_string())
            .with_visibility(true)
            .with_location(10, 20);

        let results = app.convert_function_results(vec![&func]);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("test_func"));
        assert!(results[0].context.as_ref().unwrap().contains("10-20"));
    }

    #[test]
    async fn test_convert_struct_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();

        let struct_def =
            StructSignature::new("TestStruct".to_string(), "/test/file.rs".to_string())
                .with_visibility(true)
                .with_location(5, 15);

        let results = app.convert_struct_results(vec![&struct_def]);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("TestStruct"));
        assert!(results[0].context.as_ref().unwrap().contains("5-15"));
    }

    #[test]
    async fn test_convert_import_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();

        let import = ImportStatement::new(
            "std::collections::HashMap".to_string(),
            "/test/file.rs".to_string(),
        )
        .with_line_number(1);

        let results = app.convert_import_results(vec![&import]);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("std::collections::HashMap"));
        assert_eq!(results[0].line, Some(1));
    }

    #[test]
    async fn test_convert_export_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();

        let export = ExportStatement::new("MyFunction".to_string(), "/test/file.rs".to_string())
            .with_line_number(10);

        let results = app.convert_export_results(vec![&export]);
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("MyFunction"));
        assert_eq!(results[0].line, Some(10));
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

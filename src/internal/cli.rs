use anyhow::{Context, Result};
use serde_json;
use std::path::Path;
use std::time::Instant;
use tokio::fs;
use tracing::info;

// Use public API instead of direct internal access
use crate::{
    LoreGrep, LoreGrepBuilder, LoreGrepError,
    core::types::{ToolResult, ScanResult as PublicScanResult},
    internal::{
        config::CliConfig,
        cli_types::{AnalyzeArgs, QueryArgs, ScanArgs, SearchArgs},
        ui::{UIManager, ThemeType, formatter::SearchResult},
    },
    types::{
        function::FunctionSignature,
        struct_def::{StructSignature, ImportStatement, ExportStatement},
    },
};

pub struct CliApp {
    config: CliConfig,
    loregrep: LoreGrep,
    verbose: bool,
    ui: UIManager,
}

impl CliApp {
    pub async fn new(config: CliConfig, verbose: bool, colors_enabled: bool) -> Result<Self> {
        info!("Initializing Loregrep CLI");

        // Initialize UI manager with theme
        let theme_type = if let Ok(theme_str) = std::env::var("LOREGREP_THEME") {
            ThemeType::from_str(&theme_str).unwrap_or(ThemeType::Auto)
        } else {
            ThemeType::Auto
        };
        
        let ui = UIManager::new(colors_enabled, theme_type)
            .context("Failed to create UI manager")?;

        // Create LoreGrep instance using public API
        let mut builder = LoreGrep::builder()
            .with_rust_analyzer()
            .max_files(10000)  // Default max files
            .cache_ttl(config.cache.ttl_hours * 3600)  // Convert hours to seconds
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

        let loregrep = builder.build()
            .map_err(|e| anyhow::anyhow!("Failed to create LoreGrep instance: {}", e))?;

        // Create cache directory if it doesn't exist
        if config.cache.enabled {
            if let Some(parent) = config.cache.path.parent() {
                tokio::fs::create_dir_all(parent).await
                    .context("Failed to create cache directory")?;
            }
        }

        if verbose {
            ui.print_success("LoreGrep initialized with public API");
        }

        Ok(Self {
            config,
            loregrep,
            verbose,
            ui,
        })
    }

    pub async fn scan(&mut self, args: ScanArgs) -> Result<()> {
        let start_time = Instant::now();
        
        self.ui.print_header("Repository Scan");
        
        // Show absolute path for clarity
        let abs_path = args.path.canonicalize().unwrap_or_else(|_| args.path.clone());
        self.ui.print_info(&format!("Scanning directory: {}", abs_path.display()));
        
        if self.verbose {
            self.ui.print_info(&format!("Include patterns: {:?}", self.config.file_scanning.include_patterns));
            self.ui.print_info(&format!("Exclude patterns: {:?}", self.config.file_scanning.exclude_patterns));
        }

        // Use public API to scan the repository
        let progress = self.ui.create_scan_progress(100); // Estimated progress
        progress.set_message("Scanning repository...");
        
        let scan_result = self.loregrep.scan(&args.path.to_string_lossy())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scan repository: {}", e))?;
        
        progress.finish_with_message(&format!("Scanned {} files", scan_result.files_scanned));

        // Display scan results using public API data
        self.print_public_scan_results(&scan_result);

        // Cache results if enabled
        if args.cache && self.config.cache.enabled {
            self.save_cache(&args.path).await?;
        }

        let total_duration = start_time.elapsed();
        self.ui.print_success(&format!("Total scan time: {:?}", total_duration));

        Ok(())
    }

    pub async fn search(&self, args: SearchArgs) -> Result<()> {
        self.ui.print_header("Search");

        if !self.loregrep.is_scanned() {
            self.ui.print_warning("Repository not scanned. Run 'scan' first to populate data.");
            return Ok(());
        }

        let start_time = Instant::now();
        
        if self.verbose {
            self.ui.print_info(&format!("Query: {}", args.query));
            self.ui.print_info(&format!("Search type: {}", args.r#type));
            self.ui.print_info(&format!("Fuzzy matching: {}", if args.fuzzy { "enabled" } else { "disabled" }));
        }

        // Perform search using public API tools
        let results = match args.r#type.as_str() {
            "function" | "func" => {
                let tool_result = self.loregrep.execute_tool("search_functions", serde_json::json!({
                    "pattern": args.query,
                    "limit": args.limit
                })).await
                .map_err(|e| anyhow::anyhow!("Function search failed: {}", e))?;
                
                if tool_result.success {
                    self.convert_tool_result_to_search_results(tool_result.data, "function")
                } else {
                    self.ui.print_error(&format!("Search failed: {:?}", tool_result.error));
                    Vec::new()
                }
            },
            "struct" => {
                let tool_result = self.loregrep.execute_tool("search_structs", serde_json::json!({
                    "pattern": args.query,
                    "limit": args.limit
                })).await
                .map_err(|e| anyhow::anyhow!("Struct search failed: {}", e))?;
                
                if tool_result.success {
                    self.convert_tool_result_to_search_results(tool_result.data, "struct")
                } else {
                    self.ui.print_error(&format!("Search failed: {:?}", tool_result.error));
                    Vec::new()
                }
            },
            "all" => {
                let mut all_results = Vec::new();
                
                // Search functions
                if let Ok(func_result) = self.loregrep.execute_tool("search_functions", serde_json::json!({
                    "pattern": args.query,
                    "limit": args.limit / 2
                })).await {
                    if func_result.success {
                        all_results.extend(self.convert_tool_result_to_search_results(func_result.data, "function"));
                    }
                }
                
                // Search structs
                if let Ok(struct_result) = self.loregrep.execute_tool("search_structs", serde_json::json!({
                    "pattern": args.query,
                    "limit": args.limit / 2
                })).await {
                    if struct_result.success {
                        all_results.extend(self.convert_tool_result_to_search_results(struct_result.data, "struct"));
                    }
                }
                
                all_results
            },
            _ => {
                self.ui.print_error_with_suggestions(&format!("Unknown search type: {}", args.r#type), 
                    Some("Available types: function, struct, all"));
                return Ok(());
            }
        };

        let search_duration = start_time.elapsed();

        // Display results using the new formatter
        let formatted_results = self.ui.formatter.format_search_results(&results, &args.query);
        println!("{}", formatted_results);
        
        if self.verbose && !results.is_empty() {
            self.ui.print_info(&format!("Search completed in {:?}", search_duration));
        }

        Ok(())
    }

    pub async fn analyze(&mut self, args: AnalyzeArgs) -> Result<()> {
        self.ui.print_header("File Analysis");

        if !args.file.exists() {
            self.ui.print_error(&format!("File not found: {}", args.file.display()));
            return Ok(());
        }

        if self.verbose {
            self.ui.print_info(&format!("Analyzing file: {}", args.file.display()));
            self.ui.print_info(&format!("Output format: {}", args.format));
        }

        let start_time = Instant::now();
        
        // Use public API to analyze file
        let tool_result = self.loregrep.execute_tool("analyze_file", serde_json::json!({
            "file_path": args.file.to_string_lossy(),
            "include_source": true
        })).await
        .map_err(|e| anyhow::anyhow!("File analysis failed: {}", e))?;
        
        let analysis_duration = start_time.elapsed();

        if !tool_result.success {
            self.ui.print_error(&format!("Analysis failed: {:?}", tool_result.error));
            return Ok(());
        }

        // Display results based on format
        match args.format.as_str() {
            "json" => {
                let json = serde_json::to_string_pretty(&tool_result.data)
                    .context("Failed to serialize analysis to JSON")?;
                println!("{}", json);
            },
            "text" => {
                self.display_tool_analysis_text(&tool_result.data, &args);
            },
            "tree" => {
                self.display_tool_analysis_tree(&tool_result.data);
            },
            _ => {
                self.ui.print_error(&format!("Unknown output format: {}", args.format));
                return Ok(());
            }
        }

        if self.verbose {
            self.ui.print_info(&format!("\nAnalysis completed in {:?}", analysis_duration));
        }

        Ok(())
    }

    pub async fn show_config(&self) -> Result<()> {
        self.ui.print_header("Configuration");

        let config_json = serde_json::to_string_pretty(&self.config)
            .context("Failed to serialize configuration")?;
        
        println!("{}", config_json);
        
        // Show cache status
        self.ui.print_info("\nCache Status:");
        if self.config.cache.enabled {
            self.ui.print_info("  Status: Enabled");
            self.ui.print_info(&format!("  Path: {}", self.config.cache.path.display().to_string()));
            
            if self.config.cache.path.exists() {
                if let Ok(metadata) = std::fs::metadata(&self.config.cache.path) {
                    let size_mb = metadata.len() / (1024 * 1024);
                    self.ui.print_info(&format!("  Size: {} MB", size_mb.to_string()));
                }
            } else {
                self.ui.print_info("  Size: 0 MB (no cache file found)");
            }
        } else {
            self.ui.print_info("  Status: Disabled");
        }

        // Show repository scan status
        self.ui.print_info("\nRepository Status:");
        if self.loregrep.is_scanned() {
            if let Ok(stats) = self.loregrep.get_stats() {
                self.ui.print_info(&format!("  Files scanned: {}", stats.files_scanned));
                self.ui.print_info(&format!("  Functions found: {}", stats.functions_found));
                self.ui.print_info(&format!("  Structs found: {}", stats.structs_found));
            } else {
                self.ui.print_info("  Status: Scanned (stats unavailable)");
            }
        } else {
            self.ui.print_info("  Status: Not scanned");
        }

        Ok(())
    }

    pub async fn query(&mut self, args: QueryArgs) -> Result<()> {
        self.ui.print_header("AI Query Mode");
        
        // AI functionality is not available when using public API only
        self.ui.print_warning("AI query functionality has been temporarily disabled during CLI refactoring.");
        self.ui.print_info("Please use the search and analyze commands for code exploration.");
        self.ui.print_info("Available commands: scan, search, analyze, config");
        
        Ok(())
    }

    // AI methods temporarily disabled during refactoring
    /*
    async fn process_ai_query_with_engine(&self, conversation_engine: &mut ConversationEngine, query: &str) -> Result<()> {
        if self.verbose {
            self.ui.print_info(&format!("Query: {}", query));
        }

        let start_time = Instant::now();
        
        // Show thinking indicator
        self.ui.show_thinking("Processing your query").await;

        // Process the query
        match conversation_engine.process_user_message(query).await {
            Ok(response) => {
                let duration = start_time.elapsed();
                self.ui.print_success("AI Response:");
                let formatted_response = self.ui.formatter.format_ai_response(&response);
                println!("{}", formatted_response);
                
                if self.verbose {
                    self.ui.print_info(&format!("Response time: {:?}", duration));
                    self.ui.print_info(&format!("Conversation messages: {}", conversation_engine.get_message_count()));
                }
            }
            Err(e) => {
                self.ui.print_error_with_suggestions(
                    &format!("Failed to process query: {}", e),
                    Some("AI query processing")
                );
                if self.verbose {
                    self.ui.print_info(&format!("Error details: {:?}", e));
                }
            }
        }

        Ok(())
    }

    async fn start_interactive_mode_with_engine(&mut self, conversation_engine: &mut ConversationEngine) -> Result<()> {
        // Ensure any previous output is flushed before starting interactive mode
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        
        loop {
            // Prompt for input with better error handling
            print!("\n{}> ", "loregrep");
            if let Err(e) = std::io::stdout().flush() {
                self.ui.print_error(&format!("Failed to flush stdout: {}", e));
                return Err(anyhow::anyhow!("Interactive mode failed: stdout flush error"));
            }

            // Read user input with better error handling
            let mut input = String::new();
            match std::io::stdin().read_line(&mut input) {
                Ok(_) => {},
                Err(e) => {
                    self.ui.print_error(&format!("Failed to read input: {}", e));
                    continue;
                }
            }

            let input = input.trim();
            
            // Handle special commands
            match input {
                "exit" | "quit" | "q" => {
                    self.ui.print_success("Goodbye! 👋");
                    break;
                }
                "clear" | "reset" => {
                    conversation_engine.clear_conversation();
                    self.ui.print_info("Conversation history cleared.");
                    continue;
                }
                "help" | "h" => {
                    self.print_help_interactive();
                    continue;
                }
                "status" => {
                    self.print_status(conversation_engine);
                    continue;
                }
                "scan" | "scan ." => {
                    self.ui.print_info("Scanning current directory...");
                    let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                    
                    // Create a simple ScanArgs for the existing scan method
                    let scan_args = ScanArgs {
                        path: current_dir,
                        include: vec![],
                        exclude: vec![],
                        follow_symlinks: false,
                        cache: true,
                    };
                    
                    // Use the existing scan method
                    match self.scan(scan_args).await {
                        Ok(()) => {
                            self.ui.print_success(&format!("Scan completed! Found {} files", self.repo_map.lock().unwrap().file_count()));
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Scan failed: {}", e));
                        }
                    }
                    continue;
                }
                "" => continue,
                _ => {}
            }

            // Process AI query
            if let Err(e) = self.process_ai_query_with_engine(conversation_engine, input).await {
                self.ui.print_error(&format!("Error: {}", e));
            }
        }

        Ok(())
    }

    fn print_help_interactive(&self) {
        self.ui.print_header("Interactive Commands");
        self.ui.print_info("Available commands:");
        self.ui.print_info("  help, h          - Show this help message");
        self.ui.print_info("  scan, scan .     - Scan current directory for files");
        self.ui.print_info("  status           - Show AI engine status");
        self.ui.print_info("  clear, reset     - Clear conversation history");
        self.ui.print_info("  exit, quit, q    - Exit interactive mode");
        self.ui.print_info("");
        self.ui.print_info("Or ask any question about your code:");
        self.ui.print_info("  > What functions handle configuration?");
        self.ui.print_info("  > Show me all public structs");
        self.ui.print_info("  > How does error handling work?");
    }
    */

    // Helper methods for public API conversion
    fn print_public_scan_results(&self, scan_result: &PublicScanResult) {
        self.ui.print_info(&format!("Files scanned: {}", scan_result.files_scanned));
        self.ui.print_info(&format!("Functions found: {}", scan_result.functions_found));
        self.ui.print_info(&format!("Structs found: {}", scan_result.structs_found));
        self.ui.print_info(&format!("Duration: {}ms", scan_result.duration_ms));
        
        if !scan_result.languages.is_empty() {
            self.ui.print_info(&format!("Languages: {:?}", scan_result.languages));
        }
    }
    
    fn convert_tool_result_to_search_results(&self, data: serde_json::Value, result_type: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        
        // Handle the tool result data based on type
        if let Some(items) = data.as_array() {
            for item in items {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    let file_path = item.get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    
                    let line_number = item.get("line_number")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32);
                    
                    let signature = match result_type {
                        "function" => {
                            let params = item.get("parameters")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            let return_type = item.get("return_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if return_type.is_empty() {
                                format!("fn {}(...) [{}params]", name, params)
                            } else {
                                format!("fn {}(...) -> {} [{}params]", name, return_type, params)
                            }
                        },
                        "struct" => {
                            let fields = item.get("fields")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            format!("struct {} {{ {}fields }}", name, fields)
                        },
                        _ => name.to_string()
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
            self.ui.print_info(&format!("File: {}", file_path));
        }
        
        if let Some(language) = data.get("language").and_then(|v| v.as_str()) {
            self.ui.print_info(&format!("Language: {}", language));
        }
        
        // Display functions
        if args.functions || (!args.structs && !args.imports) {
            if let Some(functions) = data.get("functions").and_then(|v| v.as_array()) {
                if !functions.is_empty() {
                    self.ui.print_header("Functions");
                    for func in functions {
                        if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                            let params = func.get("parameters")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.len())
                                .unwrap_or(0);
                            let return_type = func.get("return_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            
                            println!("  fn {}({} params) -> {}", name, params, 
                                if return_type.is_empty() { "()" } else { return_type });
                        }
                    }
                }
            }
        }
        
        // Display structs
        if args.structs || (!args.functions && !args.imports) {
            if let Some(structs) = data.get("structs").and_then(|v| v.as_array()) {
                if !structs.is_empty() {
                    self.ui.print_header("Structs");
                    for struct_item in structs {
                        if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                            let fields = struct_item.get("fields")
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
            println!("📁 {}", file_path);
            
            if let Some(functions) = data.get("functions").and_then(|v| v.as_array()) {
                for func in functions {
                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                        println!("  ├── fn {}", name);
                    }
                }
            }
            
            if let Some(structs) = data.get("structs").and_then(|v| v.as_array()) {
                for struct_item in structs {
                    if let Some(name) = struct_item.get("name").and_then(|v| v.as_str()) {
                        println!("  └── struct {}", name);
                    }
                }
            }
        }
    }

    // Helper methods

    // Commented out during refactoring - now uses public API
    // async fn analyze_file_internal(&self, file_path: &Path) -> Result<TreeNode> { ... }

    async fn save_cache(&self, _root_path: &Path) -> Result<()> {
        // TODO: Implement cache saving
        // For now, this is a placeholder
        if self.verbose {
            self.ui.print_info("Cache saving not yet implemented");
        }
        Ok(())
    }

    // Commented out during refactoring - now uses public API
    // fn print_scan_results(&self, result: &ScanResult) { ... }

    // Convert methods for search results
    fn convert_function_results(&self, functions: Vec<&FunctionSignature>) -> Vec<SearchResult> {
        functions.into_iter().map(|func| {
            let signature = self.ui.formatter.format_function_signature(
                &func.name,
                &func.parameters.iter().map(|p| format!("{}: {}", p.name, p.param_type)).collect::<Vec<_>>(),
                func.return_type.as_deref()
            );
            
            SearchResult::new(
                "function".to_string(),
                signature,
                "unknown".to_string(), // TODO: Add file_path to FunctionSignature
                Some(func.start_line),
            ).with_context(format!("Lines: {}-{}", func.start_line, func.end_line))
        }).collect()
    }

    fn convert_struct_results(&self, structs: Vec<&StructSignature>) -> Vec<SearchResult> {
        structs.into_iter().map(|s| {
            let field_names: Vec<String> = s.fields.iter().map(|f| f.name.clone()).collect();
            let signature = self.ui.formatter.format_struct_signature(&s.name, &field_names);
            
            SearchResult::new(
                "struct".to_string(),
                signature,
                "unknown".to_string(), // TODO: Add file_path to StructSignature
                Some(s.start_line),
            ).with_context(format!("Lines: {}-{}, {} fields", s.start_line, s.end_line, s.fields.len()))
        }).collect()
    }

    fn convert_import_results(&self, imports: Vec<&ImportStatement>) -> Vec<SearchResult> {
        imports.into_iter().map(|import| {
            SearchResult::new(
                "import".to_string(),
                format!("use {}", import.module_path),
                "unknown".to_string(), // TODO: Add file_path to ImportStatement
                Some(import.line_number),
            )
        }).collect()
    }

    fn convert_export_results(&self, exports: Vec<&ExportStatement>) -> Vec<SearchResult> {
        exports.into_iter().map(|export| {
            SearchResult::new(
                "export".to_string(),
                format!("pub {}", export.exported_item),
                "unknown".to_string(), // TODO: Add file_path to ExportStatement
                Some(export.line_number),
            )
        }).collect()
    }

    // Commented out during refactoring - now uses public API
    // fn display_analysis_text(&self, analysis: &TreeNode, args: &AnalyzeArgs) { ... }

    // Commented out during refactoring - now uses public API
    // fn display_analysis_tree(&self, analysis: &TreeNode) { ... }

    // Utility methods for consistent output formatting
    fn print_header(&self, title: &str) {
        self.ui.print_header(title);
    }

    fn print_success(&self, message: &str) {
        self.ui.print_success(message);
    }

    fn print_info(&self, message: &str) {
        self.ui.print_info(message);
    }

    fn print_warning(&self, message: &str) {
        self.ui.print_warning(message);
    }

    fn print_error(&self, message: &str) {
        self.ui.print_error(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
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
        let result = app.loregrep.execute_tool("analyze_file", serde_json::json!({
            "file_path": file_path.to_string_lossy(),
            "include_source": false
        })).await;
        
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
    async fn test_query_without_api_key() {
        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();
        
        let args = QueryArgs {
            query: Some("test query".to_string()),
            path: PathBuf::from("."),
            interactive: false,
        };
        
        // Should not panic, should handle gracefully
        let result = app.query(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_query_disabled_gracefully() {
        let config = create_test_config();
        let mut app = CliApp::new(config, false, false).await.unwrap();
        
        let args = QueryArgs {
            query: Some("What functions are available?".to_string()),
            path: PathBuf::from("."),
            interactive: false,
        };
        
        // Should handle gracefully with informative message
        let result = app.query(args).await;
        assert!(result.is_ok());
    }

    #[test]
    async fn test_convert_function_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();
        
        let func = FunctionSignature::new("test_func".to_string())
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
        
        let struct_def = StructSignature::new("TestStruct".to_string())
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
        
        let import = ImportStatement::new("std::collections::HashMap".to_string())
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
        
        let export = ExportStatement::new("MyFunction".to_string())
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
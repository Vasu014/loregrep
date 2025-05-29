use anyhow::{Context, Result};
use colored::*;
use serde_json;
use std::path::Path;
use std::time::Instant;
use tokio::fs;
use tracing::info;

// Use crate imports since we're within the same crate
use crate::{
    CliConfig,
    cli_types::{AnalyzeArgs, QueryArgs, ScanArgs, SearchArgs},
    scanner::{RepositoryScanner, ScanConfig, ScanResult},
    storage::memory::RepoMap,
    analyzers::{rust::RustAnalyzer, LanguageAnalyzer},
    types::{
        analysis::TreeNode,
        function::FunctionSignature,
        struct_def::{StructSignature, ImportStatement, ExportStatement},
    },
    conversation::ConversationEngine,
    ai_tools::LocalAnalysisTools,
};

pub struct CliApp {
    config: CliConfig,
    repo_scanner: RepositoryScanner,
    repo_map: RepoMap,
    rust_analyzer: RustAnalyzer,
    conversation_engine: Option<ConversationEngine>,
    verbose: bool,
    colors_enabled: bool,
}

impl CliApp {
    pub async fn new(config: CliConfig, verbose: bool, colors_enabled: bool) -> Result<Self> {
        info!("Initializing Loregrep CLI");

        // Initialize components
        let scan_config = ScanConfig {
            follow_symlinks: config.file_scanning.follow_symlinks,
            max_depth: config.file_scanning.max_depth,
            show_progress: !config.output.verbose,
            parallel: true,
        };

        let repo_scanner = RepositoryScanner::new(&config.file_scanning, Some(scan_config))
            .context("Failed to create repository scanner")?;

        let repo_map = RepoMap::new();
        let rust_analyzer = RustAnalyzer::new()
            .context("Failed to create Rust analyzer")?;

        // Initialize conversation engine if API key is available
        let conversation_engine = if config.ai.api_key.is_some() {
            let repo_map_arc = std::sync::Arc::new(repo_map.clone());
            
            // Create new instances for the tools (since they don't support cloning)
            let tools_scan_config = ScanConfig {
                follow_symlinks: config.file_scanning.follow_symlinks,
                max_depth: config.file_scanning.max_depth,
                show_progress: !config.output.verbose,
                parallel: true,
            };
            let tools_scanner = RepositoryScanner::new(&config.file_scanning, Some(tools_scan_config))
                .context("Failed to create tools scanner")?;
            let tools_analyzer = RustAnalyzer::new()
                .context("Failed to create tools analyzer")?;
            
            let local_tools = LocalAnalysisTools::new(
                repo_map_arc,
                tools_scanner,
                tools_analyzer,
            );
            
            match ConversationEngine::from_config_and_tools(&config, local_tools) {
                Ok(engine) => {
                    if verbose {
                        println!("{}", "✅ AI conversation engine initialized".green());
                    }
                    Some(engine)
                }
                Err(e) => {
                    if verbose {
                        println!("{}", format!("⚠️  Failed to initialize AI engine: {}", e).yellow());
                    }
                    None
                }
            }
        } else {
            None
        };

        // Create cache directory if it doesn't exist
        if config.cache.enabled {
            if let Some(parent) = config.cache.path.parent() {
                tokio::fs::create_dir_all(parent).await
                    .context("Failed to create cache directory")?;
            }
        }

        Ok(Self {
            config,
            repo_scanner,
            repo_map,
            rust_analyzer,
            conversation_engine,
            verbose,
            colors_enabled,
        })
    }

    pub async fn scan(&mut self, args: ScanArgs) -> Result<()> {
        let start_time = Instant::now();
        
        self.print_header("Repository Scan");
        
        if self.verbose {
            println!("Scanning directory: {}", args.path.display().to_string().cyan());
            println!("Include patterns: {:?}", self.config.file_scanning.include_patterns);
            println!("Exclude patterns: {:?}", self.config.file_scanning.exclude_patterns);
        }

        // Perform the scan
        let scan_result = self.repo_scanner.scan(&args.path)
            .with_context(|| format!("Failed to scan directory: {:?}", args.path))?;

        // Display results
        self.print_scan_results(&scan_result);

        // Analyze discovered files if requested
        if !scan_result.files.is_empty() {
            self.print_info("Starting file analysis...");
            
            let analysis_start = Instant::now();
            
            for file in &scan_result.files {
                if file.language == "rust" {
                    match self.analyze_file_internal(&file.path).await {
                        Ok(analysis) => {
                            if let Err(e) = self.repo_map.add_file(analysis) {
                                self.print_warning(&format!(
                                    "Failed to add {} to repository map: {}",
                                    file.relative_path.display(),
                                    e
                                ));
                            }
                        }
                        Err(e) => {
                            self.print_warning(&format!(
                                "Failed to analyze {}: {}",
                                file.relative_path.display(),
                                e
                            ));
                        }
                    }
                }
            }

            let analysis_duration = analysis_start.elapsed();
            self.print_success(&format!(
                "Analysis completed in {:?}. Repository map contains {} files",
                analysis_duration,
                self.repo_map.file_count()
            ));
        }

        // Cache results if enabled
        if args.cache && self.config.cache.enabled {
            self.save_cache(&args.path).await?;
        }

        let total_duration = start_time.elapsed();
        self.print_success(&format!("Total scan time: {:?}", total_duration));

        Ok(())
    }

    pub async fn search(&self, args: SearchArgs) -> Result<()> {
        self.print_header("Search");

        if self.repo_map.is_empty() {
            self.print_warning("Repository map is empty. Run 'scan' first to populate data.");
            return Ok(());
        }

        let start_time = Instant::now();
        
        if self.verbose {
            println!("Query: {}", args.query.green());
            println!("Search type: {}", args.r#type.cyan());
            println!("Fuzzy matching: {}", if args.fuzzy { "enabled".green() } else { "disabled".red() });
        }

        // Perform search based on type
        let results = match args.r#type.as_str() {
            "function" | "func" => {
                let functions = self.repo_map.find_functions_with_options(&args.query, args.limit, args.fuzzy);
                self.format_function_results(functions)
            },
            "struct" => {
                let structs = self.repo_map.find_structs_with_options(&args.query, args.limit, args.fuzzy);
                self.format_struct_results(structs)
            },
            "import" => {
                let imports = self.repo_map.find_imports(&args.query, args.limit);
                self.format_import_results(imports)
            },
            "export" => {
                let exports = self.repo_map.find_exports(&args.query, args.limit);
                self.format_export_results(exports)
            },
            "all" => {
                // Search across all types
                let mut all_results = Vec::new();
                
                let functions = self.repo_map.find_functions_with_options(&args.query, args.limit / 4, args.fuzzy);
                all_results.extend(self.format_function_results(functions));
                
                let structs = self.repo_map.find_structs_with_options(&args.query, args.limit / 4, args.fuzzy);
                all_results.extend(self.format_struct_results(structs));
                
                all_results
            },
            _ => {
                self.print_error(&format!("Unknown search type: {}", args.r#type));
                return Ok(());
            }
        };

        let search_duration = start_time.elapsed();

        // Display results
        if results.is_empty() {
            self.print_warning(&format!("No results found for query: {}", args.query));
        } else {
            self.print_success(&format!("Found {} results in {:?}", results.len(), search_duration));
            println!();
            
            for (i, result) in results.iter().enumerate() {
                if i >= args.limit {
                    break;
                }
                println!("{}", result);
                if i < results.len() - 1 {
                    println!();
                }
            }
        }

        Ok(())
    }

    pub async fn analyze(&mut self, args: AnalyzeArgs) -> Result<()> {
        self.print_header("File Analysis");

        if !args.file.exists() {
            self.print_error(&format!("File not found: {}", args.file.display()));
            return Ok(());
        }

        if self.verbose {
            println!("Analyzing file: {}", args.file.display().to_string().cyan());
            println!("Output format: {}", args.format.cyan());
        }

        let start_time = Instant::now();
        let analysis = self.analyze_file_internal(&args.file).await?;
        let analysis_duration = start_time.elapsed();

        // Display results based on format
        match args.format.as_str() {
            "json" => {
                let json = serde_json::to_string_pretty(&analysis)
                    .context("Failed to serialize analysis to JSON")?;
                println!("{}", json);
            },
            "text" => {
                self.display_analysis_text(&analysis, &args);
            },
            "tree" => {
                self.display_analysis_tree(&analysis);
            },
            _ => {
                self.print_error(&format!("Unknown output format: {}", args.format));
                return Ok(());
            }
        }

        if self.verbose {
            println!("\n{}", format!("Analysis completed in {:?}", analysis_duration).green());
        }

        Ok(())
    }

    pub async fn show_config(&self) -> Result<()> {
        self.print_header("Configuration");

        let config_json = serde_json::to_string_pretty(&self.config)
            .context("Failed to serialize configuration")?;
        
        println!("{}", config_json);
        
        // Show cache status
        println!("\n{}", "Cache Status:".bold());
        if self.config.cache.enabled {
            println!("  Status: {}", "Enabled".green());
            println!("  Path: {}", self.config.cache.path.display().to_string().cyan());
            
            if self.config.cache.path.exists() {
                if let Ok(metadata) = std::fs::metadata(&self.config.cache.path) {
                    let size_mb = metadata.len() / (1024 * 1024);
                    println!("  Size: {} MB", size_mb.to_string().yellow());
                }
            } else {
                println!("  Size: {} (no cache file found)", "0 MB".yellow());
            }
        } else {
            println!("  Status: {}", "Disabled".red());
        }

        // Show repository map status
        println!("\n{}", "Repository Map Status:".bold());
        println!("  Files loaded: {}", self.repo_map.file_count().to_string().cyan());
        println!("  Memory usage: {} MB", (self.repo_map.memory_usage() / (1024 * 1024)).to_string().yellow());

        Ok(())
    }

    pub async fn query(&mut self, args: QueryArgs) -> Result<()> {
        self.print_header("AI Query Mode");
        
        // Early return checks to avoid borrow conflicts
        if self.conversation_engine.is_none() {
            if self.config.ai.api_key.is_none() {
                self.print_error("No API key configured. Set ANTHROPIC_API_KEY environment variable or add it to your config file.");
                self.print_info("You can get an API key from https://console.anthropic.com/");
            } else {
                self.print_error("AI conversation engine is not available.");
            }
            return Ok(());
        }

        // Show repository status
        if self.repo_map.is_empty() {
            self.print_warning("Repository map is empty. Consider running 'scan' first for better context.");
            self.print_info(&format!("Current directory: {}", args.path.display()));
        } else {
            if self.verbose {
                self.print_info(&format!("Repository contains {} analyzed files", self.repo_map.file_count()));
            }
        }

        // Take ownership of the conversation engine temporarily
        let mut conversation_engine = self.conversation_engine.take().unwrap();
        
        let result = match args.query {
            Some(query) => {
                // Single query mode
                self.process_ai_query_with_engine(&mut conversation_engine, &query).await
            }
            None => {
                // Interactive mode
                self.start_interactive_mode_with_engine(&mut conversation_engine).await
            }
        };

        // Put the conversation engine back
        self.conversation_engine = Some(conversation_engine);
        
        result
    }

    async fn process_ai_query_with_engine(&self, conversation_engine: &mut ConversationEngine, query: &str) -> Result<()> {
        if self.verbose {
            println!("Query: {}", query.cyan());
        }

        let start_time = Instant::now();
        
        // Show thinking indicator
        print!("🤔 Thinking");
        for _ in 0..3 {
            print!(".");
            use std::io::Write;
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        }
        println!();

        // Process the query
        match conversation_engine.process_user_message(query).await {
            Ok(response) => {
                let duration = start_time.elapsed();
                println!("\n{}", "AI Response:".bold().green());
                println!("{}", response);
                
                if self.verbose {
                    println!("\n{}", format!("Response time: {:?}", duration).dimmed());
                    println!("{}", format!("Conversation messages: {}", conversation_engine.get_message_count()).dimmed());
                }
            }
            Err(e) => {
                println!("{} {}", "✗".red(), format!("Failed to process query: {}", e));
                if self.verbose {
                    println!("Error details: {:?}", e);
                }
            }
        }

        Ok(())
    }

    async fn start_interactive_mode_with_engine(&self, conversation_engine: &mut ConversationEngine) -> Result<()> {
        println!("{} {}", "ℹ".blue(), "Starting interactive AI mode. Type 'help' for commands, 'exit' to quit.");
        
        loop {
            // Prompt for input
            print!("\n{}> ", "loregrep".cyan().bold());
            use std::io::Write;
            std::io::Write::flush(&mut std::io::stdout()).unwrap();

            // Read user input
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input).is_err() {
                println!("{} {}", "✗".red(), "Failed to read input");
                continue;
            }

            let input = input.trim();
            
            // Handle special commands
            match input {
                "exit" | "quit" | "q" => {
                    println!("{} {}", "ℹ".blue(), "Goodbye! 👋");
                    break;
                }
                "clear" | "reset" => {
                    conversation_engine.clear_conversation();
                    println!("{} {}", "ℹ".blue(), "Conversation history cleared.");
                    continue;
                }
                "help" | "h" => {
                    Self::print_help_interactive();
                    continue;
                }
                "status" => {
                    Self::print_status_static(conversation_engine, &self.repo_map, &self.config);
                    continue;
                }
                "" => continue,
                _ => {}
            }

            // Process AI query
            if let Err(e) = self.process_ai_query_with_engine(conversation_engine, input).await {
                println!("{} {}", "✗".red(), format!("Error: {}", e));
            }
        }

        Ok(())
    }

    fn print_help_interactive() {
        println!("\n{}", "Interactive AI Commands:".bold());
        println!("  {}  - Get help with this menu", "help, h".cyan());
        println!("  {}  - Clear conversation history", "clear, reset".cyan());
        println!("  {}  - Show conversation status", "status".cyan());
        println!("  {}  - Exit interactive mode", "exit, quit, q".cyan());
        println!("\n{}", "Example queries:".bold());
        println!("  {}  - What functions handle authentication?", "\"".dimmed());
        println!("  {}  - Show me all public structs", "\"".dimmed());
        println!("  {}  - Find functions that call parse_config", "\"".dimmed());
        println!("  {}  - What would break if I change this function?", "\"".dimmed());
        println!("  {}  - Analyze the dependencies in main.rs", "\"".dimmed());
    }

    fn print_status_static(conversation_engine: &ConversationEngine, repo_map: &RepoMap, config: &CliConfig) {
        println!("\n{}", "AI Status:".bold());
        println!("  API Key: {}", if conversation_engine.has_api_key() { "✅ Available".green() } else { "❌ Missing".red() });
        println!("  Repository: {} files analyzed", repo_map.file_count().to_string().cyan());
        println!("  Conversation: {} messages", conversation_engine.get_message_count().to_string().cyan());
        println!("  Model: {}", config.ai.model.cyan());
        println!("{}", conversation_engine.get_conversation_summary().dimmed());
    }

    // Helper methods

    async fn analyze_file_internal(&self, file_path: &Path) -> Result<TreeNode> {
        let content = fs::read_to_string(file_path).await
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        let language = self.repo_scanner.detect_file_language(file_path);
        
        match language.as_str() {
            "rust" => {
                let file_analysis = self.rust_analyzer.analyze_file(&content, &file_path.to_string_lossy()).await
                    .context("Failed to analyze Rust file")?;
                Ok(file_analysis.tree_node)
            },
            _ => {
                anyhow::bail!("Unsupported language: {}", language);
            }
        }
    }

    async fn save_cache(&self, _root_path: &Path) -> Result<()> {
        if !self.config.cache.enabled {
            return Ok(());
        }

        info!("Saving repository map to cache");
        
        // TODO: Implement actual caching once persistence layer is ready
        if self.verbose {
            self.print_info("Cache saving not yet implemented");
        }
        
        Ok(())
    }

    fn print_scan_results(&self, result: &ScanResult) {
        println!("\n{}", "Scan Results:".bold());
        
        println!("  Total files found: {}", result.total_files_found.to_string().cyan());
        println!("  Files after filtering: {}", result.files.len().to_string().green());
        println!("  Files filtered out: {}", result.total_files_filtered.to_string().yellow());
        println!("  Scan duration: {:?}", result.scan_duration);
        
        if !result.languages_found.is_empty() {
            println!("\n{}", "Languages found:".bold());
            for (language, count) in &result.languages_found {
                println!("  {}: {}", language.cyan(), count.to_string().green());
            }
        }
    }

    fn format_function_results(&self, functions: Vec<&FunctionSignature>) -> Vec<String> {
        functions.into_iter().map(|func| {
            format!(
                "{}fn {}{}\n  Lines: {}-{}",
                if func.is_public { "pub " } else { "" }.green(),
                func.name.cyan().bold(),
                if func.parameters.is_empty() { 
                    "()".to_string() 
                } else { 
                    format!("({})", func.parameters.len()) 
                },
                func.start_line, func.end_line
            )
        }).collect()
    }

    fn format_struct_results(&self, structs: Vec<&StructSignature>) -> Vec<String> {
        structs.into_iter().map(|s| {
            format!(
                "{}struct {}\n  Lines: {}-{}",
                if s.is_public { "pub " } else { "" }.green(),
                s.name.cyan().bold(),
                s.start_line, s.end_line
            )
        }).collect()
    }

    fn format_import_results(&self, imports: Vec<&ImportStatement>) -> Vec<String> {
        imports.into_iter().map(|import| {
            format!(
                "use {}\n  Line: {}",
                import.module_path.cyan(),
                import.line_number.to_string().dimmed()
            )
        }).collect()
    }

    fn format_export_results(&self, exports: Vec<&ExportStatement>) -> Vec<String> {
        exports.into_iter().map(|export| {
            format!(
                "pub {}\n  Line: {}",
                export.exported_item.cyan(),
                export.line_number.to_string().dimmed()
            )
        }).collect()
    }

    fn display_analysis_text(&self, analysis: &TreeNode, args: &AnalyzeArgs) {
        println!("{}", format!("File: {}", analysis.file_path).bold());
        println!("Language: {}", analysis.language.cyan());
        println!("Functions: {}", analysis.functions.len().to_string().green());
        println!("Structs: {}", analysis.structs.len().to_string().green());
        println!("Imports: {}", analysis.imports.len().to_string().green());
        println!("Exports: {}", analysis.exports.len().to_string().green());
        
        if args.functions || (!args.structs && !args.imports) {
            if !analysis.functions.is_empty() {
                println!("\n{}", "Functions:".bold());
                for func in &analysis.functions {
                    println!("  {}fn {}", 
                        if func.is_public { "pub " } else { "" }.green(),
                        func.name.cyan()
                    );
                    if self.verbose && !func.parameters.is_empty() {
                        println!("    Parameters: {}", func.parameters.len().to_string().yellow());
                    }
                    println!("    Lines: {}-{}", func.start_line, func.end_line);
                }
            }
        }

        if args.structs || (!args.functions && !args.imports) {
            if !analysis.structs.is_empty() {
                println!("\n{}", "Structs:".bold());
                for s in &analysis.structs {
                    println!("  {}struct {}", 
                        if s.is_public { "pub " } else { "" }.green(),
                        s.name.cyan()
                    );
                    if self.verbose && !s.fields.is_empty() {
                        println!("    Fields: {}", s.fields.len().to_string().yellow());
                    }
                    println!("    Lines: {}-{}", s.start_line, s.end_line);
                }
            }
        }

        if args.imports || (!args.functions && !args.structs) {
            if !analysis.imports.is_empty() {
                println!("\n{}", "Imports:".bold());
                for import in &analysis.imports {
                    println!("  use {}", import.module_path.cyan());
                }
            }

            if !analysis.exports.is_empty() {
                println!("\n{}", "Exports:".bold());
                for export in &analysis.exports {
                    println!("  pub {}", export.exported_item.cyan());
                }
            }
        }
    }

    fn display_analysis_tree(&self, analysis: &TreeNode) {
        println!("{}", format!("📁 {}", analysis.file_path).bold());
        
        for func in &analysis.functions {
            println!("├── 🔧 fn {}", func.name.cyan());
        }
        
        for s in &analysis.structs {
            println!("├── 📦 struct {}", s.name.cyan());
        }
        
        if !analysis.imports.is_empty() {
            println!("└── 📥 {} imports", analysis.imports.len().to_string().yellow());
        }
    }

    // Utility methods for consistent output formatting
    fn print_header(&self, title: &str) {
        if self.colors_enabled {
            println!("\n{}", format!("=== {} ===", title).bold().cyan());
        } else {
            println!("\n=== {} ===", title);
        }
    }

    fn print_success(&self, message: &str) {
        if self.colors_enabled {
            println!("{} {}", "✓".green(), message);
        } else {
            println!("✓ {}", message);
        }
    }

    fn print_info(&self, message: &str) {
        if self.colors_enabled {
            println!("{} {}", "ℹ".blue(), message);
        } else {
            println!("ℹ {}", message);
        }
    }

    fn print_warning(&self, message: &str) {
        if self.colors_enabled {
            eprintln!("{} {}", "⚠".yellow(), message);
        } else {
            eprintln!("⚠ {}", message);
        }
    }

    fn print_error(&self, message: &str) {
        if self.colors_enabled {
            eprintln!("{} {}", "✗".red(), message);
        } else {
            eprintln!("✗ {}", message);
        }
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
        
        let result = app.analyze_file_internal(&file_path).await;
        assert!(result.is_ok());
        
        let analysis = result.unwrap();
        assert_eq!(analysis.language, "rust");
        assert_eq!(analysis.functions.len(), 1);
        assert_eq!(analysis.structs.len(), 1);
        assert_eq!(analysis.imports.len(), 1);
        
        // Check function details
        let func = &analysis.functions[0];
        assert_eq!(func.name, "hello_world");
        assert!(func.is_public);
        
        // Check struct details
        let struct_def = &analysis.structs[0];
        assert_eq!(struct_def.name, "TestStruct");
        assert!(struct_def.is_public);
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
        
        // Check that files were added to repo map
        assert!(app.repo_map.file_count() > 0);
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
    async fn test_ai_query_with_engine() {
        let mut config = create_test_config();
        config.ai.api_key = Some("test-api-key".to_string());
        
        let mut app = CliApp::new(config, false, false).await.unwrap();
        
        // Conversation engine should be initialized
        assert!(app.conversation_engine.is_some());
        
        let args = QueryArgs {
            query: Some("What functions are available?".to_string()),
            path: PathBuf::from("."),
            interactive: false,
        };
        
        // This will fail due to no real API, but should handle the error gracefully
        let result = app.query(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_conversation_engine_initialization() {
        let mut config = create_test_config();
        config.ai.api_key = Some("test-key".to_string());
        config.ai.model = "claude-3-5-sonnet-20241022".to_string();
        
        let app = CliApp::new(config, true, true).await.unwrap();
        
        // Should have conversation engine
        assert!(app.conversation_engine.is_some());
        
        if let Some(engine) = &app.conversation_engine {
            assert!(engine.has_api_key());
            assert_eq!(engine.get_message_count(), 0);
        }
    }

    #[tokio::test]
    async fn test_conversation_engine_without_api_key() {
        let config = create_test_config(); // No API key by default
        
        let app = CliApp::new(config, false, false).await.unwrap();
        
        // Should not have conversation engine
        assert!(app.conversation_engine.is_none());
    }

    #[tokio::test]
    async fn test_ai_status_display() {
        let mut config = create_test_config();
        config.ai.api_key = Some("test-key".to_string());
        
        let app = CliApp::new(config, false, false).await.unwrap();
        
        if let Some(engine) = &app.conversation_engine {
            // This should not panic
            app.print_status(engine);
        }
    }

    #[tokio::test]
    async fn test_interactive_commands() {
        let app = CliApp::new(create_test_config(), false, false).await.unwrap();
        
        // These should not panic
        app.print_help_interactive();
    }

    #[test]
    async fn test_format_function_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();
        
        let func = FunctionSignature::new("test_func".to_string())
            .with_visibility(true)
            .with_location(10, 20);
        
        let results = app.format_function_results(vec![&func]);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("test_func"));
        assert!(results[0].contains("10-20"));
    }

    #[test]
    async fn test_format_struct_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();
        
        let struct_def = StructSignature::new("TestStruct".to_string())
            .with_visibility(true)
            .with_location(5, 15);
        
        let results = app.format_struct_results(vec![&struct_def]);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("TestStruct"));
        assert!(results[0].contains("5-15"));
    }

    #[test]
    async fn test_format_import_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();
        
        let import = ImportStatement::new("std::collections::HashMap".to_string())
            .with_line_number(1);
        
        let results = app.format_import_results(vec![&import]);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("std::collections::HashMap"));
        assert!(results[0].contains("1"));
    }

    #[test]
    async fn test_format_export_results() {
        let config = create_test_config();
        let app = CliApp::new(config, false, true).await.unwrap();
        
        let export = ExportStatement::new("MyFunction".to_string())
            .with_line_number(10);
        
        let results = app.format_export_results(vec![&export]);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("MyFunction"));
        assert!(results[0].contains("10"));
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
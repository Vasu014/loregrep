use anyhow::{Context, Result};
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
    ui::{UIManager, ThemeType, formatter::SearchResult},
};

pub struct CliApp {
    config: CliConfig,
    repo_scanner: RepositoryScanner,
    repo_map: RepoMap,
    rust_analyzer: RustAnalyzer,
    conversation_engine: Option<ConversationEngine>,
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
                        ui.print_success("AI conversation engine initialized");
                    }
                    Some(engine)
                }
                Err(e) => {
                    if verbose {
                        ui.print_warning(&format!("Failed to initialize AI engine: {}", e));
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
            ui,
        })
    }

    pub async fn scan(&mut self, args: ScanArgs) -> Result<()> {
        let start_time = Instant::now();
        
        self.ui.print_header("Repository Scan");
        
        if self.verbose {
            self.ui.print_info(&format!("Scanning directory: {}", args.path.display()));
            self.ui.print_info(&format!("Include patterns: {:?}", self.config.file_scanning.include_patterns));
            self.ui.print_info(&format!("Exclude patterns: {:?}", self.config.file_scanning.exclude_patterns));
        }

        // Perform the scan with progress indicator
        let scan_result = if args.path.is_dir() {
            // Get estimated file count for progress bar
            let estimated_files = std::fs::read_dir(&args.path)
                .map(|entries| entries.count() as u64)
                .unwrap_or(100);
            
            let progress = self.ui.create_scan_progress(estimated_files);
            progress.set_message("Discovering files...");
            
            let result = self.repo_scanner.scan(&args.path)
                .with_context(|| format!("Failed to scan directory: {:?}", args.path))?;
            
            progress.finish_with_message(&format!("Discovered {} files", result.files.len()));
            result
        } else {
            self.repo_scanner.scan(&args.path)
                .with_context(|| format!("Failed to scan path: {:?}", args.path))?
        };

        // Display results
        self.print_scan_results(&scan_result);

        // Analyze discovered files if requested
        if !scan_result.files.is_empty() {
            self.ui.print_info("Starting file analysis...");
            
            let analysis_start = Instant::now();
            let progress = self.ui.progress.create_analysis_progress(scan_result.files.len() as u64);
            
            for file in &scan_result.files {
                if file.language == "rust" {
                    progress.set_current_file(&file.relative_path.display().to_string());
                    
                    match self.analyze_file_internal(&file.path).await {
                        Ok(analysis) => {
                            if let Err(e) = self.repo_map.add_file(analysis) {
                                self.ui.print_warning(&format!(
                                    "Failed to add {} to repository map: {}",
                                    file.relative_path.display(),
                                    e
                                ));
                            }
                        }
                        Err(e) => {
                            self.ui.print_warning(&format!(
                                "Failed to analyze {}: {}",
                                file.relative_path.display(),
                                e
                            ));
                        }
                    }
                    progress.inc();
                }
            }

            let analysis_duration = analysis_start.elapsed();
            progress.finish_with_message("Analysis completed");
            
            let summary = self.ui.formatter.format_analysis_summary(
                scan_result.files.len(),
                self.repo_map.find_functions("").items.len(),
                self.repo_map.find_structs("").items.len(),
                analysis_duration
            );
            println!("{}", summary);
        }

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

        if self.repo_map.is_empty() {
            self.ui.print_warning("Repository map is empty. Run 'scan' first to populate data.");
            return Ok(());
        }

        let start_time = Instant::now();
        
        if self.verbose {
            self.ui.print_info(&format!("Query: {}", args.query));
            self.ui.print_info(&format!("Search type: {}", args.r#type));
            self.ui.print_info(&format!("Fuzzy matching: {}", if args.fuzzy { "enabled" } else { "disabled" }));
        }

        // Perform search based on type
        let results = match args.r#type.as_str() {
            "function" | "func" => {
                let functions = self.repo_map.find_functions_with_options(&args.query, args.limit, args.fuzzy);
                self.convert_function_results(functions)
            },
            "struct" => {
                let structs = self.repo_map.find_structs_with_options(&args.query, args.limit, args.fuzzy);
                self.convert_struct_results(structs)
            },
            "import" => {
                let imports = self.repo_map.find_imports(&args.query, args.limit);
                self.convert_import_results(imports)
            },
            "export" => {
                let exports = self.repo_map.find_exports(&args.query, args.limit);
                self.convert_export_results(exports)
            },
            "all" => {
                let mut all_results = Vec::new();
                
                let functions = self.repo_map.find_functions_with_options(&args.query, args.limit / 4, args.fuzzy);
                all_results.extend(self.convert_function_results(functions));
                
                let structs = self.repo_map.find_structs_with_options(&args.query, args.limit / 4, args.fuzzy);
                all_results.extend(self.convert_struct_results(structs));
                
                all_results
            },
            _ => {
                self.ui.print_error_with_suggestions(&format!("Unknown search type: {}", args.r#type), 
                    Some("Available types: function, struct, import, export, all"));
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

        // Show repository map status
        self.ui.print_info("\nRepository Map Status:");
        self.ui.print_info(&format!("  Files loaded: {}", self.repo_map.file_count().to_string()));
        self.ui.print_info(&format!("  Memory usage: {} MB", (self.repo_map.memory_usage() / (1024 * 1024)).to_string()));

        Ok(())
    }

    pub async fn query(&mut self, args: QueryArgs) -> Result<()> {
        self.ui.print_header("AI Query Mode");
        
        // Friendly welcome message - should be the very first thing users see
        if args.query.is_none() {
            // Only show welcome for interactive mode
            self.ui.print_info("👻 Hey! I'm your friendly ghost in the shell! 👻");
            self.ui.print_info("I'm here to help you explore and understand your codebase.");
            self.ui.print_info("Type 'help' for commands, or just ask me anything about your code!");
            self.ui.print_info("Ready to dig into some code mysteries? Let's go! 🚀");
            println!(); // Add some space
        }
        
        // Early return checks to avoid borrow conflicts
        if self.conversation_engine.is_none() {
            if self.config.ai.api_key.is_none() {
                self.ui.print_error("No API key configured. Set ANTHROPIC_API_KEY environment variable or add it to your config file.");
                self.ui.print_info("You can get an API key from https://console.anthropic.com/");
            } else {
                self.ui.print_error("AI conversation engine is not available.");
            }
            return Ok(());
        }

        // Show repository status and auto-scan if needed
        if self.repo_map.is_empty() {
            self.ui.print_warning("Repository map is empty. Auto-scanning current directory for better context...");
            self.ui.print_info(&format!("Current directory: {}", args.path.display()));
            
            // Auto-scan the current directory
            let scan_args = ScanArgs {
                path: args.path.clone(),
                include: vec![],
                exclude: vec![],
                follow_symlinks: false,
                cache: true,
            };
            
            match self.scan(scan_args).await {
                Ok(()) => {
                    self.ui.print_success(&format!("Auto-scan completed! Found {} files", self.repo_map.file_count()));
                }
                Err(e) => {
                    self.ui.print_warning(&format!("Auto-scan failed: {}. Continuing with empty repository map.", e));
                    self.ui.print_info("You can manually run 'scan .' to try again.");
                }
            }
        } else {
            if self.verbose {
                self.ui.print_info(&format!("Repository contains {} analyzed files", self.repo_map.file_count()));
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
        loop {
            // Prompt for input
            print!("\n{}> ", "loregrep");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();

            // Read user input
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input).is_err() {
                self.ui.print_error("Failed to read input");
                continue;
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
                            self.ui.print_success(&format!("Scan completed! Found {} files", self.repo_map.file_count()));
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

    fn print_status(&self, conversation_engine: &ConversationEngine) {
        self.ui.print_header("AI Status");
        self.ui.print_info(&format!("  API Key: {}", if conversation_engine.has_api_key() { "✅ Available" } else { "❌ Missing" }));
        self.ui.print_info(&format!("  Repository: {} files analyzed", self.repo_map.file_count()));
        self.ui.print_info(&format!("  Conversation: {} messages", conversation_engine.get_message_count()));
        self.ui.print_info(&format!("  Model: {}", self.config.ai.model));
        self.ui.print_info(&conversation_engine.get_conversation_summary());
    }

    // Helper methods

    async fn analyze_file_internal(&self, file_path: &Path) -> Result<TreeNode> {
        let content = fs::read_to_string(file_path).await
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        let language = self.repo_scanner.detect_file_language(file_path);
        
        match language.as_str() {
            "rust" => {
                let file_analysis = self.rust_analyzer.analyze_file(&content, &file_path.to_string_lossy()).await
                    .with_context(|| format!("Failed to analyze Rust file: {:?}", file_path))?;
                Ok(file_analysis.tree_node)
            }
            _ => {
                Err(anyhow::anyhow!("Unsupported language: {}", language))
            }
        }
    }

    async fn save_cache(&self, _root_path: &Path) -> Result<()> {
        // TODO: Implement cache saving
        // For now, this is a placeholder
        if self.verbose {
            self.ui.print_info("Cache saving not yet implemented");
        }
        Ok(())
    }

    fn print_scan_results(&self, result: &ScanResult) {
        self.ui.print_success(&format!("Discovered {} files", result.files.len()));
        
        if self.verbose {
            let mut language_counts = std::collections::HashMap::new();
            for file in &result.files {
                *language_counts.entry(&file.language).or_insert(0) += 1;
            }
            
            for (language, count) in language_counts {
                self.ui.print_info(&format!("  {}: {} files", language, count));
            }
        }
    }

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

    fn display_analysis_text(&self, analysis: &TreeNode, args: &AnalyzeArgs) {
        self.ui.print_info(&format!("File: {}", analysis.file_path));
        self.ui.print_info(&format!("Language: {}", analysis.language));
        self.ui.print_info(&format!("Functions: {}", analysis.functions.len()));
        self.ui.print_info(&format!("Structs: {}", analysis.structs.len()));
        self.ui.print_info(&format!("Imports: {}", analysis.imports.len()));
        self.ui.print_info(&format!("Exports: {}", analysis.exports.len()));
        
        if args.functions || (!args.structs && !args.imports) {
            if !analysis.functions.is_empty() {
                self.ui.print_header("Functions");
                for func in &analysis.functions {
                    let signature = self.ui.formatter.format_function_signature(
                        &func.name,
                        &func.parameters.iter().map(|p| format!("{}: {}", p.name, p.param_type)).collect::<Vec<_>>(),
                        func.return_type.as_deref()
                    );
                    println!("  {}", signature);
                    if self.verbose && !func.parameters.is_empty() {
                        self.ui.print_info(&format!("    Parameters: {}", func.parameters.len()));
                    }
                    self.ui.print_info(&format!("    Lines: {}-{}", func.start_line, func.end_line));
                }
            }
        }

        if args.structs || (!args.functions && !args.imports) {
            if !analysis.structs.is_empty() {
                self.ui.print_header("Structs");
                for s in &analysis.structs {
                    let field_names: Vec<String> = s.fields.iter().map(|f| f.name.clone()).collect();
                    let signature = self.ui.formatter.format_struct_signature(&s.name, &field_names);
                    println!("  {}", signature);
                    if self.verbose && !s.fields.is_empty() {
                        self.ui.print_info(&format!("    Fields: {}", s.fields.len()));
                    }
                    self.ui.print_info(&format!("    Lines: {}-{}", s.start_line, s.end_line));
                }
            }
        }

        if args.imports || (!args.functions && !args.structs) {
            if !analysis.imports.is_empty() {
                self.ui.print_header("Imports");
                for import in &analysis.imports {
                    println!("  use {}", import.module_path);
                }
            }

            if !analysis.exports.is_empty() {
                self.ui.print_header("Exports");
                for export in &analysis.exports {
                    println!("  pub {}", export.exported_item);
                }
            }
        }
    }

    fn display_analysis_tree(&self, analysis: &TreeNode) {
        println!("📁 {}", analysis.file_path);
        
        for func in &analysis.functions {
            println!("├── 🔧 fn {}", func.name);
        }
        
        for s in &analysis.structs {
            println!("├── 📦 struct {}", s.name);
        }
        
        if !analysis.imports.is_empty() {
            println!("└── 📥 {} imports", analysis.imports.len());
        }
    }

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
            // This should not panic - use the instance method
            app.print_status(engine);
        }
    }

    #[tokio::test]
    async fn test_interactive_commands() {
        let app = CliApp::new(create_test_config(), false, false).await.unwrap();
        
        // These should not panic - use the instance method
        app.print_help_interactive();
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
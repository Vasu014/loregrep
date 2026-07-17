use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ScanArgs {
    /// Directory to scan
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Include only these file patterns
    #[arg(short, long)]
    pub include: Vec<String>,

    /// Exclude these file patterns
    #[arg(short, long)]
    pub exclude: Vec<String>,

    /// Follow symbolic links
    #[arg(long)]
    pub follow_symlinks: bool,

    /// Save results to cache
    #[arg(long)]
    pub cache: bool,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Search query (function name, struct name, etc.)
    pub query: String,

    /// Directory to search in
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,

    /// Search type: function, struct, import, export, all
    #[arg(short, long, default_value = "all")]
    pub r#type: String,

    /// Maximum number of results
    #[arg(short, long, default_value = "20")]
    pub limit: usize,

    /// Use fuzzy matching
    #[arg(short, long)]
    pub fuzzy: bool,
}

#[derive(Args)]
pub struct AnalyzeArgs {
    /// File to analyze
    pub file: PathBuf,

    /// Output format: json, text, tree
    #[arg(short, long, default_value = "text")]
    pub format: String,

    /// Show function details
    #[arg(long)]
    pub functions: bool,

    /// Show struct details
    #[arg(long)]
    pub structs: bool,

    /// Show imports/exports
    #[arg(long)]
    pub imports: bool,
}

#[derive(Args)]
pub struct ExecToolArgs {
    /// Tool to execute (search_functions, search_structs, find_callers,
    /// get_dependencies, analyze_file, get_repository_tree)
    pub tool: String,

    /// Tool parameters as a JSON object
    #[arg(long, default_value = "{}")]
    pub params: String,

    /// Directory to scan before executing the tool
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
}

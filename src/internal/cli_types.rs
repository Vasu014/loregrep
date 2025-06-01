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
pub struct QueryArgs {
    /// Natural language query
    pub query: Option<String>,

    /// Directory to analyze
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,

    /// Enter interactive mode
    #[arg(short, long)]
    pub interactive: bool,
} 
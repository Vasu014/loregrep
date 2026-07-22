use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ScanArgs {
    /// Directory to scan (relative paths resolve against --directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    // `--include`, `--exclude` and `--follow-symlinks` used to be accepted here
    // and were then silently discarded — nothing in `src/` ever read them, so a
    // flag that looked like it worked did nothing. Include/exclude patterns and
    // symlink behaviour come from the config file (`file_scanning.*`), which
    // `CliApp::new` feeds to the builder. Removed rather than left parsed: a flag
    // that lies is worse than a flag that is absent.
    /// Save results to cache
    #[arg(long)]
    pub cache: bool,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Search query (function name, struct name, etc.)
    pub query: String,

    /// Directory to search in (relative paths resolve against --directory)
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
    /// File or directory to analyze (relative paths resolve against --directory)
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

    /// Analysis root to scan before executing the tool (relative paths resolve
    /// against --directory). Path parameters inside --params are resolved
    /// against this root, never against the shell's working directory.
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
}

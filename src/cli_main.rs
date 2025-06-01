// Public CLI wrapper for main.rs access
// This provides a clean interface to CLI functionality without exposing internal details

pub use crate::internal::{
    cli::CliApp,
    config::CliConfig,
    cli_types::{AnalyzeArgs, QueryArgs, ScanArgs, SearchArgs},
};
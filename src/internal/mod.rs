pub mod cli;
pub mod cli_types;
pub mod config;
pub mod anthropic;
pub mod ai_tools;
pub mod conversation;
pub mod ui;

// Re-export commonly used internal types for internal usage
pub use cli::CliApp;
pub use cli_types::*;
pub use config::{CliConfig, FileScanningConfig};
pub use anthropic::{AnthropicClient, ConversationContext};
pub use conversation::ConversationEngine;
pub use ai_tools::LocalAnalysisTools;
pub use ui::{UIManager, OutputFormatter, ProgressIndicator, InteractivePrompts, ErrorSuggestions, ColorTheme, ThemeType};
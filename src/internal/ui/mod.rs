pub mod formatter;
pub mod progress;
pub mod prompts;
pub mod themes;
pub mod suggestions;

pub use formatter::OutputFormatter;
pub use progress::{ProgressIndicator, ProgressStyle, ScanProgress};
pub use prompts::{InteractivePrompts, PromptResult, QueryAmbiguity};
pub use themes::{ColorTheme, ThemeType};
pub use suggestions::{ErrorSuggestions, SuggestionType};

use anyhow::Result;
use std::io::{self, Write};

/// Central UI coordinator that manages all user interface components
pub struct UIManager {
    pub formatter: OutputFormatter,
    pub progress: ProgressIndicator,
    pub prompts: InteractivePrompts,
    pub suggestions: ErrorSuggestions,
}

impl UIManager {
    pub fn new(colors_enabled: bool, theme_type: ThemeType) -> Result<Self> {
        let theme = ColorTheme::new(theme_type);
        let formatter = OutputFormatter::new(colors_enabled, theme.clone());
        let progress = ProgressIndicator::new(colors_enabled, theme.clone());
        let prompts = InteractivePrompts::new(theme.clone());
        let suggestions = ErrorSuggestions::new();

        Ok(Self {
            formatter,
            progress,
            prompts,
            suggestions,
        })
    }

    /// Print a header with consistent styling
    pub fn print_header(&self, title: &str) {
        println!("{}", self.formatter.format_header(title));
    }

    /// Print a success message
    pub fn print_success(&self, message: &str) {
        println!("{}", self.formatter.format_success(message));
    }

    /// Print an info message
    pub fn print_info(&self, message: &str) {
        println!("{}", self.formatter.format_info(message));
    }

    /// Print a warning message
    pub fn print_warning(&self, message: &str) {
        println!("{}", self.formatter.format_warning(message));
    }

    /// Print an error message
    pub fn print_error(&self, message: &str) {
        println!("{}", self.formatter.format_error(message));
    }

    /// Print an error message with suggestions
    pub fn print_error_with_suggestions(&self, error: &str, context: Option<&str>) {
        let suggestions = self.suggestions.get_suggestions(error, context);
        let formatted = self.formatter.format_error_with_suggestions(error, &suggestions);
        println!("{}", formatted);
    }

    /// Print a thinking indicator
    pub async fn show_thinking(&self, message: &str) {
        self.progress.show_thinking(message).await;
    }

    /// Create a file scanning progress bar
    pub fn create_scan_progress(&self, total_files: u64) -> ScanProgress {
        self.progress.create_scan_progress(total_files)
    }

    /// Show an interactive prompt
    pub async fn prompt_user(&self, message: &str, options: &[&str]) -> Result<PromptResult> {
        self.prompts.prompt_user(message, options).await
    }

    /// Handle query ambiguity
    pub async fn handle_query_ambiguity(&self, ambiguity: QueryAmbiguity) -> Result<String> {
        self.prompts.handle_query_ambiguity(ambiguity).await
    }

    /// Clear the current line (useful for progress indicators)
    pub fn clear_line(&self) {
        print!("\r\x1b[K");
        io::stdout().flush().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_manager_creation() {
        let ui = UIManager::new(true, ThemeType::Dark).unwrap();
        assert!(ui.formatter.colors_enabled);
        assert_eq!(ui.formatter.theme.theme_type, ThemeType::Dark);
    }

    #[test]
    fn test_ui_manager_creation_no_colors() {
        let ui = UIManager::new(false, ThemeType::Minimal).unwrap();
        assert!(!ui.formatter.colors_enabled);
        assert_eq!(ui.formatter.theme.theme_type, ThemeType::Minimal);
    }

    #[test]
    fn test_ui_manager_auto_theme() {
        let ui = UIManager::new(true, ThemeType::Auto).unwrap();
        // Auto theme should resolve to either Dark or Minimal
        assert!(matches!(ui.formatter.theme.theme_type, ThemeType::Dark | ThemeType::Minimal));
    }

    #[test]
    fn test_ui_manager_all_themes() {
        for theme_type in [ThemeType::Auto, ThemeType::Light, ThemeType::Dark, ThemeType::HighContrast, ThemeType::Minimal] {
            let ui = UIManager::new(true, theme_type);
            assert!(ui.is_ok(), "Failed to create UI manager with theme: {:?}", theme_type);
        }
    }

    #[test]
    fn test_ui_manager_print_methods() {
        let ui = UIManager::new(false, ThemeType::Minimal).unwrap();
        
        // These should not panic
        ui.print_header("Test Header");
        ui.print_success("Test success");
        ui.print_info("Test info");
        ui.print_warning("Test warning");
        ui.print_error("Test error");
        ui.print_error_with_suggestions("Test error", Some("test context"));
    }

    #[tokio::test]
    async fn test_ui_manager_async_methods() {
        let ui = UIManager::new(false, ThemeType::Minimal).unwrap();
        
        // Test thinking indicator
        ui.show_thinking("Test thinking").await;
        
        // Test progress creation
        let progress = ui.create_scan_progress(100);
        progress.set_message("Test message");
        progress.finish_with_message("Test complete");
    }

    #[test]
    fn test_ui_manager_suggestions() {
        let ui = UIManager::new(false, ThemeType::Minimal).unwrap();
        
        // Test that suggestions are generated for common errors
        let suggestions = ui.suggestions.get_suggestions("file not found", None);
        assert!(!suggestions.is_empty());
        
        let suggestions = ui.suggestions.get_suggestions("api key missing", None);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_ui_manager_formatter_integration() {
        let ui = UIManager::new(true, ThemeType::Dark).unwrap();
        
        // Test formatter methods
        let header = ui.formatter.format_header("Test");
        assert!(header.contains("Test"));
        
        let success = ui.formatter.format_success("Success");
        assert!(success.contains("Success"));
        
        let error = ui.formatter.format_error("Error");
        assert!(error.contains("Error"));
    }

    #[test]
    fn test_ui_manager_theme_consistency() {
        let ui = UIManager::new(true, ThemeType::HighContrast).unwrap();
        
        // All components should use the same theme
        assert_eq!(ui.formatter.theme.theme_type, ThemeType::HighContrast);
        assert_eq!(ui.progress.theme.theme_type, ThemeType::HighContrast);
        // Note: prompts.theme is private, so we can't test it directly
        // but the consistency is ensured by passing the same theme to all constructors
    }
} 
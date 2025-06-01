use colored::{Color, ColoredString, Colorize};
use console::Term;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeType {
    Auto,      // Detect terminal and adjust
    Light,     // Light background
    Dark,      // Dark background  
    HighContrast, // High contrast for accessibility
    Minimal,   // Minimal colors
}

impl Default for ThemeType {
    fn default() -> Self {
        ThemeType::Auto
    }
}

impl fmt::Display for ThemeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeType::Auto => write!(f, "auto"),
            ThemeType::Light => write!(f, "light"),
            ThemeType::Dark => write!(f, "dark"),
            ThemeType::HighContrast => write!(f, "high-contrast"),
            ThemeType::Minimal => write!(f, "minimal"),
        }
    }
}

impl ThemeType {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ThemeType::Auto),
            "light" => Ok(ThemeType::Light),
            "dark" => Ok(ThemeType::Dark),
            "high-contrast" | "high_contrast" => Ok(ThemeType::HighContrast),
            "minimal" => Ok(ThemeType::Minimal),
            _ => Err(format!("Unknown theme type: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColorTheme {
    pub theme_type: ThemeType,
    
    // Primary colors
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    
    // Status colors
    pub success: Color,
    pub info: Color,
    pub warning: Color,
    pub error: Color,
    
    // Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_bold: Color,
    
    // Syntax highlighting
    pub keyword: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub function: Color,
    pub type_name: Color,
    
    // UI elements
    pub border: Color,
    pub background: Option<Color>,
    pub highlight: Color,
    
    // Special elements
    pub progress_done: Color,
    pub progress_todo: Color,
    pub thinking: Color,
}

impl ColorTheme {
    pub fn new(theme_type: ThemeType) -> Self {
        match theme_type {
            ThemeType::Auto => Self::auto_detect(),
            ThemeType::Light => Self::light_theme(),
            ThemeType::Dark => Self::dark_theme(),
            ThemeType::HighContrast => Self::high_contrast_theme(),
            ThemeType::Minimal => Self::minimal_theme(),
        }
    }

    fn auto_detect() -> Self {
        // Try to detect if we're on a light or dark terminal
        // For now, default to dark as it's more common for developers
        if Term::stdout().is_term() {
            // Could add more sophisticated detection here
            Self::dark_theme()
        } else {
            Self::minimal_theme()
        }
    }

    fn light_theme() -> Self {
        Self {
            theme_type: ThemeType::Light,
            
            // Primary colors - more muted for light backgrounds
            primary: Color::Blue,
            secondary: Color::Cyan,
            accent: Color::Magenta,
            
            // Status colors
            success: Color::Green,
            info: Color::Blue,
            warning: Color::Yellow,
            error: Color::Red,
            
            // Text colors - darker for readability on light
            text_primary: Color::Black,
            text_secondary: Color::BrightBlack,
            text_muted: Color::BrightBlack,
            text_bold: Color::Black,
            
            // Syntax highlighting
            keyword: Color::Blue,
            string: Color::Green,
            number: Color::Red,
            comment: Color::BrightBlack,
            function: Color::Magenta,
            type_name: Color::Cyan,
            
            // UI elements
            border: Color::BrightBlack,
            background: None,
            highlight: Color::BrightYellow,
            
            // Special elements
            progress_done: Color::Green,
            progress_todo: Color::BrightBlack,
            thinking: Color::Blue,
        }
    }

    fn dark_theme() -> Self {
        Self {
            theme_type: ThemeType::Dark,
            
            // Primary colors - bright for dark backgrounds
            primary: Color::BrightBlue,
            secondary: Color::BrightCyan,
            accent: Color::BrightMagenta,
            
            // Status colors
            success: Color::BrightGreen,
            info: Color::BrightBlue,
            warning: Color::BrightYellow,
            error: Color::BrightRed,
            
            // Text colors - bright for readability on dark
            text_primary: Color::White,
            text_secondary: Color::BrightWhite,
            text_muted: Color::BrightBlack,
            text_bold: Color::BrightWhite,
            
            // Syntax highlighting
            keyword: Color::BrightBlue,
            string: Color::BrightGreen,
            number: Color::BrightRed,
            comment: Color::BrightBlack,
            function: Color::BrightMagenta,
            type_name: Color::BrightCyan,
            
            // UI elements
            border: Color::BrightBlack,
            background: None,
            highlight: Color::BrightYellow,
            
            // Special elements
            progress_done: Color::BrightGreen,
            progress_todo: Color::BrightBlack,
            thinking: Color::BrightBlue,
        }
    }

    fn high_contrast_theme() -> Self {
        Self {
            theme_type: ThemeType::HighContrast,
            
            // Primary colors - maximum contrast
            primary: Color::BrightWhite,
            secondary: Color::BrightYellow,
            accent: Color::BrightMagenta,
            
            // Status colors - high contrast
            success: Color::BrightGreen,
            info: Color::BrightCyan,
            warning: Color::BrightYellow,
            error: Color::BrightRed,
            
            // Text colors
            text_primary: Color::BrightWhite,
            text_secondary: Color::BrightWhite,
            text_muted: Color::White,
            text_bold: Color::BrightWhite,
            
            // Syntax highlighting - high contrast
            keyword: Color::BrightBlue,
            string: Color::BrightGreen,
            number: Color::BrightYellow,
            comment: Color::White,
            function: Color::BrightMagenta,
            type_name: Color::BrightCyan,
            
            // UI elements
            border: Color::BrightWhite,
            background: None,
            highlight: Color::BrightYellow,
            
            // Special elements
            progress_done: Color::BrightGreen,
            progress_todo: Color::White,
            thinking: Color::BrightCyan,
        }
    }

    fn minimal_theme() -> Self {
        Self {
            theme_type: ThemeType::Minimal,
            
            // Primary colors - minimal, mostly using default terminal colors
            primary: Color::White,
            secondary: Color::White,
            accent: Color::White,
            
            // Status colors - only basics
            success: Color::Green,
            info: Color::White,
            warning: Color::Yellow,
            error: Color::Red,
            
            // Text colors
            text_primary: Color::White,
            text_secondary: Color::White,
            text_muted: Color::BrightBlack,
            text_bold: Color::White,
            
            // Syntax highlighting - minimal
            keyword: Color::White,
            string: Color::White,
            number: Color::White,
            comment: Color::BrightBlack,
            function: Color::White,
            type_name: Color::White,
            
            // UI elements
            border: Color::BrightBlack,
            background: None,
            highlight: Color::BrightBlack,
            
            // Special elements
            progress_done: Color::White,
            progress_todo: Color::BrightBlack,
            thinking: Color::White,
        }
    }

    // Helper methods for common formatting patterns
    
    pub fn format_header(&self, text: &str) -> ColoredString {
        format!("━━━ {} ━━━", text).color(self.primary).bold()
    }

    pub fn format_success(&self, text: &str) -> ColoredString {
        format!("✅ {}", text).color(self.success)
    }

    pub fn format_info(&self, text: &str) -> ColoredString {
        format!("ℹ  {}", text).color(self.info)
    }

    pub fn format_warning(&self, text: &str) -> ColoredString {
        format!("⚠️  {}", text).color(self.warning)
    }

    pub fn format_error(&self, text: &str) -> ColoredString {
        format!("❌ {}", text).color(self.error).bold()
    }

    pub fn format_thinking(&self, text: &str) -> ColoredString {
        format!("🤔 {}", text).color(self.thinking)
    }

    pub fn format_progress(&self, text: &str) -> ColoredString {
        format!("🔄 {}", text).color(self.progress_done)
    }

    pub fn format_highlight(&self, text: &str) -> ColoredString {
        text.color(self.highlight).bold()
    }

    pub fn format_muted(&self, text: &str) -> ColoredString {
        text.color(self.text_muted)
    }

    pub fn format_code(&self, text: &str) -> ColoredString {
        text.color(self.function)
    }

    pub fn format_file_path(&self, text: &str) -> ColoredString {
        text.color(self.secondary)
    }

    pub fn format_number(&self, text: &str) -> ColoredString {
        text.color(self.number).bold()
    }

    pub fn format_keyword(&self, text: &str) -> ColoredString {
        text.color(self.keyword).bold()
    }

    pub fn format_type(&self, text: &str) -> ColoredString {
        text.color(self.type_name)
    }

    pub fn format_string(&self, text: &str) -> ColoredString {
        text.color(self.string)
    }

    // Terminal capability detection
    pub fn supports_unicode(&self) -> bool {
        Term::stdout().features().wants_emoji()
    }

    pub fn supports_colors(&self) -> bool {
        Term::stdout().features().colors_supported()
    }

    pub fn get_terminal_width(&self) -> usize {
        Term::stdout().size().1 as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_type_display() {
        assert_eq!(ThemeType::Auto.to_string(), "auto");
        assert_eq!(ThemeType::Light.to_string(), "light");
        assert_eq!(ThemeType::Dark.to_string(), "dark");
        assert_eq!(ThemeType::HighContrast.to_string(), "high-contrast");
        assert_eq!(ThemeType::Minimal.to_string(), "minimal");
    }

    #[test]
    fn test_theme_type_from_str() {
        assert_eq!(ThemeType::from_str("auto").unwrap(), ThemeType::Auto);
        assert_eq!(ThemeType::from_str("light").unwrap(), ThemeType::Light);
        assert_eq!(ThemeType::from_str("dark").unwrap(), ThemeType::Dark);
        assert_eq!(ThemeType::from_str("high-contrast").unwrap(), ThemeType::HighContrast);
        assert_eq!(ThemeType::from_str("high_contrast").unwrap(), ThemeType::HighContrast);
        assert_eq!(ThemeType::from_str("minimal").unwrap(), ThemeType::Minimal);
        
        assert!(ThemeType::from_str("invalid").is_err());
    }

    #[test]
    fn test_theme_type_default() {
        assert_eq!(ThemeType::default(), ThemeType::Auto);
    }

    #[test]
    fn test_color_theme_creation() {
        for theme_type in [ThemeType::Auto, ThemeType::Light, ThemeType::Dark, ThemeType::HighContrast, ThemeType::Minimal] {
            let theme = ColorTheme::new(theme_type);
            assert_eq!(theme.theme_type, theme_type);
        }
    }

    #[test]
    fn test_light_theme_properties() {
        let theme = ColorTheme::light_theme();
        assert_eq!(theme.theme_type, ThemeType::Light);
        assert_eq!(theme.primary, Color::Blue);
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.error, Color::Red);
        assert_eq!(theme.text_primary, Color::Black);
    }

    #[test]
    fn test_dark_theme_properties() {
        let theme = ColorTheme::dark_theme();
        assert_eq!(theme.theme_type, ThemeType::Dark);
        assert_eq!(theme.primary, Color::BrightBlue);
        assert_eq!(theme.success, Color::BrightGreen);
        assert_eq!(theme.error, Color::BrightRed);
        assert_eq!(theme.text_primary, Color::White);
    }

    #[test]
    fn test_high_contrast_theme_properties() {
        let theme = ColorTheme::high_contrast_theme();
        assert_eq!(theme.theme_type, ThemeType::HighContrast);
        assert_eq!(theme.primary, Color::BrightWhite);
        assert_eq!(theme.success, Color::BrightGreen);
        assert_eq!(theme.error, Color::BrightRed);
        assert_eq!(theme.text_primary, Color::BrightWhite);
    }

    #[test]
    fn test_minimal_theme_properties() {
        let theme = ColorTheme::minimal_theme();
        assert_eq!(theme.theme_type, ThemeType::Minimal);
        assert_eq!(theme.primary, Color::White);
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.error, Color::Red);
        assert_eq!(theme.text_primary, Color::White);
    }

    #[test]
    fn test_theme_formatting_methods() {
        let theme = ColorTheme::new(ThemeType::Dark);
        
        let header = theme.format_header("Test Header");
        assert!(header.to_string().contains("Test Header"));
        
        let success = theme.format_success("Success message");
        assert!(success.to_string().contains("Success message"));
        
        let info = theme.format_info("Info message");
        assert!(info.to_string().contains("Info message"));
        
        let warning = theme.format_warning("Warning message");
        assert!(warning.to_string().contains("Warning message"));
        
        let error = theme.format_error("Error message");
        assert!(error.to_string().contains("Error message"));
        
        let thinking = theme.format_thinking("Thinking message");
        assert!(thinking.to_string().contains("Thinking message"));
        
        let progress = theme.format_progress("Progress message");
        assert!(progress.to_string().contains("Progress message"));
    }

    #[test]
    fn test_theme_text_formatting() {
        let theme = ColorTheme::new(ThemeType::Dark);
        
        let highlight = theme.format_highlight("highlighted text");
        assert!(highlight.to_string().contains("highlighted text"));
        
        let muted = theme.format_muted("muted text");
        assert!(muted.to_string().contains("muted text"));
        
        let code = theme.format_code("code text");
        assert!(code.to_string().contains("code text"));
        
        let file_path = theme.format_file_path("/path/to/file");
        assert!(file_path.to_string().contains("/path/to/file"));
        
        let number = theme.format_number("42");
        assert!(number.to_string().contains("42"));
        
        let keyword = theme.format_keyword("fn");
        assert!(keyword.to_string().contains("fn"));
        
        let type_name = theme.format_type("String");
        assert!(type_name.to_string().contains("String"));
        
        let string = theme.format_string("\"hello\"");
        assert!(string.to_string().contains("\"hello\""));
    }

    #[test]
    fn test_auto_detect_theme() {
        let theme = ColorTheme::auto_detect();
        // Auto detect should return either Dark or Minimal theme
        assert!(matches!(theme.theme_type, ThemeType::Dark | ThemeType::Minimal));
    }

    #[test]
    fn test_terminal_capabilities() {
        let theme = ColorTheme::new(ThemeType::Dark);
        
        // These methods should not panic
        let _supports_unicode = theme.supports_unicode();
        let _supports_colors = theme.supports_colors();
        let _terminal_width = theme.get_terminal_width();
    }

    #[test]
    fn test_theme_consistency() {
        // Test that all themes have consistent color assignments
        for theme_type in [ThemeType::Light, ThemeType::Dark, ThemeType::HighContrast, ThemeType::Minimal] {
            let theme = ColorTheme::new(theme_type);
            
            // All themes should have these basic colors defined
            assert_ne!(theme.success, theme.error);
            assert_ne!(theme.warning, theme.error);
            assert_ne!(theme.info, theme.error);
        }
    }

    #[test]
    fn test_theme_clone() {
        let theme1 = ColorTheme::new(ThemeType::Dark);
        let theme2 = theme1.clone();
        
        assert_eq!(theme1.theme_type, theme2.theme_type);
        assert_eq!(theme1.primary, theme2.primary);
        assert_eq!(theme1.success, theme2.success);
    }
} 
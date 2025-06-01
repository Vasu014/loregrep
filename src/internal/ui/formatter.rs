use super::themes::ColorTheme;
use super::suggestions::Suggestion;
use tabled::{Table, Tabled, settings::{Style, Alignment, Modify, Width, object::{Rows, Columns}}};

/// Centralized output formatting with theme support
pub struct OutputFormatter {
    pub colors_enabled: bool,
    pub theme: ColorTheme,
    max_results_display: usize,
}

impl OutputFormatter {
    pub fn new(colors_enabled: bool, theme: ColorTheme) -> Self {
        Self {
            colors_enabled,
            theme,
            max_results_display: 50,
        }
    }

    pub fn set_max_results(&mut self, max: usize) {
        self.max_results_display = max;
    }

    // Core formatting methods

    pub fn format_header(&self, title: &str) -> String {
        if self.colors_enabled {
            self.theme.format_header(title).to_string()
        } else {
            format!("=== {} ===", title)
        }
    }

    pub fn format_success(&self, message: &str) -> String {
        if self.colors_enabled {
            self.theme.format_success(message).to_string()
        } else {
            format!("[OK] {}", message)
        }
    }

    pub fn format_info(&self, message: &str) -> String {
        if self.colors_enabled {
            self.theme.format_info(message).to_string()
        } else {
            format!("[INFO] {}", message)
        }
    }

    pub fn format_warning(&self, message: &str) -> String {
        if self.colors_enabled {
            self.theme.format_warning(message).to_string()
        } else {
            format!("[WARN] {}", message)
        }
    }

    pub fn format_error(&self, message: &str) -> String {
        if self.colors_enabled {
            self.theme.format_error(message).to_string()
        } else {
            format!("[ERROR] {}", message)
        }
    }

    pub fn format_error_with_suggestions(&self, error: &str, suggestions: &[Suggestion]) -> String {
        let mut output = self.format_error(error);
        
        if !suggestions.is_empty() {
            output.push('\n');
            
            if self.colors_enabled {
                output.push_str(&self.theme.format_info("Suggestions:").to_string());
            } else {
                output.push_str("[SUGGESTIONS]");
            }
            
            for (i, suggestion) in suggestions.iter().enumerate() {
                output.push('\n');
                if self.colors_enabled {
                    output.push_str(&format!("  {}. {}", 
                        self.theme.format_number(&(i + 1).to_string()),
                        suggestion.message
                    ));
                } else {
                    output.push_str(&format!("  {}. {}", i + 1, suggestion.message));
                }
                
                if let Some(ref command) = suggestion.command {
                    output.push('\n');
                    if self.colors_enabled {
                        output.push_str(&format!("     Try: {}", self.theme.format_code(command)));
                    } else {
                        output.push_str(&format!("     Try: {}", command));
                    }
                }
            }
        }
        
        output
    }

    // Code analysis formatting

    pub fn format_function_signature(&self, name: &str, params: &[String], return_type: Option<&str>) -> String {
        let mut signature = String::new();
        
        if self.colors_enabled {
            signature.push_str(&self.theme.format_keyword("fn").to_string());
            signature.push(' ');
            signature.push_str(&self.theme.format_code(name).to_string());
        } else {
            signature.push_str("fn ");
            signature.push_str(name);
        }
        
        signature.push('(');
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                signature.push_str(", ");
            }
            if self.colors_enabled {
                signature.push_str(&self.theme.format_muted(param).to_string());
            } else {
                signature.push_str(param);
            }
        }
        signature.push(')');
        
        if let Some(ret_type) = return_type {
            signature.push_str(" -> ");
            if self.colors_enabled {
                signature.push_str(&self.theme.format_type(ret_type).to_string());
            } else {
                signature.push_str(ret_type);
            }
        }
        
        signature
    }

    pub fn format_struct_signature(&self, name: &str, fields: &[String]) -> String {
        let mut signature = String::new();
        
        if self.colors_enabled {
            signature.push_str(&self.theme.format_keyword("struct").to_string());
            signature.push(' ');
            signature.push_str(&self.theme.format_type(name).to_string());
        } else {
            signature.push_str("struct ");
            signature.push_str(name);
        }
        
        if !fields.is_empty() {
            signature.push_str(" {");
            for (i, field) in fields.iter().enumerate() {
                if i == 0 {
                    signature.push(' ');
                }
                if self.colors_enabled {
                    signature.push_str(&self.theme.format_muted(field).to_string());
                } else {
                    signature.push_str(field);
                }
                if i < fields.len() - 1 {
                    signature.push_str(", ");
                }
            }
            signature.push_str(" }");
        }
        
        signature
    }

    pub fn format_file_location(&self, file_path: &str, line: Option<u32>) -> String {
        let mut location = String::new();
        
        if self.colors_enabled {
            location.push_str(&self.theme.format_file_path(file_path).to_string());
        } else {
            location.push_str(file_path);
        }
        
        if let Some(line_num) = line {
            location.push(':');
            if self.colors_enabled {
                location.push_str(&self.theme.format_number(&line_num.to_string()).to_string());
            } else {
                location.push_str(&line_num.to_string());
            }
        }
        
        location
    }

    // Search result formatting

    pub fn format_search_results(&self, results: &[SearchResult], query: &str) -> String {
        if results.is_empty() {
            return self.format_warning(&format!("No results found for query: {}", query));
        }

        let mut output = String::new();
        let display_count = results.len().min(self.max_results_display);
        
        // Header
        output.push_str(&self.format_success(&format!(
            "Found {} result{} for \"{}\"",
            results.len(),
            if results.len() == 1 { "" } else { "s" },
            query
        )));
        
        if results.len() > self.max_results_display {
            output.push('\n');
            output.push_str(&self.format_info(&format!(
                "Showing first {} results (use --limit to see more)",
                self.max_results_display
            )));
        }
        
        output.push_str("\n\n");
        
        // Results
        for (i, result) in results.iter().take(display_count).enumerate() {
            if i > 0 {
                output.push('\n');
            }
            output.push_str(&self.format_search_result(result, i + 1));
        }
        
        output
    }

    fn format_search_result(&self, result: &SearchResult, index: usize) -> String {
        let mut output = String::new();
        
        // Index and type
        if self.colors_enabled {
            output.push_str(&format!("{}. {}: ",
                self.theme.format_number(&index.to_string()),
                self.theme.format_keyword(&result.result_type)
            ));
        } else {
            output.push_str(&format!("{}. {}: ", index, result.result_type));
        }
        
        // Main content
        output.push_str(&result.content);
        
        // Location
        output.push('\n');
        if self.colors_enabled {
            output.push_str(&format!("   📍 {}",
                self.theme.format_muted(&self.format_file_location(&result.file_path, result.line))
            ));
        } else {
            output.push_str(&format!("   Location: {}", 
                self.format_file_location(&result.file_path, result.line)
            ));
        }
        
        // Additional context if available
        if let Some(ref context) = result.context {
            output.push('\n');
            if self.colors_enabled {
                output.push_str(&format!("   💡 {}",
                    self.theme.format_muted(context)
                ));
            } else {
                output.push_str(&format!("   Context: {}", context));
            }
        }
        
        output
    }

    // Analysis result formatting

    pub fn format_analysis_summary(&self, 
        file_count: usize, 
        function_count: usize, 
        struct_count: usize,
        duration: std::time::Duration
    ) -> String {
        let mut output = String::new();
        
        output.push_str(&self.format_success("Analysis Complete"));
        output.push('\n');
        output.push('\n');
        
        // Create table data
        #[derive(Tabled)]
        struct AnalysisRow {
            #[tabled(rename = "Metric")]
            metric: String,
            #[tabled(rename = "Count")]
            count: String,
            #[tabled(rename = "Details")]
            details: String,
        }
        
        let rows = vec![
            AnalysisRow {
                metric: "Files Analyzed".to_string(),
                count: file_count.to_string(),
                details: if file_count == 1 { "file scanned".to_string() } else { "files scanned".to_string() },
            },
            AnalysisRow {
                metric: "Functions Found".to_string(),
                count: function_count.to_string(),
                details: if function_count == 1 { "function extracted".to_string() } else { "functions extracted".to_string() },
            },
            AnalysisRow {
                metric: "Structs Found".to_string(),
                count: struct_count.to_string(),
                details: if struct_count == 1 { "struct/type found".to_string() } else { "structs/types found".to_string() },
            },
            AnalysisRow {
                metric: "Processing Time".to_string(),
                count: format!("{:.2?}", duration),
                details: "total analysis time".to_string(),
            },
        ];
        
        let mut table = Table::new(rows);
        
        // Apply styling with better column width control
        table
            .with(Style::rounded())
            .with(Modify::new(Rows::first()).with(Alignment::center()))
            .with(Modify::new(Columns::single(0)).with(Alignment::left()))
            .with(Modify::new(Columns::single(1)).with(Alignment::center())) // Center align count for better look
            .with(Modify::new(Columns::single(2)).with(Alignment::left()))
            .with(Width::list([20, 10, 25])); // Set explicit column widths: Metric(20), Count(10), Details(25)
        
        // Apply colors if enabled
        if self.colors_enabled {
            // For now, we'll add the table as-is since tabled doesn't directly support
            // colored integration, but we can enhance this further
            output.push_str("  ");
            for line in table.to_string().lines() {
                output.push_str(&self.theme.format_muted(line));
                output.push('\n');
            }
        } else {
            // Indent the table
            for line in table.to_string().lines() {
                output.push_str("  ");
                output.push_str(line);
                output.push('\n');
            }
        }
        
        output
    }

    // AI conversation formatting with full markdown support

    pub fn format_ai_response(&self, response: &str) -> String {
        if !self.colors_enabled {
            return self.format_markdown_plain(response);
        }
        
        self.format_markdown_with_theme(response)
    }

    fn format_markdown_with_theme(&self, text: &str) -> String {
        let mut output = String::new();
        let mut in_code_block = false;
        let mut in_table = false;
        let mut table_headers: Vec<String> = Vec::new();
        
        for line in text.lines() {
            let trimmed = line.trim();
            
            // Handle code blocks first (highest priority)
            if trimmed.starts_with("```") {
                if in_code_block {
                    // End code block
                    in_code_block = false;
                    output.push_str(&self.theme.format_muted("```").to_string());
                } else {
                    // Start code block
                    in_code_block = true;
                    let code_language = trimmed.strip_prefix("```").filter(|s| !s.is_empty());
                    if let Some(lang) = code_language {
                        output.push_str(&self.theme.format_muted(&format!("```{}", lang)).to_string());
                    } else {
                        output.push_str(&self.theme.format_muted("```").to_string());
                    }
                }
                output.push('\n');
                continue;
            }
            
            if in_code_block {
                // Inside code block - apply syntax highlighting
                output.push_str(&self.theme.format_code(line).to_string());
                output.push('\n');
                continue;
            }
            
            // Handle tables
            if trimmed.contains('|') && !trimmed.starts_with('#') {
                if !in_table {
                    // Starting a new table
                    in_table = true;
                    table_headers = self.parse_table_row(trimmed);
                    output.push_str(&self.format_table_header(&table_headers));
                } else if trimmed.chars().all(|c| c == '-' || c == '|' || c == ' ' || c == ':') {
                    // Table separator row - format as separator
                    output.push_str(&self.format_table_separator(&table_headers));
                } else {
                    // Table data row
                    let cells = self.parse_table_row(trimmed);
                    output.push_str(&self.format_table_row(&cells));
                }
                output.push('\n');
                continue;
            } else if in_table {
                in_table = false;
                table_headers.clear();
            }
            
            // Format the line based on markdown syntax
            let formatted_line = self.format_markdown_line(line);
            output.push_str(&formatted_line);
            output.push('\n');
        }
        
        output
    }
    
    fn format_markdown_line(&self, line: &str) -> String {
        let trimmed = line.trim();
        
        // Headers
        if let Some(header_text) = trimmed.strip_prefix("### ") {
            return format!("   {}", self.theme.format_header(header_text));
        }
        if let Some(header_text) = trimmed.strip_prefix("## ") {
            return format!("  {}", self.theme.format_header(header_text));
        }
        if let Some(header_text) = trimmed.strip_prefix("# ") {
            return format!(" {}", self.theme.format_header(header_text));
        }
        
        // Blockquotes
        if let Some(quote_text) = trimmed.strip_prefix("> ") {
            return format!("│ {}", self.theme.format_muted(quote_text));
        }
        if trimmed == ">" {
            return "│".to_string();
        }
        
        // Horizontal rules
        if trimmed == "---" || trimmed == "***" || trimmed.chars().all(|c| c == '-' && trimmed.len() >= 3) {
            let horizontal_rule = "─".repeat(50);
            return self.theme.format_muted(&horizontal_rule).to_string();
        }
        
        // Unordered lists
        if let Some(list_text) = trimmed.strip_prefix("- ") {
            return format!("  • {}", self.format_inline_markdown(list_text));
        }
        if let Some(list_text) = trimmed.strip_prefix("* ") {
            return format!("  • {}", self.format_inline_markdown(list_text));
        }
        
        // Ordered lists
        if let Some(caps) = regex::Regex::new(r"^(\d+)\. (.*)").unwrap().captures(trimmed) {
            let number = caps.get(1).unwrap().as_str();
            let text = caps.get(2).unwrap().as_str();
            return format!("  {}. {}", 
                self.theme.format_number(number),
                self.format_inline_markdown(text)
            );
        }
        
        // Regular text with inline formatting
        if !trimmed.is_empty() {
            self.format_inline_markdown(line)
        } else {
            String::new()
        }
    }
    
    fn format_inline_markdown(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();
        let mut current_word = String::new();
        
        while let Some(ch) = chars.next() {
            match ch {
                // Bold text **text**
                '*' if chars.peek() == Some(&'*') => {
                    chars.next(); // consume second *
                    if !current_word.is_empty() {
                        result.push_str(&current_word);
                        current_word.clear();
                    }
                    
                    let mut bold_text = String::new();
                    let mut found_end = false;
                    
                    while let Some(ch) = chars.next() {
                        if ch == '*' && chars.peek() == Some(&'*') {
                            chars.next(); // consume second *
                            found_end = true;
                            break;
                        }
                        bold_text.push(ch);
                    }
                    
                    if found_end {
                        result.push_str(&self.theme.format_highlight(&bold_text).to_string());
                    } else {
                        result.push_str("**");
                        result.push_str(&bold_text);
                    }
                }
                // Italic text *text*
                '*' => {
                    if !current_word.is_empty() {
                        result.push_str(&current_word);
                        current_word.clear();
                    }
                    
                    let mut italic_text = String::new();
                    let mut found_end = false;
                    
                    while let Some(ch) = chars.next() {
                        if ch == '*' {
                            found_end = true;
                            break;
                        }
                        italic_text.push(ch);
                    }
                    
                    if found_end {
                        result.push_str(&self.theme.format_muted(&italic_text).to_string());
                    } else {
                        result.push('*');
                        result.push_str(&italic_text);
                    }
                }
                // Inline code `code`
                '`' => {
                    if !current_word.is_empty() {
                        result.push_str(&current_word);
                        current_word.clear();
                    }
                    
                    let mut code_text = String::new();
                    let mut found_end = false;
                    
                    while let Some(ch) = chars.next() {
                        if ch == '`' {
                            found_end = true;
                            break;
                        }
                        code_text.push(ch);
                    }
                    
                    if found_end {
                        result.push_str(&self.theme.format_code(&code_text).to_string());
                    } else {
                        result.push('`');
                        result.push_str(&code_text);
                    }
                }
                // Links [text](url)
                '[' => {
                    if !current_word.is_empty() {
                        result.push_str(&current_word);
                        current_word.clear();
                    }
                    
                    let mut link_text = String::new();
                    let mut found_closing_bracket = false;
                    
                    // Read link text
                    while let Some(ch) = chars.next() {
                        if ch == ']' {
                            found_closing_bracket = true;
                            break;
                        }
                        link_text.push(ch);
                    }
                    
                    if found_closing_bracket && chars.peek() == Some(&'(') {
                        chars.next(); // consume (
                        let mut url = String::new();
                        let mut found_closing_paren = false;
                        
                        while let Some(ch) = chars.next() {
                            if ch == ')' {
                                found_closing_paren = true;
                                break;
                            }
                            url.push(ch);
                        }
                        
                        if found_closing_paren {
                            result.push_str(&format!("{} ({})",
                                self.theme.format_highlight(&link_text),
                                self.theme.format_muted(&url)
                            ));
                        } else {
                            result.push('[');
                            result.push_str(&link_text);
                            result.push_str("](");
                            result.push_str(&url);
                        }
                    } else {
                        result.push('[');
                        result.push_str(&link_text);
                        if found_closing_bracket {
                            result.push(']');
                        }
                    }
                }
                _ => {
                    current_word.push(ch);
                }
            }
        }
        
        if !current_word.is_empty() {
            result.push_str(&current_word);
        }
        
        result
    }
    
    fn parse_table_row(&self, line: &str) -> Vec<String> {
        line.split('|')
            .map(|cell| cell.trim().to_string())
            .filter(|cell| !cell.is_empty())
            .collect()
    }
    
    fn format_table_header(&self, headers: &[String]) -> String {
        let mut result = String::new();
        result.push_str("┌");
        for (i, header) in headers.iter().enumerate() {
            if i > 0 {
                result.push_str("┬");
            }
            result.push_str(&"─".repeat(header.len().max(8) + 2));
        }
        result.push_str("┐\n│");
        
        for (i, header) in headers.iter().enumerate() {
            if i > 0 {
                result.push_str("│");
            }
            result.push_str(&format!(" {:<width$} ", 
                self.theme.format_highlight(header), 
                width = header.len().max(8)
            ));
        }
        result.push_str("│");
        
        result
    }
    
    fn format_table_separator(&self, headers: &[String]) -> String {
        let mut result = String::new();
        result.push_str("├");
        for (i, header) in headers.iter().enumerate() {
            if i > 0 {
                result.push_str("┼");
            }
            result.push_str(&"─".repeat(header.len().max(8) + 2));
        }
        result.push_str("┤");
        
        result
    }
    
    fn format_table_row(&self, cells: &[String]) -> String {
        let mut result = String::new();
        result.push_str("│");
        
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                result.push_str("│");
            }
            result.push_str(&format!(" {:<8} ", self.format_inline_markdown(cell)));
        }
        result.push_str("│");
        
        result
    }
    
    fn format_markdown_plain(&self, text: &str) -> String {
        // Plain text version without colors for accessibility
        let mut output = String::new();
        let mut in_code_block = false;
        
        for line in text.lines() {
            let trimmed = line.trim();
            
            // Handle code blocks
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                output.push_str(line);
                output.push('\n');
                continue;
            }
            
            if in_code_block {
                output.push_str(line);
                output.push('\n');
                continue;
            }
            
            // Convert markdown to plain text
            let plain_line = self.convert_markdown_to_plain(line);
            output.push_str(&plain_line);
            output.push('\n');
        }
        
        output
    }
    
    fn convert_markdown_to_plain(&self, line: &str) -> String {
        let trimmed = line.trim();
        
        // Headers
        if let Some(text) = trimmed.strip_prefix("### ") {
            return format!("   {}", text);
        }
        if let Some(text) = trimmed.strip_prefix("## ") {
            return format!("  {}", text);
        }
        if let Some(text) = trimmed.strip_prefix("# ") {
            return format!(" {}", text);
        }
        
        // Blockquotes
        if let Some(text) = trimmed.strip_prefix("> ") {
            return format!("| {}", text);
        }
        
        // Lists
        if let Some(text) = trimmed.strip_prefix("- ") {
            return format!("  - {}", self.strip_inline_markdown(text));
        }
        if let Some(text) = trimmed.strip_prefix("* ") {
            return format!("  * {}", self.strip_inline_markdown(text));
        }
        
        // Ordered lists
        if let Some(caps) = regex::Regex::new(r"^(\d+)\. (.*)").unwrap().captures(trimmed) {
            let number = caps.get(1).unwrap().as_str();
            let text = caps.get(2).unwrap().as_str();
            return format!("  {}. {}", number, self.strip_inline_markdown(text));
        }
        
        // Horizontal rules
        if trimmed == "---" || trimmed == "***" {
            return "-".repeat(50);
        }
        
        self.strip_inline_markdown(line)
    }
    
    fn strip_inline_markdown(&self, text: &str) -> String {
        // Remove markdown formatting for plain text output
        text
            .replace("**", "")
            .replace("*", "")
            .replace("`", "")
            // Simple link conversion [text](url) -> text (url)
            .replace(|c| c == '[' || c == ']' || c == '(' || c == ')', "")
    }

    // Table formatting for configuration display

    pub fn format_config_table(&self, config_items: &[(String, String)]) -> String {
        if config_items.is_empty() {
            return self.format_info("No configuration items to display");
        }

        let mut output = String::new();
        let max_key_len = config_items.iter()
            .map(|(k, _)| k.len())
            .max()
            .unwrap_or(0);
        
        for (key, value) in config_items {
            if self.colors_enabled {
                output.push_str(&format!("  {}: {}",
                    self.theme.format_muted(&format!("{:<width$}", key, width = max_key_len)),
                    self.theme.format_highlight(value)
                ));
            } else {
                output.push_str(&format!("  {:<width$}: {}", key, value, width = max_key_len));
            }
            output.push('\n');
        }
        
        output
    }

    /// Create a beautifully formatted table with optional colors
    pub fn format_table<T: Tabled>(&self, data: Vec<T>, title: Option<&str>) -> String {
        let mut output = String::new();
        
        if let Some(title) = title {
            output.push_str(&self.format_header(title));
            output.push('\n');
            output.push('\n');
        }
        
        if data.is_empty() {
            output.push_str(&self.theme.format_muted("  No data to display"));
            output.push('\n');
            return output;
        }
        
        let mut table = Table::new(data);
        
        // Apply styling
        table
            .with(Style::rounded())
            .with(Modify::new(Rows::first()).with(Alignment::center()))
            .with(Modify::new(Columns::new(1..)).with(Alignment::left()));
        
        // Apply colors and indentation
        if self.colors_enabled {
            for line in table.to_string().lines() {
                output.push_str("  ");
                output.push_str(&self.theme.format_muted(line));
                output.push('\n');
            }
        } else {
            for line in table.to_string().lines() {
                output.push_str("  ");
                output.push_str(line);
                output.push('\n');
            }
        }
        
        output
    }

    /// Format a simple key-value table
    pub fn format_key_value_table(&self, data: Vec<(&str, &str)>, title: Option<&str>) -> String {
        #[derive(Tabled)]
        struct KeyValueRow {
            #[tabled(rename = "Property")]
            key: String,
            #[tabled(rename = "Value")]
            value: String,
        }
        
        let rows: Vec<KeyValueRow> = data
            .into_iter()
            .map(|(k, v)| KeyValueRow {
                key: k.to_string(),
                value: v.to_string(),
            })
            .collect();
        
        self.format_table(rows, title)
    }

    /// Format search results as a table
    pub fn format_search_results_table(&self, results: Vec<(&str, &str, u32)>, title: Option<&str>) -> String {
        #[derive(Tabled)]
        struct SearchResultRow {
            #[tabled(rename = "Name")]
            name: String,
            #[tabled(rename = "File")]
            file: String,
            #[tabled(rename = "Line")]
            line: String,
        }
        
        let rows: Vec<SearchResultRow> = results
            .into_iter()
            .map(|(name, file, line)| SearchResultRow {
                name: name.to_string(),
                file: file.to_string(),
                line: line.to_string(),
            })
            .collect();
        
        self.format_table(rows, title)
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub result_type: String,
    pub content: String,
    pub file_path: String,
    pub line: Option<u32>,
    pub context: Option<String>,
    pub score: Option<f64>,
}

impl SearchResult {
    pub fn new(
        result_type: String,
        content: String,
        file_path: String,
        line: Option<u32>,
    ) -> Self {
        Self {
            result_type,
            content,
            file_path,
            line,
            context: None,
            score: None,
        }
    }

    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_score(mut self, score: f64) -> Self {
        self.score = Some(score);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::ui::themes::{ColorTheme, ThemeType};
    use crate::internal::ui::suggestions::{Suggestion, SuggestionPriority, SuggestionType};

    fn create_test_formatter(colors_enabled: bool) -> OutputFormatter {
        let theme = ColorTheme::new(ThemeType::Dark);
        OutputFormatter::new(colors_enabled, theme)
    }

    #[test]
    fn test_formatter_creation() {
        let formatter = create_test_formatter(true);
        assert!(formatter.colors_enabled);
        assert_eq!(formatter.max_results_display, 50);
    }

    #[test]
    fn test_formatter_creation_no_colors() {
        let formatter = create_test_formatter(false);
        assert!(!formatter.colors_enabled);
    }

    #[test]
    fn test_set_max_results() {
        let mut formatter = create_test_formatter(true);
        formatter.set_max_results(100);
        assert_eq!(formatter.max_results_display, 100);
    }

    #[test]
    fn test_format_header_with_colors() {
        let formatter = create_test_formatter(true);
        let result = formatter.format_header("Test Header");
        assert!(result.contains("Test Header"));
    }

    #[test]
    fn test_format_header_no_colors() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_header("Test Header");
        assert_eq!(result, "=== Test Header ===");
    }

    #[test]
    fn test_format_success_with_colors() {
        let formatter = create_test_formatter(true);
        let result = formatter.format_success("Success message");
        assert!(result.contains("Success message"));
    }

    #[test]
    fn test_format_success_no_colors() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_success("Success message");
        assert_eq!(result, "[OK] Success message");
    }

    #[test]
    fn test_format_info_with_colors() {
        let formatter = create_test_formatter(true);
        let result = formatter.format_info("Info message");
        assert!(result.contains("Info message"));
    }

    #[test]
    fn test_format_info_no_colors() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_info("Info message");
        assert_eq!(result, "[INFO] Info message");
    }

    #[test]
    fn test_format_warning_with_colors() {
        let formatter = create_test_formatter(true);
        let result = formatter.format_warning("Warning message");
        assert!(result.contains("Warning message"));
    }

    #[test]
    fn test_format_warning_no_colors() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_warning("Warning message");
        assert_eq!(result, "[WARN] Warning message");
    }

    #[test]
    fn test_format_error_with_colors() {
        let formatter = create_test_formatter(true);
        let result = formatter.format_error("Error message");
        assert!(result.contains("Error message"));
    }

    #[test]
    fn test_format_error_no_colors() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_error("Error message");
        assert_eq!(result, "[ERROR] Error message");
    }

    #[test]
    fn test_format_error_with_suggestions() {
        let formatter = create_test_formatter(false);
        let suggestions = vec![
            Suggestion {
                message: "Try this command".to_string(),
                command: Some("loregrep --help".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Command,
            },
            Suggestion {
                message: "Check configuration".to_string(),
                command: None,
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Configuration,
            },
        ];
        
        let result = formatter.format_error_with_suggestions("Test error", &suggestions);
        assert!(result.contains("Test error"));
        assert!(result.contains("Try this command"));
        assert!(result.contains("Check configuration"));
        assert!(result.contains("loregrep --help"));
    }

    #[test]
    fn test_format_error_with_no_suggestions() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_error_with_suggestions("Test error", &[]);
        assert_eq!(result, "[ERROR] Test error");
    }

    #[test]
    fn test_format_function_signature() {
        let formatter = create_test_formatter(false);
        let params = vec!["x: i32".to_string(), "y: String".to_string()];
        let result = formatter.format_function_signature("test_func", &params, Some("bool"));
        
        assert!(result.contains("fn test_func"));
        assert!(result.contains("x: i32"));
        assert!(result.contains("y: String"));
        assert!(result.contains("-> bool"));
    }

    #[test]
    fn test_format_function_signature_no_params() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_function_signature("test_func", &[], None);
        
        assert!(result.contains("fn test_func"));
        assert!(result.contains("()"));
        assert!(!result.contains("->"));
    }

    #[test]
    fn test_format_struct_signature() {
        let formatter = create_test_formatter(false);
        let fields = vec!["name: String".to_string(), "age: u32".to_string()];
        let result = formatter.format_struct_signature("Person", &fields);
        
        assert!(result.contains("struct Person"));
        assert!(result.contains("name: String"));
        assert!(result.contains("age: u32"));
    }

    #[test]
    fn test_format_struct_signature_no_fields() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_struct_signature("Empty", &[]);
        
        assert_eq!(result, "struct Empty");
    }

    #[test]
    fn test_format_file_location() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_file_location("src/main.rs", Some(42));
        assert_eq!(result, "src/main.rs:42");
    }

    #[test]
    fn test_format_file_location_no_line() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_file_location("src/main.rs", None);
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn test_format_search_results_empty() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_search_results(&[], "test query");
        assert!(result.contains("No results found"));
        assert!(result.contains("test query"));
    }

    #[test]
    fn test_format_search_results_single() {
        let formatter = create_test_formatter(false);
        let results = vec![
            SearchResult::new(
                "function".to_string(),
                "fn test()".to_string(),
                "src/main.rs".to_string(),
                Some(10),
            ),
        ];
        
        let result = formatter.format_search_results(&results, "test");
        assert!(result.contains("Found 1 result"));
        assert!(result.contains("fn test()"));
        assert!(result.contains("src/main.rs:10"));
    }

    #[test]
    fn test_format_search_results_multiple() {
        let formatter = create_test_formatter(false);
        let results = vec![
            SearchResult::new(
                "function".to_string(),
                "fn test1()".to_string(),
                "src/main.rs".to_string(),
                Some(10),
            ),
            SearchResult::new(
                "function".to_string(),
                "fn test2()".to_string(),
                "src/lib.rs".to_string(),
                Some(20),
            ),
        ];
        
        let result = formatter.format_search_results(&results, "test");
        assert!(result.contains("Found 2 results"));
        assert!(result.contains("fn test1()"));
        assert!(result.contains("fn test2()"));
    }

    #[test]
    fn test_format_search_results_with_context() {
        let formatter = create_test_formatter(false);
        let results = vec![
            SearchResult::new(
                "function".to_string(),
                "fn test()".to_string(),
                "src/main.rs".to_string(),
                Some(10),
            ).with_context("Helper function".to_string()),
        ];
        
        let result = formatter.format_search_results(&results, "test");
        assert!(result.contains("Helper function"));
    }

    #[test]
    fn test_format_analysis_summary() {
        let formatter = create_test_formatter(false);
        let duration = std::time::Duration::from_millis(500);
        let result = formatter.format_analysis_summary(5, 10, 3, duration);
        
        assert!(result.contains("Analysis Complete"));
        assert!(result.contains("Files: 5"));
        assert!(result.contains("Functions: 10"));
        assert!(result.contains("Structs: 3"));
        assert!(result.contains("Duration:"));
    }

    #[test]
    fn test_format_ai_response_no_colors() {
        let formatter = create_test_formatter(false);
        let response = "This is a test response\nwith multiple lines";
        let result = formatter.format_ai_response(response);
        assert_eq!(result, response);
    }

    #[test]
    fn test_format_ai_response_with_code_blocks() {
        let formatter = create_test_formatter(true);
        let response = "Here's some code:\n```rust\nfn main() {}\n```\nThat's it!";
        let result = formatter.format_ai_response(response);
        
        assert!(result.contains("fn main() {}"));
        assert!(result.contains("```rust"));
    }

    #[test]
    fn test_format_config_table_empty() {
        let formatter = create_test_formatter(false);
        let result = formatter.format_config_table(&[]);
        assert!(result.contains("No configuration items"));
    }

    #[test]
    fn test_format_config_table_with_items() {
        let formatter = create_test_formatter(false);
        let items = vec![
            ("key1".to_string(), "value1".to_string()),
            ("longer_key".to_string(), "value2".to_string()),
        ];
        
        let result = formatter.format_config_table(&items);
        assert!(result.contains("key1"));
        assert!(result.contains("value1"));
        assert!(result.contains("longer_key"));
        assert!(result.contains("value2"));
    }

    #[test]
    fn test_search_result_creation() {
        let result = SearchResult::new(
            "function".to_string(),
            "fn test()".to_string(),
            "src/main.rs".to_string(),
            Some(42),
        );
        
        assert_eq!(result.result_type, "function");
        assert_eq!(result.content, "fn test()");
        assert_eq!(result.file_path, "src/main.rs");
        assert_eq!(result.line, Some(42));
        assert!(result.context.is_none());
        assert!(result.score.is_none());
    }

    #[test]
    fn test_search_result_with_context() {
        let result = SearchResult::new(
            "function".to_string(),
            "fn test()".to_string(),
            "src/main.rs".to_string(),
            Some(42),
        ).with_context("Test function".to_string());
        
        assert_eq!(result.context, Some("Test function".to_string()));
    }

    #[test]
    fn test_search_result_with_score() {
        let result = SearchResult::new(
            "function".to_string(),
            "fn test()".to_string(),
            "src/main.rs".to_string(),
            Some(42),
        ).with_score(0.95);
        
        assert_eq!(result.score, Some(0.95));
    }

    #[test]
    fn test_search_result_chaining() {
        let result = SearchResult::new(
            "function".to_string(),
            "fn test()".to_string(),
            "src/main.rs".to_string(),
            Some(42),
        )
        .with_context("Test function".to_string())
        .with_score(0.95);
        
        assert_eq!(result.context, Some("Test function".to_string()));
        assert_eq!(result.score, Some(0.95));
    }

    #[test]
    fn test_markdown_headers() {
        let formatter = create_test_formatter(false);
        
        let markdown = "# Header 1\n## Header 2\n### Header 3";
        let result = formatter.format_ai_response(markdown);
        
        assert!(result.contains(" Header 1"));
        assert!(result.contains("  Header 2"));
        assert!(result.contains("   Header 3"));
    }

    #[test]
    fn test_markdown_code_blocks() {
        let formatter = create_test_formatter(false);
        
        let markdown = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let result = formatter.format_ai_response(markdown);
        
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main()"));
        assert!(result.contains("```"));
    }

    #[test]
    fn test_markdown_lists() {
        let formatter = create_test_formatter(false);
        
        let markdown = "- Item 1\n- Item 2\n1. Numbered 1\n2. Numbered 2";
        let result = formatter.format_ai_response(markdown);
        
        assert!(result.contains("  - Item 1"));
        assert!(result.contains("  - Item 2"));
        assert!(result.contains("  1. Numbered 1"));
        assert!(result.contains("  2. Numbered 2"));
    }

    #[test]
    fn test_markdown_blockquotes() {
        let formatter = create_test_formatter(false);
        
        let markdown = "> This is a quote\n> Second line";
        let result = formatter.format_ai_response(markdown);
        
        assert!(result.contains("| This is a quote"));
        assert!(result.contains("| Second line"));
    }

    #[test]
    fn test_markdown_horizontal_rules() {
        let formatter = create_test_formatter(false);
        
        let markdown = "---\nSome text\n***";
        let result = formatter.format_ai_response(markdown);
        
        // Should contain horizontal rule representations
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.iter().any(|line| line.contains("-".repeat(50).as_str())));
    }

    #[test]
    fn test_markdown_inline_formatting() {
        let formatter = create_test_formatter(false);
        
        // Test that inline formatting is stripped in plain text mode
        let markdown = "This is **bold** and *italic* and `code`";
        let result = formatter.format_ai_response(markdown);
        
        // In plain text mode, formatting should be stripped
        assert!(result.contains("This is bold and italic and code"));
    }

    #[test]
    fn test_markdown_tables() {
        let formatter = create_test_formatter(false);
        
        let markdown = "| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |";
        let result = formatter.format_ai_response(markdown);
        
        // Should contain table elements
        assert!(result.contains("Header 1"));
        assert!(result.contains("Header 2"));
        assert!(result.contains("Cell 1"));
        assert!(result.contains("Cell 2"));
    }

    #[test]
    fn test_markdown_mixed_content() {
        let formatter = create_test_formatter(false);
        
        let markdown = r#"# Main Header

This is regular text with **bold** formatting.

## Code Example

```rust
fn hello() {
    println!("Hello, world!");
}
```

### List of Items

- First item
- Second item
  
> Important note: This is a blockquote

---

Final paragraph."#;

        let result = formatter.format_ai_response(markdown);
        
        // Should contain various elements
        assert!(result.contains(" Main Header"));
        assert!(result.contains("  Code Example"));
        assert!(result.contains("```rust"));
        assert!(result.contains("  - First item"));
        assert!(result.contains("| Important note"));
        assert!(result.contains("Final paragraph"));
    }

    #[test]
    fn test_markdown_with_colors_enabled() {
        let formatter = create_test_formatter(true);
        
        let markdown = "# Header\n**bold text**\n`code`";
        let result = formatter.format_ai_response(markdown);
        
        // With colors enabled, the result should contain ANSI color codes
        // We can't test exact color codes without knowing the theme, but we can test
        // that the content is present and the result is different from plain text
        assert!(result.contains("Header"));
        assert!(result.len() > markdown.len()); // Should be longer due to formatting codes
    }

    #[test]
    fn test_markdown_empty_and_whitespace() {
        let formatter = create_test_formatter(false);
        
        // Test empty content
        let result = formatter.format_ai_response("");
        assert_eq!(result.trim(), "");
        
        // Test whitespace-only content
        let result = formatter.format_ai_response("   \n  \n   ");
        assert!(result.trim().is_empty() || result.chars().all(|c| c.is_whitespace()));
    }
}

use super::themes::ColorTheme;
use anyhow::Result;
use std::io::{self, Write};
use console::{Key, Term};

#[derive(Debug, Clone)]
pub enum PromptResult {
    Selected(usize),
    Text(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct QueryAmbiguity {
    pub query: String,
    pub possible_meanings: Vec<String>,
    pub suggestions: Vec<String>,
    pub context: Option<String>,
}

pub struct InteractivePrompts {
    theme: ColorTheme,
    term: Term,
}

impl InteractivePrompts {
    pub fn new(theme: ColorTheme) -> Self {
        Self {
            theme,
            term: Term::stdout(),
        }
    }

    /// Show an interactive prompt with multiple options
    pub async fn prompt_user(&self, message: &str, options: &[&str]) -> Result<PromptResult> {
        if options.is_empty() {
            return Ok(PromptResult::Cancelled);
        }

        self.clear_screen_area(options.len() + 5)?;
        
        // Display the message
        println!("{}", self.theme.format_info(message));
        println!();

        // Display options
        for (i, option) in options.iter().enumerate() {
            println!("  {}. {}", 
                self.theme.format_number(&(i + 1).to_string()),
                option
            );
        }

        println!();
        print!("{} ", self.theme.format_highlight("Select option (1-N, or 'q' to quit):"));
        io::stdout().flush()?;

        // Read user input
        loop {
            match self.term.read_key()? {
                Key::Char('q') | Key::Char('Q') => {
                    println!("{}", self.theme.format_muted("Cancelled"));
                    return Ok(PromptResult::Cancelled);
                }
                Key::Char(ch) if ch.is_ascii_digit() => {
                    let digit = ch.to_digit(10).unwrap() as usize;
                    if digit > 0 && digit <= options.len() {
                        println!("{}", ch);
                        println!("Selected: {}", self.theme.format_highlight(options[digit - 1]));
                        return Ok(PromptResult::Selected(digit - 1));
                    }
                    // Invalid number, continue loop
                }
                Key::Enter => {
                    // If only one option, select it
                    if options.len() == 1 {
                        println!();
                        println!("Selected: {}", self.theme.format_highlight(options[0]));
                        return Ok(PromptResult::Selected(0));
                    }
                }
                Key::Escape => {
                    println!("{}", self.theme.format_muted("Cancelled"));
                    return Ok(PromptResult::Cancelled);
                }
                _ => {
                    // Ignore other keys
                }
            }
        }
    }

    /// Handle query ambiguity with smart suggestions
    pub async fn handle_query_ambiguity(&self, ambiguity: QueryAmbiguity) -> Result<String> {
        self.clear_screen_area(ambiguity.possible_meanings.len() + ambiguity.suggestions.len() + 10)?;

        // Display the ambiguous query
        println!("{}", self.theme.format_warning(&format!(
            "Your query \"{}\" could mean several things:",
            ambiguity.query
        )));
        println!();

        // Show possible meanings
        if !ambiguity.possible_meanings.is_empty() {
            println!("{}", self.theme.format_info("Possible interpretations:"));
            for (i, meaning) in ambiguity.possible_meanings.iter().enumerate() {
                println!("  {}. {}", 
                    self.theme.format_number(&(i + 1).to_string()),
                    meaning
                );
            }
            println!();
        }

        // Show suggestions
        if !ambiguity.suggestions.is_empty() {
            println!("{}", self.theme.format_info("Suggested queries:"));
            let start_idx = ambiguity.possible_meanings.len();
            for (i, suggestion) in ambiguity.suggestions.iter().enumerate() {
                println!("  {}. {}", 
                    self.theme.format_number(&(start_idx + i + 1).to_string()),
                    self.theme.format_code(suggestion)
                );
            }
            println!();
        }

        // Show context if available
        if let Some(ref context) = ambiguity.context {
            println!("{}", self.theme.format_muted(&format!("Context: {}", context)));
            println!();
        }

        // Create combined options for selection
        let mut all_options: Vec<String> = ambiguity.possible_meanings.clone();
        all_options.extend(ambiguity.suggestions.clone());

        if all_options.is_empty() {
            return Ok(ambiguity.query);
        }

        // Add custom query option
        all_options.push("Enter a custom query".to_string());

        let options_refs: Vec<&str> = all_options.iter().map(|s| s.as_str()).collect();
        
        match self.prompt_user("Which option would you like?", &options_refs).await? {
            PromptResult::Selected(index) => {
                if index == all_options.len() - 1 {
                    // Custom query option selected
                    self.prompt_for_custom_query().await
                } else if index < ambiguity.possible_meanings.len() {
                    // A possible meaning was selected - convert to a better query
                    Ok(self.meaning_to_query(&ambiguity.possible_meanings[index]))
                } else {
                    // A suggestion was selected
                    let suggestion_index = index - ambiguity.possible_meanings.len();
                    Ok(ambiguity.suggestions[suggestion_index].clone())
                }
            }
            PromptResult::Cancelled => Ok(ambiguity.query),
            PromptResult::Text(text) => Ok(text),
        }
    }

    /// Prompt for a custom query
    pub async fn prompt_for_custom_query(&self) -> Result<String> {
        print!("{} ", self.theme.format_highlight("Enter your query:"));
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        let trimmed = input.trim().to_string();
        if trimmed.is_empty() {
            Ok("help".to_string()) // Default to help if empty
        } else {
            Ok(trimmed)
        }
    }

    /// Confirm a potentially destructive action
    pub async fn confirm_action(&self, message: &str, default_yes: bool) -> Result<bool> {
        let prompt = if default_yes {
            format!("{} [Y/n]", message)
        } else {
            format!("{} [y/N]", message)
        };

        print!("{} ", self.theme.format_warning(&prompt));
        io::stdout().flush()?;

        loop {
            match self.term.read_key()? {
                Key::Char('y') | Key::Char('Y') => {
                    println!("y");
                    return Ok(true);
                }
                Key::Char('n') | Key::Char('N') => {
                    println!("n");
                    return Ok(false);
                }
                Key::Enter => {
                    println!();
                    return Ok(default_yes);
                }
                Key::Escape => {
                    println!("{}", self.theme.format_muted("Cancelled"));
                    return Ok(false);
                }
                _ => {
                    // Ignore other keys
                }
            }
        }
    }

    /// Show a selection menu with search capability
    pub async fn select_from_list(&self, title: &str, items: &[String], allow_search: bool) -> Result<PromptResult> {
        if items.is_empty() {
            println!("{}", self.theme.format_warning("No items to select from"));
            return Ok(PromptResult::Cancelled);
        }

        self.clear_screen_area(items.len().min(10) + 5)?;

        println!("{}", self.theme.format_header(title));
        println!();

        let display_items: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
        let limited_items: Vec<&str> = display_items.iter().take(10).cloned().collect();

        if items.len() > 10 {
            println!("{}", self.theme.format_info(&format!(
                "Showing first 10 of {} items{}",
                items.len(),
                if allow_search { " (type to search)" } else { "" }
            )));
            println!();
        }

        if allow_search && items.len() > 10 {
            // TODO: Implement search functionality
            // For now, just show the limited list
            self.prompt_user("Select an item:", &limited_items).await
        } else {
            self.prompt_user("Select an item:", &limited_items).await
        }
    }

    /// Show a yes/no/cancel prompt
    pub async fn yes_no_cancel(&self, message: &str) -> Result<YesNoCancel> {
        print!("{} [y/n/c] ", self.theme.format_info(message));
        io::stdout().flush()?;

        loop {
            match self.term.read_key()? {
                Key::Char('y') | Key::Char('Y') => {
                    println!("y");
                    return Ok(YesNoCancel::Yes);
                }
                Key::Char('n') | Key::Char('N') => {
                    println!("n");
                    return Ok(YesNoCancel::No);
                }
                Key::Char('c') | Key::Char('C') => {
                    println!("c");
                    return Ok(YesNoCancel::Cancel);
                }
                Key::Escape => {
                    println!("{}", self.theme.format_muted("Cancelled"));
                    return Ok(YesNoCancel::Cancel);
                }
                _ => {
                    // Ignore other keys
                }
            }
        }
    }

    // Helper methods

    fn clear_screen_area(&self, lines: usize) -> Result<()> {
        // Move cursor up and clear lines to make room for our prompt
        for _ in 0..lines {
            print!("\x1b[1A\x1b[2K");
        }
        Ok(())
    }

    fn meaning_to_query(&self, meaning: &str) -> String {
        // Convert a meaning description to a more specific query
        // This is a simple heuristic - in a real implementation,
        // you might want more sophisticated NLP
        
        if meaning.contains("function") {
            if meaning.contains("public") {
                "search functions --visibility public".to_string()
            } else if meaning.contains("private") {
                "search functions --visibility private".to_string()
            } else {
                "search functions".to_string()
            }
        } else if meaning.contains("struct") {
            "search structs".to_string()
        } else if meaning.contains("import") {
            "search imports".to_string()
        } else if meaning.contains("dependency") {
            "analyze dependencies".to_string()
        } else {
            meaning.to_lowercase()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum YesNoCancel {
    Yes,
    No,
    Cancel,
}

// Helper function to create query ambiguity for common cases
pub fn create_query_ambiguity(query: &str) -> Option<QueryAmbiguity> {
    let query_lower = query.to_lowercase();
    
    if query_lower.contains("function") && query_lower.len() < 20 {
        Some(QueryAmbiguity {
            query: query.to_string(),
            possible_meanings: vec![
                "Search for function definitions".to_string(),
                "Search for function calls".to_string(),
                "Show all functions in current file".to_string(),
                "Show public functions only".to_string(),
            ],
            suggestions: vec![
                "search functions".to_string(),
                "search calls".to_string(),
                "analyze file --functions-only".to_string(),
                "search functions --visibility public".to_string(),
            ],
            context: Some("Functions can be searched by name, signature, or visibility".to_string()),
        })
    } else if query_lower.contains("struct") && query_lower.len() < 20 {
        Some(QueryAmbiguity {
            query: query.to_string(),
            possible_meanings: vec![
                "Search for struct definitions".to_string(),
                "Show struct fields".to_string(),
                "Find structs implementing traits".to_string(),
            ],
            suggestions: vec![
                "search structs".to_string(),
                "analyze struct --fields".to_string(),
                "search structs --with-traits".to_string(),
            ],
            context: Some("Structs can be searched by name, fields, or trait implementations".to_string()),
        })
    } else if query_lower.contains("import") || query_lower.contains("dependency") {
        Some(QueryAmbiguity {
            query: query.to_string(),
            possible_meanings: vec![
                "Show import statements".to_string(),
                "Analyze file dependencies".to_string(),
                "Find circular dependencies".to_string(),
            ],
            suggestions: vec![
                "search imports".to_string(),
                "analyze dependencies".to_string(),
                "scan --check-circular".to_string(),
            ],
            context: Some("Dependencies can be analyzed at file or project level".to_string()),
        })
    } else {
        None
    }
} 
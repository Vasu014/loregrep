use std::path::Path;

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub message: String,
    pub command: Option<String>,
    pub priority: SuggestionPriority,
    pub suggestion_type: SuggestionType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SuggestionPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SuggestionType {
    Command,
    Configuration,
    Installation,
    FileSystem,
    Network,
    Permission,
    Syntax,
    Alternative,
}

pub struct ErrorSuggestions;

impl ErrorSuggestions {
    pub fn new() -> Self {
        Self
    }

    /// Get suggestions for an error message
    pub fn get_suggestions(&self, error: &str, context: Option<&str>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        let error_lower = error.to_lowercase();

        // File and directory related errors
        if error_lower.contains("no such file") || error_lower.contains("file not found") {
            suggestions.extend(self.file_not_found_suggestions(error, context));
        }
        
        // Permission errors
        if error_lower.contains("permission denied") || error_lower.contains("access denied") {
            suggestions.extend(self.permission_denied_suggestions(error, context));
        }

        // API key related errors
        if error_lower.contains("api key") || error_lower.contains("authentication") {
            suggestions.extend(self.api_key_suggestions(error, context));
        }

        // Network related errors
        if error_lower.contains("connection") || error_lower.contains("network") || error_lower.contains("timeout") {
            suggestions.extend(self.network_suggestions(error, context));
        }

        // Configuration errors
        if error_lower.contains("config") || error_lower.contains("configuration") {
            suggestions.extend(self.config_suggestions(error, context));
        }

        // Repository scanning errors
        if error_lower.contains("scan") || error_lower.contains("repository") {
            suggestions.extend(self.scan_suggestions(error, context));
        }

        // Analysis errors
        if error_lower.contains("analysis") || error_lower.contains("parse") {
            suggestions.extend(self.analysis_suggestions(error, context));
        }

        // Search related errors
        if error_lower.contains("search") || error_lower.contains("query") {
            suggestions.extend(self.search_suggestions(error, context));
        }

        // Dependency errors
        if error_lower.contains("dependency") || error_lower.contains("import") {
            suggestions.extend(self.dependency_suggestions(error, context));
        }

        // Generic command suggestions
        if suggestions.is_empty() {
            suggestions.extend(self.generic_suggestions(error, context));
        }

        // Sort by priority
        suggestions.sort_by(|a, b| {
            match (a.priority, b.priority) {
                (SuggestionPriority::High, SuggestionPriority::High) => std::cmp::Ordering::Equal,
                (SuggestionPriority::High, _) => std::cmp::Ordering::Less,
                (_, SuggestionPriority::High) => std::cmp::Ordering::Greater,
                (SuggestionPriority::Medium, SuggestionPriority::Medium) => std::cmp::Ordering::Equal,
                (SuggestionPriority::Medium, _) => std::cmp::Ordering::Less,
                (_, SuggestionPriority::Medium) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            }
        });

        // Limit to top 5 suggestions
        suggestions.truncate(5);
        suggestions
    }

    fn file_not_found_suggestions(&self, error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        // Try to extract file path from error
        let file_path = self.extract_file_path(error);

        if let Some(path) = file_path {
            // Check if it's a common typo or case issue
            if let Some(parent) = Path::new(&path).parent() {
                suggestions.push(Suggestion {
                    message: format!("Check if the file exists in the {} directory", parent.display()),
                    command: Some(format!("ls -la {}", parent.display())),
                    priority: SuggestionPriority::High,
                    suggestion_type: SuggestionType::FileSystem,
                });
            }

            // Suggest creating the file if it's a config file
            if path.contains("config") || path.ends_with(".toml") || path.ends_with(".json") {
                suggestions.push(Suggestion {
                    message: "Create a configuration file with default settings".to_string(),
                    command: Some("loregrep config --init".to_string()),
                    priority: SuggestionPriority::High,
                    suggestion_type: SuggestionType::Configuration,
                });
            }

            // Suggest scanning if it's a source file
            if path.ends_with(".rs") || path.ends_with(".py") || path.ends_with(".ts") {
                suggestions.push(Suggestion {
                    message: "Run a repository scan to find available files".to_string(),
                    command: Some("loregrep scan .".to_string()),
                    priority: SuggestionPriority::Medium,
                    suggestion_type: SuggestionType::Command,
                });
            }
        }

        // General file not found suggestions
        suggestions.push(Suggestion {
            message: "Verify the file path is correct and the file exists".to_string(),
            command: Some("pwd && ls -la".to_string()),
            priority: SuggestionPriority::Medium,
            suggestion_type: SuggestionType::FileSystem,
        });

        suggestions
    }

    fn permission_denied_suggestions(&self, _error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        vec![
            Suggestion {
                message: "Check file permissions".to_string(),
                command: Some("ls -la".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Permission,
            },
            Suggestion {
                message: "Try running with appropriate permissions".to_string(),
                command: Some("sudo loregrep".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Permission,
            },
            Suggestion {
                message: "Change file ownership if needed".to_string(),
                command: Some("sudo chown $USER:$USER <file>".to_string()),
                priority: SuggestionPriority::Low,
                suggestion_type: SuggestionType::Permission,
            },
        ]
    }

    fn api_key_suggestions(&self, error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        if error.contains("missing") || error.contains("not found") {
            suggestions.push(Suggestion {
                message: "Set your Anthropic API key as an environment variable".to_string(),
                command: Some("export ANTHROPIC_API_KEY=your_api_key_here".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Configuration,
            });

            suggestions.push(Suggestion {
                message: "Add API key to your config file".to_string(),
                command: Some("loregrep config --set ai.api_key=your_key".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Configuration,
            });

            suggestions.push(Suggestion {
                message: "Get an API key from Anthropic Console".to_string(),
                command: Some("open https://console.anthropic.com/".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Alternative,
            });
        }

        if error.contains("invalid") || error.contains("unauthorized") {
            suggestions.push(Suggestion {
                message: "Verify your API key is correct and active".to_string(),
                command: Some("echo $ANTHROPIC_API_KEY".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Configuration,
            });

            suggestions.push(Suggestion {
                message: "Check if your API key has the required permissions".to_string(),
                command: None,
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Configuration,
            });
        }

        suggestions
    }

    fn network_suggestions(&self, error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        if error.contains("timeout") {
            suggestions.push(Suggestion {
                message: "Check your internet connection".to_string(),
                command: Some("ping google.com".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Network,
            });

            suggestions.push(Suggestion {
                message: "Increase timeout in configuration".to_string(),
                command: Some("loregrep config --set ai.timeout=60".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Configuration,
            });
        }

        if error.contains("connection refused") || error.contains("connection failed") {
            suggestions.push(Suggestion {
                message: "Check if the service is available".to_string(),
                command: Some("curl -I https://api.anthropic.com".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Network,
            });

            suggestions.push(Suggestion {
                message: "Try again in a few moments (service might be temporarily unavailable)".to_string(),
                command: None,
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Alternative,
            });
        }

        suggestions
    }

    fn config_suggestions(&self, _error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        vec![
            Suggestion {
                message: "Initialize a new configuration file".to_string(),
                command: Some("loregrep config --init".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Configuration,
            },
            Suggestion {
                message: "View current configuration".to_string(),
                command: Some("loregrep config".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Command,
            },
            Suggestion {
                message: "Reset configuration to defaults".to_string(),
                command: Some("loregrep config --reset".to_string()),
                priority: SuggestionPriority::Low,
                suggestion_type: SuggestionType::Configuration,
            },
        ]
    }

    fn scan_suggestions(&self, error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        if error.contains("empty") || error.contains("no files") {
            suggestions.push(Suggestion {
                message: "Try scanning with more inclusive patterns".to_string(),
                command: Some("loregrep scan . --include '**/*.rs' --include '**/*.py'".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Command,
            });

            suggestions.push(Suggestion {
                message: "Check if you're in the right directory".to_string(),
                command: Some("pwd && ls -la".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::FileSystem,
            });
        }

        suggestions.push(Suggestion {
            message: "Scan current directory with verbose output".to_string(),
            command: Some("loregrep scan . --verbose".to_string()),
            priority: SuggestionPriority::Medium,
            suggestion_type: SuggestionType::Command,
        });

        suggestions
    }

    fn analysis_suggestions(&self, error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        if error.contains("parse") || error.contains("syntax") {
            suggestions.push(Suggestion {
                message: "Check if the file is valid source code".to_string(),
                command: None,
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Syntax,
            });

            suggestions.push(Suggestion {
                message: "Try analyzing individual files instead".to_string(),
                command: Some("loregrep analyze <file>".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Command,
            });
        }

        suggestions.push(Suggestion {
            message: "Run analysis with error recovery enabled".to_string(),
            command: Some("loregrep analyze --continue-on-error".to_string()),
            priority: SuggestionPriority::Medium,
            suggestion_type: SuggestionType::Command,
        });

        suggestions
    }

    fn search_suggestions(&self, error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        if error.contains("no results") || error.contains("empty") {
            suggestions.push(Suggestion {
                message: "Make sure you've scanned the repository first".to_string(),
                command: Some("loregrep scan .".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Command,
            });

            suggestions.push(Suggestion {
                message: "Try a broader search query".to_string(),
                command: Some("loregrep search '*pattern*' --fuzzy".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Command,
            });

            suggestions.push(Suggestion {
                message: "Search in all types instead of specific ones".to_string(),
                command: Some("loregrep search 'pattern' --type all".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Command,
            });
        }

        suggestions
    }

    fn dependency_suggestions(&self, _error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        vec![
            Suggestion {
                message: "Analyze dependencies for a specific file".to_string(),
                command: Some("loregrep analyze <file> --dependencies".to_string()),
                priority: SuggestionPriority::High,
                suggestion_type: SuggestionType::Command,
            },
            Suggestion {
                message: "Check import statements in the file".to_string(),
                command: Some("loregrep search imports --in-file <file>".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Command,
            },
        ]
    }

    fn generic_suggestions(&self, _error: &str, _context: Option<&str>) -> Vec<Suggestion> {
        vec![
            Suggestion {
                message: "Check the help documentation".to_string(),
                command: Some("loregrep --help".to_string()),
                priority: SuggestionPriority::Medium,
                suggestion_type: SuggestionType::Command,
            },
            Suggestion {
                message: "View current configuration".to_string(),
                command: Some("loregrep config".to_string()),
                priority: SuggestionPriority::Low,
                suggestion_type: SuggestionType::Command,
            },
            Suggestion {
                message: "Run with verbose output for more details".to_string(),
                command: Some("loregrep <command> --verbose".to_string()),
                priority: SuggestionPriority::Low,
                suggestion_type: SuggestionType::Command,
            },
        ]
    }

    // Helper methods

    fn extract_file_path(&self, error: &str) -> Option<String> {
        // Simple heuristic to extract file paths from error messages
        // Look for common patterns like "file 'path' not found" or "No such file or directory: path"
        
        if let Some(start) = error.find("'") {
            if let Some(end) = error[start + 1..].find("'") {
                return Some(error[start + 1..start + 1 + end].to_string());
            }
        }
        
        if let Some(start) = error.find("\"") {
            if let Some(end) = error[start + 1..].find("\"") {
                return Some(error[start + 1..start + 1 + end].to_string());
            }
        }
        
        // Look for patterns like ": /path/to/file"
        if let Some(colon_pos) = error.find(": ") {
            let after_colon = &error[colon_pos + 2..];
            if let Some(space_pos) = after_colon.find(' ') {
                let potential_path = &after_colon[..space_pos];
                if potential_path.contains('/') || potential_path.contains('\\') {
                    return Some(potential_path.to_string());
                }
            } else if after_colon.contains('/') || after_colon.contains('\\') {
                return Some(after_colon.to_string());
            }
        }
        
        None
    }
}

impl Default for ErrorSuggestions {
    fn default() -> Self {
        Self::new()
    }
} 
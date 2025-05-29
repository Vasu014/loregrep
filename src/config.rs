use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use directories::ProjectDirs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub file_scanning: FileScanningConfig,
    pub analysis: AnalysisConfig,
    pub output: OutputConfig,
    pub ai: AiConfig,
    pub cache: CacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileScanningConfig {
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub max_file_size: u64,
    pub follow_symlinks: bool,
    pub max_depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub languages: Vec<String>,
    pub max_parallel_files: usize,
    pub timeout_seconds: u64,
    pub extract_function_calls: bool,
    pub extract_dependencies: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub colors: bool,
    pub verbose: bool,
    pub max_results: usize,
    pub truncate_lines: bool,
    pub line_numbers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub api_key: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub timeout_seconds: u64,
    pub conversation_memory: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub path: PathBuf,
    pub max_size_mb: u64,
    pub ttl_hours: u64,
}

impl Default for CliConfig {
    fn default() -> Self {
        let cache_dir = ProjectDirs::from("com", "loregrep", "loregrep")
            .map(|dirs| dirs.cache_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".loregrep_cache"));

        Self {
            file_scanning: FileScanningConfig {
                include_patterns: vec![
                    "*.rs".to_string(),
                    "*.py".to_string(),
                    "*.ts".to_string(),
                    "*.tsx".to_string(),
                    "*.js".to_string(),
                    "*.jsx".to_string(),
                    "*.go".to_string(),
                ],
                exclude_patterns: vec![
                    "target/*".to_string(),
                    "node_modules/*".to_string(),
                    "dist/*".to_string(),
                    "build/*".to_string(),
                    ".git/*".to_string(),
                    "*.test.js".to_string(),
                    "*.spec.js".to_string(),
                    "*.test.ts".to_string(),
                    "*.spec.ts".to_string(),
                ],
                max_file_size: 1024 * 1024, // 1MB
                follow_symlinks: false,
                max_depth: Some(20),
            },
            analysis: AnalysisConfig {
                languages: vec!["rust".to_string()], // Start with Rust only
                max_parallel_files: num_cpus::get(),
                timeout_seconds: 30,
                extract_function_calls: true,
                extract_dependencies: true,
            },
            output: OutputConfig {
                colors: true,
                verbose: false,
                max_results: 50,
                truncate_lines: true,
                line_numbers: true,
            },
            ai: AiConfig {
                api_key: None,
                model: "claude-3-sonnet-20240229".to_string(),
                max_tokens: 4096,
                temperature: 0.1,
                timeout_seconds: 30,
                conversation_memory: 10,
            },
            cache: CacheConfig {
                enabled: true,
                path: cache_dir,
                max_size_mb: 100,
                ttl_hours: 24,
            },
        }
    }
}

impl CliConfig {
    /// Load configuration from file, environment variables, and defaults
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let mut config = Self::default();

        // Try to load from config file
        if let Some(path) = config_path {
            config = Self::load_from_file(path)?;
        } else {
            // Try default config locations
            let default_paths = Self::default_config_paths();
            for path in default_paths {
                if path.exists() {
                    config = Self::load_from_file(&path)
                        .with_context(|| format!("Failed to load config from {:?}", path))?;
                    break;
                }
            }
        }

        // Override with environment variables
        config.apply_env_vars();

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Load configuration from TOML file
    fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;
        
        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_env_vars(&mut self) {
        // AI configuration
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            self.ai.api_key = Some(api_key);
        }
        if let Ok(model) = std::env::var("LOREGREP_AI_MODEL") {
            self.ai.model = model;
        }

        // Cache configuration
        if let Ok(cache_enabled) = std::env::var("LOREGREP_CACHE_ENABLED") {
            self.cache.enabled = cache_enabled.parse().unwrap_or(self.cache.enabled);
        }
        if let Ok(cache_path) = std::env::var("LOREGREP_CACHE_PATH") {
            self.cache.path = PathBuf::from(cache_path);
        }

        // Output configuration
        if let Ok(colors) = std::env::var("LOREGREP_COLORS") {
            self.output.colors = colors.parse().unwrap_or(self.output.colors);
        }
        if let Ok(verbose) = std::env::var("LOREGREP_VERBOSE") {
            self.output.verbose = verbose.parse().unwrap_or(self.output.verbose);
        }

        // File scanning
        if let Ok(max_file_size) = std::env::var("LOREGREP_MAX_FILE_SIZE") {
            if let Ok(size) = max_file_size.parse() {
                self.file_scanning.max_file_size = size;
            }
        }
    }

    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        // Validate AI configuration
        if self.ai.max_tokens == 0 {
            anyhow::bail!("AI max_tokens must be greater than 0");
        }
        if !(0.0..=2.0).contains(&self.ai.temperature) {
            anyhow::bail!("AI temperature must be between 0.0 and 2.0");
        }

        // Validate cache configuration
        if self.cache.enabled && self.cache.max_size_mb == 0 {
            anyhow::bail!("Cache max_size_mb must be greater than 0 when cache is enabled");
        }

        // Validate file scanning
        if self.file_scanning.max_file_size == 0 {
            anyhow::bail!("max_file_size must be greater than 0");
        }

        // Validate analysis configuration
        if self.analysis.max_parallel_files == 0 {
            anyhow::bail!("max_parallel_files must be greater than 0");
        }

        Ok(())
    }

    /// Get default configuration file paths
    fn default_config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Current directory
        paths.push(PathBuf::from("loregrep.toml"));
        paths.push(PathBuf::from(".loregrep.toml"));

        // User config directory
        if let Some(dirs) = ProjectDirs::from("com", "loregrep", "loregrep") {
            paths.push(dirs.config_dir().join("config.toml"));
        }

        // Home directory
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            paths.push(home.join(".loregrep.toml"));
        }

        paths
    }

    /// Save configuration to file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config to TOML")?;
        
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {:?}", path))?;

        Ok(())
    }

    /// Create a sample configuration file
    pub fn create_sample_config() -> String {
        let config = Self::default();
        toml::to_string_pretty(&config).unwrap_or_else(|_| "# Error generating sample config".to_string())
    }

    // Helper methods for accessing AI config in a consistent way
    pub fn anthropic_api_key(&self) -> &Option<String> {
        &self.ai.api_key
    }

    pub fn anthropic_model(&self) -> Option<String> {
        Some(self.ai.model.clone())
    }

    pub fn max_tokens(&self) -> Option<u32> {
        Some(self.ai.max_tokens)
    }

    pub fn temperature(&self) -> Option<f64> {
        Some(self.ai.temperature)
    }

    pub fn timeout_seconds(&self) -> Option<u64> {
        Some(self.ai.timeout_seconds)
    }

    pub fn conversation_memory(&self) -> Option<usize> {
        Some(self.ai.conversation_memory)
    }
} 
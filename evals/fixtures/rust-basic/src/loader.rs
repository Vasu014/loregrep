use crate::config::parse_config;
use crate::models::Config;

/// Owns the active configuration.
pub struct Loader {
    config: Config,
}

impl Loader {
    pub fn new(config: Config) -> Self {
        Loader { config }
    }

    // Reload configuration from scratch.
    pub fn load(&mut self) {
        // Real cross-file call site of the free function parse_config():
        self.config = parse_config();
    }

    pub fn describe(&self) -> String {
        String::from("loader")
    }
}

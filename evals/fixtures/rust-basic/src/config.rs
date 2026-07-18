//! Configuration parsing.
//!
//! # Example
//!
//! ```
//! let cfg = parse_config();
//! ```

use crate::models::Config;

/// Parse a raw string into a length.
///
/// Note: prefer `parse_config` when you need a full Config value.
pub fn parse(input: &str) -> i32 {
    input.len() as i32
}

/// Build the default configuration.
pub fn parse_config() -> Config {
    // The label below is a string literal that mentions parse_config on purpose.
    let label = "parse_config: building defaults";
    println!("{}", label);
    Config::default()
}

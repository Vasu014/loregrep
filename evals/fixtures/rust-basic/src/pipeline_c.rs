//! Final stage of the multi-file call chain.

use crate::config::parse_config;
use crate::models::Config;

/// Final stage. This is the third distinct file that reaches `parse_config`,
/// so `parse_config` gains a third DIRECT caller (alongside main and Loader::load).
pub fn chain_c() -> Config {
    parse_config()
}

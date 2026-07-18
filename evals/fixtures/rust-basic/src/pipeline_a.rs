//! First stage of the multi-file call chain.
//!
//! chain_a -> chain_b -> chain_c -> parse_config, one hop per file, so the
//! transitive-caller traversal has a genuine cross-file chain to walk.

use crate::models::Config;
use crate::pipeline_b::chain_b;

/// Entry stage. Delegates straight to `chain_b` in the next file.
pub fn chain_a() -> Config {
    chain_b()
}

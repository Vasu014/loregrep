//! Middle stage of the multi-file call chain.

use crate::models::Config;
use crate::pipeline_c::chain_c;

/// Middle stage. Delegates to `chain_c` in the next file.
pub fn chain_b() -> Config {
    chain_c()
}

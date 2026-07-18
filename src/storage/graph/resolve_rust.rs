//! Rust import resolver. STUB (P2-1): classifies by the analyzer's `is_external`
//! flag only. P2-4 replaces this with real resolution — a module tree built from
//! `mod` declarations plus `mod.rs`/`foo.rs` conventions, mapping `crate::a::b`,
//! `super::x`, and `self::y` `use` paths to files; external crates → `External`;
//! `pub use` re-export chains → `Unresolved` (parked).

use super::{FileSet, ImportTarget};
use crate::types::ImportStatement;

pub fn resolve_rust_import(
    import: &ImportStatement,
    _from_file: usize,
    _files: &FileSet,
) -> ImportTarget {
    // Until P2-4: `std::`/crate imports the analyzer marked external are External;
    // everything else is honestly Unresolved (never guessed at a file).
    if import.is_external {
        ImportTarget::External(import.module_path.clone())
    } else {
        ImportTarget::Unresolved(import.module_path.clone())
    }
}

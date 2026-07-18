//! Python import resolver. STUB (P2-1): classifies by the analyzer's `is_external`
//! flag only. P2-3 replaces this with real resolution — dotted module paths to
//! files under the scanned roots, packages via `__init__.py`, relative imports by
//! level counting (`from ..a import b`); site-packages → `External`.

use super::{FileSet, ImportTarget};
use crate::types::ImportStatement;

pub fn resolve_py_import(
    import: &ImportStatement,
    _from_file: usize,
    _files: &FileSet,
) -> ImportTarget {
    if import.is_external {
        ImportTarget::External(import.module_path.clone())
    } else {
        ImportTarget::Unresolved(import.module_path.clone())
    }
}

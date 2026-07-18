//! TypeScript import resolver. STUB (P2-1): classifies by the analyzer's
//! `is_external` flag only. P2-2 replaces this with real resolution — relative
//! specifiers (`./x`) probed against `x.ts` / `x.tsx` / `x/index.ts` in the
//! scanned set; bare specifiers → `External`; tsconfig path aliases →
//! `Unresolved` (parked, the status makes the gap visible in evals).

use super::{FileSet, ImportTarget};
use crate::types::ImportStatement;

pub fn resolve_ts_import(
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

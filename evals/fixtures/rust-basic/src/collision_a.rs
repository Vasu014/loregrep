//! Same-name collision fixture A.
//!
//! `SourceA::ingest` collides by NAME with `SourceB::ingest` in collision_b.rs.
//! `feed_a` calls it through a receiver typed `&SourceA`, so today (name-keyed
//! call graph) `trace_callers("ingest")` cannot tell which `ingest` a caller
//! targets and must flag `feed_a` as name_ambiguous. The typed receiver is what
//! lets Phase 3 method resolution later flip this to an exact edge (see the
//! P3-7 eval flip). `drive_a` has a unique name and resolves exactly even now.

pub struct SourceA;

impl SourceA {
    /// Terminal method; the ambiguity target.
    pub fn ingest(&self) {
        // no-op
    }
}

/// Direct, name-ambiguous caller of `ingest` (receiver typed `&SourceA`).
pub fn feed_a(s: &SourceA) {
    s.ingest();
}

/// Upstream of `feed_a`. Unique name, so it stays an exact caller.
pub fn drive_a(s: &SourceA) {
    feed_a(s);
}

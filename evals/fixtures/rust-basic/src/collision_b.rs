//! Same-name collision fixture B. Mirror of collision_a.rs.
//!
//! `SourceB::ingest` shares the name `ingest` with `SourceA::ingest`. `feed_b`
//! is its direct, name-ambiguous caller; `drive_b` is the unique-named upstream
//! that resolves exactly. The two chains are disjoint: tracing `ingest` must not
//! let one chain's deeper callers leak into the other as exact.

pub struct SourceB;

impl SourceB {
    /// Terminal method; the ambiguity target.
    pub fn ingest(&self) {
        // no-op
    }
}

/// Direct, name-ambiguous caller of `ingest` (receiver typed `&SourceB`).
pub fn feed_b(s: &SourceB) {
    s.ingest();
}

/// Upstream of `feed_b`. Unique name, so it stays an exact caller.
pub fn drive_b(s: &SourceB) {
    feed_b(s);
}

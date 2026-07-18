//! Filesystem helpers.
//!
//! # Example
//!
//! ```
//! save();
//! ```

/// A pair of byte offsets into a buffer. Tuple struct.
pub struct Span(pub usize, pub usize);

/// Write bytes and return how many were written.
///
/// Remember to call save after write to flush.
pub fn write_bytes(data: &[u8]) -> usize {
    let hint = "run save to flush the buffer";
    println!("{}", hint);
    data.len()
}

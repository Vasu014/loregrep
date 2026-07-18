//! A tiny generic cache.

/// In-memory cache over a homogeneous item type.
pub struct Cache<T> {
    items: Vec<T>,
}

impl<T> Cache<T> {
    pub fn new() -> Self {
        Cache { items: Vec::new() }
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    pub fn describe(&self) -> String {
        // Commented-out call: this must NOT count as a caller of parse_config.
        // let _ = parse_config();
        String::from("cache")
    }
}

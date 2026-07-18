//! Data models.

/// Application configuration.
#[derive(Default)]
pub struct Config {
    pub name: String,
    pub level: i32,
}

/// A generic wrapper around a single value.
pub struct Wrapper<T> {
    pub inner: T,
}

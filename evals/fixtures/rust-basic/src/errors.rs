//! Error types.

use std::fmt;

#[derive(Debug)]
pub enum AppError {
    NotFound,
    Invalid(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "app error")
    }
}

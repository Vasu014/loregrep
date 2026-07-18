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

/// A trait, so search_structs can distinguish `kind: trait` from struct/enum
/// (P1-6 eval coverage). Implemented for AppError below.
pub trait Summarize {
    fn summarize(&self) -> String;
}

impl Summarize for AppError {
    fn summarize(&self) -> String {
        String::from("summary")
    }
}

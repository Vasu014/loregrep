use thiserror::Error;

/// Main error type for the LoreGrep public API
#[derive(Debug, Error)]
pub enum LoreGrepError {
    #[error("Repository not scanned")]
    NotScanned,
    
    #[error("Analysis error: {0}")]
    AnalysisError(String),
    
    #[error("Tool execution error: {0}")]
    ToolError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<crate::types::AnalysisError> for LoreGrepError {
    fn from(err: crate::types::AnalysisError) -> Self {
        LoreGrepError::AnalysisError(err.to_string())
    }
}

/// Public Result type alias for the LoreGrep API
pub type Result<T> = std::result::Result<T, LoreGrepError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_display() {
        let error = LoreGrepError::NotScanned;
        assert_eq!(error.to_string(), "Repository not scanned");
        
        let error = LoreGrepError::AnalysisError("test error".to_string());
        assert_eq!(error.to_string(), "Analysis error: test error");
        
        let error = LoreGrepError::ToolError("invalid tool".to_string());
        assert_eq!(error.to_string(), "Tool execution error: invalid tool");
    }

    #[test]
    fn test_error_from_io() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let loregrep_error: LoreGrepError = io_error.into();
        assert!(loregrep_error.to_string().contains("file not found"));
    }

    #[test]
    fn test_error_from_json() {
        let json_error = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let loregrep_error: LoreGrepError = json_error.into();
        assert!(loregrep_error.to_string().contains("JSON error"));
    }
}
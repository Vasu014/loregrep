use thiserror::Error;

#[derive(Error, Debug)]
pub enum AnalysisError {
    #[error("Failed to parse file: {message}")]
    ParseError { message: String },

    #[error("Unsupported language: {language}")]
    UnsupportedLanguage { language: String },

    #[error("Invalid file extension: {extension}")]
    InvalidFileExtension { extension: String },

    #[error("Tree-sitter query error: {message}")]
    QueryError { message: String },

    #[error("IO error: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },

    #[error("Serialization error: {source}")]
    SerializationError {
        #[from]
        source: serde_json::Error,
    },

    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Language analyzer not found for extension: {extension}")]
    AnalyzerNotFound { extension: String },

    #[error("IO error: {0}")]
    Io(String),

    #[error("General error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AnalysisError>; 
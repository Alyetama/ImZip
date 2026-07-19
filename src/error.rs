use thiserror::Error;

pub type Result<T> = std::result::Result<T, ImzipError>;

#[derive(Error, Debug)]
pub enum ImzipError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to decode image: {0}")]
    Decode(String),

    #[error("failed to encode {format}: {msg}")]
    Encode { format: &'static str, msg: String },

    #[error("metadata error: {0}")]
    Metadata(String),

    #[error("{0}")]
    Invalid(String),
}

impl ImzipError {
    pub fn encode(format: &'static str, msg: impl std::fmt::Display) -> Self {
        ImzipError::Encode {
            format,
            msg: msg.to_string(),
        }
    }

    pub fn decode(msg: impl std::fmt::Display) -> Self {
        ImzipError::Decode(msg.to_string())
    }

    pub fn metadata(msg: impl std::fmt::Display) -> Self {
        ImzipError::Metadata(msg.to_string())
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        ImzipError::Invalid(msg.into())
    }
}

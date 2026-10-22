use serde::{Serialize, Serializer};

/// Every failure that can reach the frontend.
///
/// The variants carry messages written for the person using the app: what
/// failed and what to do about it. The frontend shows them verbatim, so they
/// must never contain a stack trace, a SQL fragment, or a token.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialisation error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Notion(String),

    #[error("{0}")]
    Media(String),
}

impl AppError {
    /// A failure with a message the user is meant to read.
    pub fn msg(text: impl Into<String>) -> Self {
        AppError::Message(text.into())
    }

    pub fn notion(text: impl Into<String>) -> Self {
        AppError::Notion(text.into())
    }

    pub fn media(text: impl Into<String>) -> Self {
        AppError::Media(text.into())
    }
}

/// `AppError` crosses the Tauri boundary as a plain string. Serialising it by
/// hand keeps the frontend contract stable if the enum grows a variant.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

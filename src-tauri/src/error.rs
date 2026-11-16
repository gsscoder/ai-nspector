//! One error type for all commands. The frontend receives its text as a plain string.

use std::error::Error;
use std::io;

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}: {1}")]
    Io(String, #[source] io::Error),
    #[error("{0}: {1}")]
    Json(String, #[source] serde_json::Error),
    /// Runtime, credential store or directory walker failure.
    #[error("{0}: {1}")]
    Platform(String, #[source] Box<dyn Error + Send + Sync>),
    /// The LLM server or its answer is not usable.
    #[error("{0}")]
    Llm(String),
    /// Input or state that breaks a rule; the text says which one.
    #[error("{0}")]
    Invalid(String),
    #[error("Cancelled")]
    Cancelled,
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

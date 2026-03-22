//! Stiva error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StivaError {
    #[error("image not found: {0}")]
    ImageNotFound(String),

    #[error("image pull failed: {0}")]
    PullFailed(String),

    #[error("container not found: {0}")]
    ContainerNotFound(String),

    #[error("container already running: {0}")]
    AlreadyRunning(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("registry error: {0}")]
    Registry(String),

    #[error("invalid image reference: {0}")]
    InvalidReference(String),

    #[error("sandbox error: {0}")]
    Sandbox(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

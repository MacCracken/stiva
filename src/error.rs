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

    #[error("invalid state: {0}")]
    InvalidState(String),

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

    #[error("digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("compose error: {0}")]
    Compose(String),

    #[error("sandbox error: {0}")]
    Sandbox(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages() {
        let e = StivaError::ImageNotFound("nginx".into());
        assert_eq!(e.to_string(), "image not found: nginx");

        let e = StivaError::ContainerNotFound("abc123".into());
        assert_eq!(e.to_string(), "container not found: abc123");

        let e = StivaError::AlreadyRunning("abc123".into());
        assert_eq!(e.to_string(), "container already running: abc123");

        let e = StivaError::InvalidState("bad transition".into());
        assert_eq!(e.to_string(), "invalid state: bad transition");

        let e = StivaError::DigestMismatch {
            expected: "sha256:aaa".into(),
            actual: "sha256:bbb".into(),
        };
        assert_eq!(
            e.to_string(),
            "digest mismatch: expected sha256:aaa, got sha256:bbb"
        );

        let e = StivaError::AuthFailed("bad creds".into());
        assert_eq!(e.to_string(), "authentication failed: bad creds");

        let e = StivaError::UnsupportedPlatform("windows/arm".into());
        assert_eq!(e.to_string(), "unsupported platform: windows/arm");

        let e = StivaError::Compose("missing services".into());
        assert_eq!(e.to_string(), "compose error: missing services");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e: StivaError = io_err.into();
        assert!(matches!(e, StivaError::Io(_)));
        assert!(e.to_string().contains("gone"));
    }

    #[test]
    fn from_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("bad json").unwrap_err();
        let e: StivaError = json_err.into();
        assert!(matches!(e, StivaError::Json(_)));
    }

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StivaError>();
    }
}

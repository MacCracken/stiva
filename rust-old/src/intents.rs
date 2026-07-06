//! Agnoshi intent system — stub for future NL→intent parsing.
//!
//! The agnoshi project does not exist yet. This module defines the intent types
//! that stiva will support when NL intent parsing is available.

use crate::error::StivaError;
use serde::{Deserialize, Serialize};

/// A parsed container intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Intent {
    /// Run a container from an image.
    Run { image: String, name: Option<String> },
    /// Stop a running container.
    Stop { id: String },
    /// Pull an image from a registry.
    Pull { image: String },
    /// Deploy an ansamblu file.
    Ansamblu { action: AnsambluAction },
    /// Scale a service to N replicas.
    Scale { service: String, replicas: u32 },
    /// Inspect a container or image.
    Inspect { target: String },
}

/// Ansamblu sub-action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AnsambluAction {
    Up,
    Down,
    Restart,
}

/// Parse a natural language intent into a structured Intent.
///
/// This is a placeholder — actual NL parsing requires the agnoshi project.
#[must_use = "parsing returns a new Intent"]
pub fn parse_intent(_text: &str) -> Result<Intent, StivaError> {
    Err(StivaError::Runtime(
        "agnoshi intent parsing not yet implemented".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_intent_not_implemented() {
        assert!(parse_intent("run nginx").is_err());
    }

    #[test]
    fn intent_serde() {
        let intents = vec![
            Intent::Run {
                image: "nginx".into(),
                name: Some("web".into()),
            },
            Intent::Stop {
                id: "abc123".into(),
            },
            Intent::Pull {
                image: "alpine".into(),
            },
            Intent::Ansamblu {
                action: AnsambluAction::Up,
            },
            Intent::Scale {
                service: "worker".into(),
                replicas: 3,
            },
            Intent::Inspect {
                target: "nginx".into(),
            },
        ];
        for intent in intents {
            let json = serde_json::to_string(&intent).unwrap();
            let back: Intent = serde_json::from_str(&json).unwrap();
            assert_eq!(intent, back);
        }
    }

    #[test]
    fn ansamblu_action_serde() {
        for action in [
            AnsambluAction::Up,
            AnsambluAction::Down,
            AnsambluAction::Restart,
        ] {
            let json = serde_json::to_string(&action).unwrap();
            let back: AnsambluAction = serde_json::from_str(&json).unwrap();
            assert_eq!(action, back);
        }
    }
}

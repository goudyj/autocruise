//! Tool error contract.
//!
//! Failed tool calls return `isError: true` with a single text block holding
//! JSON: `{"error": {"kind": "not_found", "message": "..."}}`. `kind` values
//! come from [`autocruise_core::error::ErrorKind::as_str`] and are stable.

use std::fmt::Display;

use autocruise_core::error::{Classify, ErrorKind};
use rmcp::model::{ContentBlock, IntoContents};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ToolError {
    pub fn from_err<E: Classify + Display>(error: E) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: message.into(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({ "error": { "kind": self.kind.as_str(), "message": self.message } })
    }
}

impl IntoContents for ToolError {
    fn into_contents(self) -> Vec<ContentBlock> {
        self.to_json().to_string().into_contents()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Boom;
    impl Classify for Boom {
        fn kind(&self) -> ErrorKind {
            ErrorKind::NotFound
        }
    }
    impl Display for Boom {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("nothing here")
        }
    }

    #[test]
    fn json_contract() {
        assert_eq!(
            ToolError::from_err(Boom).to_json(),
            json!({ "error": { "kind": "not_found", "message": "nothing here" } })
        );
    }
}

use serde_json::Value;
use thiserror::Error;

/// Convenience `Result` alias using [`AnkiError`] as the default error type.
pub type Result<T, E = AnkiError> = std::result::Result<T, E>;

/// Structured errors returned by the `anki-connect-client` library.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AnkiError {
    /// HTTP transport or connection failure from `reqwest`.
    #[error("Transport error: {0}")]
    Transport(#[from] reqwest::Error),

    /// Non-success HTTP status returned by the Anki-Connect server.
    #[error("HTTP {status}: {message}")]
    HttpStatus { status: u16, message: String },

    /// JSON-RPC 2.0 error object returned by the server.
    #[error("JSON-RPC error {code}: {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },

    /// Asynchronous operation terminated in a `failed` or `cancelled` state.
    #[error("Operation {operation_id} {state}: {message}")]
    OperationFailed {
        operation_id: String,
        state: String,
        message: String,
        error: Option<Value>,
    },

    /// Timed out waiting for an asynchronous operation to complete.
    #[error("Timed out waiting for operation {operation_id} after {timeout_secs}s")]
    Timeout {
        operation_id: String,
        timeout_secs: u64,
    },

    /// Invalid parameters or client-side validation failure.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Requested method or definition was not found in the embedded schema.
    #[error("Schema lookup failed: {0}")]
    SchemaNotFound(String),

    /// JSON serialization or deserialization failure.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl AnkiError {
    /// Returns `true` if this error is a JSON-RPC 2.0 error returned by the server.
    pub fn is_rpc_error(&self) -> bool {
        matches!(self, Self::Rpc { .. })
    }

    /// Returns the JSON-RPC error code if this is an [`AnkiError::Rpc`] error.
    pub fn rpc_code(&self) -> Option<i64> {
        match self {
            Self::Rpc { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// Returns the HTTP status code if this is an [`AnkiError::HttpStatus`] error.
    pub fn http_status(&self) -> Option<u16> {
        match self {
            Self::HttpStatus { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Returns `true` if this error is an operation polling timeout.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn display_formatting_matches_expected_messages() {
        let http = AnkiError::HttpStatus {
            status: 401,
            message: "Unauthorized".into(),
        };
        assert_eq!(http.to_string(), "HTTP 401: Unauthorized");

        let rpc = AnkiError::Rpc {
            code: -32602,
            message: "Invalid params".into(),
            data: Some(json!({"field": "deck"})),
        };
        assert_eq!(rpc.to_string(), "JSON-RPC error -32602: Invalid params");

        let op = AnkiError::OperationFailed {
            operation_id: "op-1".into(),
            state: "failed".into(),
            message: "boom".into(),
            error: None,
        };
        assert_eq!(op.to_string(), "Operation op-1 failed: boom");

        let timeout = AnkiError::Timeout {
            operation_id: "op-2".into(),
            timeout_secs: 30,
        };
        assert_eq!(
            timeout.to_string(),
            "Timed out waiting for operation op-2 after 30s"
        );

        let invalid = AnkiError::InvalidInput("missing field".into());
        assert_eq!(invalid.to_string(), "Invalid input: missing field");

        let schema = AnkiError::SchemaNotFound("unknown.method".into());
        assert_eq!(schema.to_string(), "Schema lookup failed: unknown.method");

        let json_err: AnkiError = serde_json::from_str::<Value>("{invalid")
            .unwrap_err()
            .into();
        assert!(json_err.to_string().starts_with("JSON serialization error:"));
    }

    #[test]
    fn helper_predicates_inspect_variants_accurately() {
        let rpc = AnkiError::Rpc {
            code: 1003,
            message: "Editor conflict.".into(),
            data: None,
        };
        assert!(rpc.is_rpc_error());
        assert_eq!(rpc.rpc_code(), Some(1003));
        assert_eq!(rpc.http_status(), None);
        assert!(!rpc.is_timeout());

        let http = AnkiError::HttpStatus {
            status: 403,
            message: "Forbidden".into(),
        };
        assert!(!http.is_rpc_error());
        assert_eq!(http.rpc_code(), None);
        assert_eq!(http.http_status(), Some(403));
        assert!(!http.is_timeout());

        let timeout = AnkiError::Timeout {
            operation_id: "op-42".into(),
            timeout_secs: 120,
        };
        assert!(!timeout.is_rpc_error());
        assert_eq!(timeout.rpc_code(), None);
        assert_eq!(timeout.http_status(), None);
        assert!(timeout.is_timeout());

        let invalid = AnkiError::InvalidInput("bad".into());
        assert!(!invalid.is_rpc_error());
        assert_eq!(invalid.rpc_code(), None);
        assert_eq!(invalid.http_status(), None);
        assert!(!invalid.is_timeout());
    }
}

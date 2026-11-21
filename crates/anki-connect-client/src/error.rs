use serde_json::{json, Value};
use thiserror::Error;

/// Structured CLI errors with deterministic exit codes and JSON serialization
/// for LLM agent consumption.
#[derive(Debug, Error)]
pub enum CliError {
    #[error("Transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("HTTP {status}: {message}")]
    HttpStatus { status: u16, message: String },

    #[error("JSON-RPC error {code}: {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },

    #[error("Operation {operation_id} {state}: {message}")]
    OperationFailed {
        operation_id: String,
        state: String,
        message: String,
        error: Option<Value>,
    },

    #[error("Timed out waiting for operation {operation_id} after {timeout_secs}s")]
    Timeout {
        operation_id: String,
        timeout_secs: u64,
    },

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Schema lookup failed: {0}")]
    SchemaNotFound(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML serialization error: {0}")]
    Yaml(String),
}

impl CliError {
    /// Exit codes help agents and shell scripts distinguish failure categories:
    /// - 2: Invalid CLI input or schema lookup failure
    /// - 3: Transport / HTTP auth / origin rejection
    /// - 4: JSON-RPC or async operation error
    /// - 5: Operation polling timeout
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::InvalidInput(_)
            | CliError::SchemaNotFound(_)
            | CliError::Json(_)
            | CliError::Yaml(_) => 2,
            CliError::Transport(_) | CliError::HttpStatus { .. } => 3,
            CliError::Rpc { .. } | CliError::OperationFailed { .. } => 4,
            CliError::Timeout { .. } => 5,
        }
    }

    /// Serialize the error as structured JSON for stderr.
    pub fn to_json(&self) -> Value {
        match self {
            CliError::Rpc {
                code,
                message,
                data,
            } => json!({
                "error": {
                    "kind": "rpc",
                    "code": code,
                    "message": message,
                    "data": data,
                }
            }),
            CliError::OperationFailed {
                operation_id,
                state,
                message,
                error,
            } => json!({
                "error": {
                    "kind": "operation",
                    "operationId": operation_id,
                    "state": state,
                    "message": message,
                    "details": error,
                }
            }),
            CliError::HttpStatus { status, message } => json!({
                "error": {
                    "kind": "http",
                    "status": status,
                    "message": message,
                }
            }),
            CliError::Timeout {
                operation_id,
                timeout_secs,
            } => json!({
                "error": {
                    "kind": "timeout",
                    "operationId": operation_id,
                    "timeoutSecs": timeout_secs,
                }
            }),
            other => json!({
                "error": {
                    "kind": "cli",
                    "message": other.to_string(),
                }
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_deterministic() {
        let json_err = serde_json::from_str::<Value>("{invalid").unwrap_err();

        assert_eq!(CliError::InvalidInput("bad".into()).exit_code(), 2);
        assert_eq!(CliError::SchemaNotFound("missing".into()).exit_code(), 2);
        assert_eq!(CliError::Json(json_err).exit_code(), 2);
        assert_eq!(CliError::Yaml("invalid yaml".into()).exit_code(), 2);

        assert_eq!(
            CliError::HttpStatus {
                status: 401,
                message: "Unauthorized".into(),
            }
            .exit_code(),
            3
        );

        assert_eq!(
            CliError::Rpc {
                code: -32602,
                message: "Invalid params".into(),
                data: None,
            }
            .exit_code(),
            4
        );
        assert_eq!(
            CliError::OperationFailed {
                operation_id: "op-1".into(),
                state: "failed".into(),
                message: "boom".into(),
                error: None,
            }
            .exit_code(),
            4
        );

        assert_eq!(
            CliError::Timeout {
                operation_id: "op-2".into(),
                timeout_secs: 30,
            }
            .exit_code(),
            5
        );
    }

    #[test]
    fn to_json_produces_expected_envelope_for_each_kind() {
        let rpc = CliError::Rpc {
            code: -32600,
            message: "Invalid Request".into(),
            data: Some(json!({"detail": "missing method"})),
        }
        .to_json();
        assert_eq!(
            rpc,
            json!({
                "error": {
                    "kind": "rpc",
                    "code": -32600,
                    "message": "Invalid Request",
                    "data": {"detail": "missing method"},
                }
            })
        );

        let operation = CliError::OperationFailed {
            operation_id: "op-9".into(),
            state: "cancelled".into(),
            message: "User cancelled".into(),
            error: Some(json!({"code": "CANCELLED"})),
        }
        .to_json();
        assert_eq!(
            operation,
            json!({
                "error": {
                    "kind": "operation",
                    "operationId": "op-9",
                    "state": "cancelled",
                    "message": "User cancelled",
                    "details": {"code": "CANCELLED"},
                }
            })
        );

        let http = CliError::HttpStatus {
            status: 403,
            message: "Forbidden".into(),
        }
        .to_json();
        assert_eq!(
            http,
            json!({
                "error": {
                    "kind": "http",
                    "status": 403,
                    "message": "Forbidden",
                }
            })
        );

        let timeout = CliError::Timeout {
            operation_id: "op-42".into(),
            timeout_secs: 120,
        }
        .to_json();
        assert_eq!(
            timeout,
            json!({
                "error": {
                    "kind": "timeout",
                    "operationId": "op-42",
                    "timeoutSecs": 120,
                }
            })
        );

        let cli_err = CliError::Yaml("invalid yaml".into());
        let expected_msg = cli_err.to_string();
        assert_eq!(
            cli_err.to_json(),
            json!({
                "error": {
                    "kind": "cli",
                    "message": expected_msg,
                }
            })
        );
    }
}

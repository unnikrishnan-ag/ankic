use anki_connect_client::AnkiError;
use serde_json::{json, Value};
use thiserror::Error;

/// Structured CLI errors with deterministic exit codes and JSON serialization
/// for LLM agent consumption.
#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Client(#[from] AnkiError),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_norway::Error),
}

impl CliError {
    /// Exit codes help agents and shell scripts distinguish failure categories:
    /// - 2: Invalid CLI input or schema lookup failure
    /// - 3: Transport / HTTP auth / origin rejection
    /// - 4: JSON-RPC or async operation error
    /// - 5: Operation polling timeout
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::InvalidInput(_) | CliError::Json(_) | CliError::Yaml(_) => 2,
            CliError::Client(err) => match err {
                AnkiError::InvalidInput(_)
                | AnkiError::SchemaNotFound(_)
                | AnkiError::Json(_) => 2,
                AnkiError::Transport(_) | AnkiError::HttpStatus { .. } => 3,
                AnkiError::Rpc { .. } | AnkiError::OperationFailed { .. } => 4,
                AnkiError::Timeout { .. } => 5,
                _ => 2,
            },
        }
    }

    /// Serialize the error as structured JSON for stderr.
    pub fn to_json(&self) -> Value {
        match self {
            CliError::Client(AnkiError::Rpc {
                code,
                message,
                data,
            }) => json!({
                "error": {
                    "kind": "rpc",
                    "code": code,
                    "message": message,
                    "data": data,
                }
            }),
            CliError::Client(AnkiError::OperationFailed {
                operation_id,
                state,
                message,
                error,
            }) => json!({
                "error": {
                    "kind": "operation",
                    "operationId": operation_id,
                    "state": state,
                    "message": message,
                    "details": error,
                }
            }),
            CliError::Client(AnkiError::HttpStatus { status, message }) => json!({
                "error": {
                    "kind": "http",
                    "status": status,
                    "message": message,
                }
            }),
            CliError::Client(AnkiError::Timeout {
                operation_id,
                timeout_secs,
            }) => json!({
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
        let yaml_err = serde_norway::from_str::<Value>(":\n  - [unclosed").unwrap_err();

        assert_eq!(CliError::InvalidInput("bad".into()).exit_code(), 2);
        assert_eq!(
            CliError::from(AnkiError::InvalidInput("bad".into())).exit_code(),
            2
        );
        assert_eq!(
            CliError::from(AnkiError::SchemaNotFound("missing".into())).exit_code(),
            2
        );
        assert_eq!(CliError::Json(json_err).exit_code(), 2);
        assert_eq!(CliError::Yaml(yaml_err).exit_code(), 2);

        assert_eq!(
            CliError::from(AnkiError::HttpStatus {
                status: 401,
                message: "Unauthorized".into(),
            })
            .exit_code(),
            3
        );

        assert_eq!(
            CliError::from(AnkiError::Rpc {
                code: -32602,
                message: "Invalid params".into(),
                data: None,
            })
            .exit_code(),
            4
        );
        assert_eq!(
            CliError::from(AnkiError::OperationFailed {
                operation_id: "op-1".into(),
                state: "failed".into(),
                message: "boom".into(),
                error: None,
            })
            .exit_code(),
            4
        );

        assert_eq!(
            CliError::from(AnkiError::Timeout {
                operation_id: "op-2".into(),
                timeout_secs: 30,
            })
            .exit_code(),
            5
        );
    }

    #[test]
    fn to_json_produces_expected_envelope_for_each_kind() {
        let rpc = CliError::from(AnkiError::Rpc {
            code: -32600,
            message: "Invalid Request".into(),
            data: Some(json!({"detail": "missing method"})),
        })
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

        let operation = CliError::from(AnkiError::OperationFailed {
            operation_id: "op-9".into(),
            state: "cancelled".into(),
            message: "User cancelled".into(),
            error: Some(json!({"code": "CANCELLED"})),
        })
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

        let http = CliError::from(AnkiError::HttpStatus {
            status: 403,
            message: "Forbidden".into(),
        })
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

        let timeout = CliError::from(AnkiError::Timeout {
            operation_id: "op-42".into(),
            timeout_secs: 120,
        })
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

        let yaml_err = serde_norway::from_str::<Value>(":\n  - [unclosed").unwrap_err();
        let cli_err = CliError::Yaml(yaml_err);
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

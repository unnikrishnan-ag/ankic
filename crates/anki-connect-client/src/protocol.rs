use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{json, Value};

use crate::error::{AnkiError, Result};

/// Default Anki-Connect API v1 endpoint URL.
pub const DEFAULT_URL: &str = "http://127.0.0.1:8765/api/v1";

/// Sorted list of all application-scoped methods in Anki-Connect API v1.
///
/// Used when the `schema` feature is disabled to determine whether a method
/// accepts a collection `session` parameter.
pub const APPLICATION_SCOPED_METHODS: &[&str] = &[
    "access.requestPermission",
    "api.describe",
    "api.execute",
    "application.exit",
    "operations.cancel",
    "operations.get",
    "profiles.getActive",
    "profiles.list",
    "profiles.switch",
];

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Shared configuration for async and blocking Anki-Connect clients.
#[derive(Clone)]
pub struct ClientConfig {
    pub(crate) url: String,
    pub(crate) api_key: Option<String>,
    pub(crate) session: Option<String>,
    pub(crate) timeout: Duration,
    pub(crate) validate: bool,
    pub(crate) poll_initial_delay: Duration,
    pub(crate) poll_max_delay: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            api_key: None,
            session: None,
            timeout: Duration::from_secs(30),
            validate: true,
            poll_initial_delay: Duration::from_millis(250),
            poll_max_delay: Duration::from_secs(2),
        }
    }
}

impl ClientConfig {
    /// Return the configured endpoint URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Return whether an API key is configured.
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Return the configured collection session token, if any.
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// Return the configured HTTP request timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Return whether pre-flight schema validation is enabled.
    pub fn validate(&self) -> bool {
        self.validate
    }

    /// Return the initial delay between `operations.get` polls.
    pub fn poll_initial_delay(&self) -> Duration {
        self.poll_initial_delay
    }

    /// Return the maximum delay between `operations.get` polls.
    pub fn poll_max_delay(&self) -> Duration {
        self.poll_max_delay
    }
}

impl fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientConfig")
            .field("url", &self.url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("session", &self.session)
            .field("timeout", &self.timeout)
            .field("validate", &self.validate)
            .field("poll_initial_delay", &self.poll_initial_delay)
            .field("poll_max_delay", &self.poll_max_delay)
            .finish()
    }
}

/// Outcome of polling an asynchronous operation via `operations.get`.
#[derive(Debug)]
pub enum OperationPollOutcome {
    /// Operation finished with `"state": "succeeded"` and produced a result value.
    Succeeded(Value),
    /// Operation finished with `"state": "failed"` or `"cancelled"`.
    Failed(AnkiError),
    /// Operation is still in progress (`"pending"`, `"running"`, etc.).
    Pending,
}

/// Generate the next monotonically increasing JSON-RPC 2.0 request ID.
pub fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// Return `true` if `method` is collection-scoped and should receive a `session` parameter.
pub fn is_collection_scoped_method(method: &str) -> bool {
    #[cfg(feature = "schema")]
    {
        crate::schema::method_scope(method) == Some("collection")
    }
    #[cfg(not(feature = "schema"))]
    {
        APPLICATION_SCOPED_METHODS.binary_search(&method).is_err()
    }
}

/// Inject `"session"` for collection-scoped methods and, when `validate` is `true`
/// and the `schema` feature is enabled, validate parameters against the embedded API v1 schema.
pub fn prepare_params(
    method: &str,
    params: Value,
    session: Option<&str>,
    validate: bool,
) -> Result<Value> {
    let params = if let Some(session) = session {
        if is_collection_scoped_method(method) {
            match params {
                Value::Object(mut map) => {
                    map.entry("session".to_string())
                        .or_insert_with(|| Value::String(session.to_string()));
                    Value::Object(map)
                }
                Value::Null => json!({ "session": session }),
                other => other,
            }
        } else {
            params
        }
    } else {
        params
    };

    #[cfg(feature = "schema")]
    if validate {
        crate::schema::validate_method_params(method, &params)?;
    }

    #[cfg(not(feature = "schema"))]
    let _ = validate;

    Ok(params)
}

/// Build a JSON-RPC 2.0 request envelope for `method` with already-prepared parameters.
pub fn build_rpc_request(method: &str, prepared_params: Value) -> (u64, Value) {
    let id = next_request_id();
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": prepared_params,
    });
    (id, body)
}

/// Prepare command items for `api.execute`.
pub fn build_execute_commands(
    commands: Vec<(String, Value)>,
    session: Option<&str>,
    validate: bool,
) -> Result<Vec<Value>> {
    let mut items = Vec::with_capacity(commands.len());
    for (method, params) in commands {
        let prepared = prepare_params(&method, params, session, validate)?;
        items.push(json!({
            "method": method,
            "params": prepared,
        }));
    }
    Ok(items)
}

/// Build a JSON-RPC 2.0 transport batch array payload (1..=100 requests).
pub fn build_transport_batch(
    requests: Vec<(String, Value)>,
    session: Option<&str>,
    validate: bool,
) -> Result<Value> {
    if requests.is_empty() || requests.len() > 100 {
        return Err(AnkiError::InvalidInput(
            "Transport batch must contain between 1 and 100 requests".into(),
        ));
    }

    let mut payload = Vec::with_capacity(requests.len());
    for (method, params) in requests {
        let prepared = prepare_params(&method, params, session, validate)?;
        let (_, body) = build_rpc_request(&method, prepared);
        payload.push(body);
    }

    Ok(Value::Array(payload))
}

/// Parse a JSON-RPC 2.0 error object into an [`AnkiError::Rpc`].
pub fn rpc_error_from_value(err: &Value) -> AnkiError {
    let code = err.get("code").and_then(Value::as_i64).unwrap_or(-32603);
    let message = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Unknown JSON-RPC error")
        .to_string();
    let data = err.get("data").cloned();
    AnkiError::Rpc {
        code,
        message,
        data,
    }
}

/// Parse a single JSON-RPC 2.0 reply and verify its response `id`.
pub fn parse_rpc_reply(reply: Value, expected_id: u64) -> Result<Value> {
    if let Some(err) = reply.get("error") {
        return Err(rpc_error_from_value(err));
    }

    if reply.get("id").and_then(Value::as_u64) != Some(expected_id) {
        return Err(AnkiError::InvalidInput(format!(
            "Unexpected JSON-RPC response id (expected {expected_id})"
        )));
    }

    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

/// Parse a JSON-RPC 2.0 transport batch reply (expecting a JSON array or top-level RPC error).
pub fn parse_transport_batch_reply(reply: Value) -> Result<Value> {
    if reply.is_array() {
        Ok(reply)
    } else if let Some(err) = reply.get("error") {
        Err(rpc_error_from_value(err))
    } else {
        Err(AnkiError::InvalidInput(
            "Expected JSON array response for transport batch".into(),
        ))
    }
}

/// Extract `operationId` from an `OperationHandle` result, if present.
pub fn extract_operation_id(result: &Value) -> Option<String> {
    let obj = result.as_object()?;
    if obj.contains_key("method") {
        obj.get("operationId")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    } else {
        None
    }
}

/// Interpret an `operations.get` response object into a terminal or pending poll outcome.
pub fn parse_operation_poll_state(operation_id: &str, op: &Value) -> OperationPollOutcome {
    let state = op
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    match state {
        "succeeded" => {
            OperationPollOutcome::Succeeded(op.get("result").cloned().unwrap_or(Value::Null))
        }
        "failed" | "cancelled" => {
            let err_val = op.get("error").cloned();
            let msg = err_val
                .as_ref()
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("Operation did not succeed")
                .to_string();
            OperationPollOutcome::Failed(AnkiError::OperationFailed {
                operation_id: operation_id.to_string(),
                state: state.to_string(),
                message: msg,
                error: err_val,
            })
        }
        _ => OperationPollOutcome::Pending,
    }
}

/// Compute the next exponential backoff polling delay (`current * 1.5`, capped at `max`).
pub fn next_poll_delay(current: Duration, max: Duration) -> Duration {
    (current.saturating_mul(3) / 2).min(max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_config_debug_redacts_api_key() {
        let cfg = ClientConfig {
            api_key: Some("super-secret-token".to_string()),
            session: Some("sess-1".to_string()),
            ..ClientConfig::default()
        };
        let debug = format!("{cfg:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("super-secret-token"));
        assert!(debug.contains("sess-1"));
    }

    #[test]
    fn parses_success_reply() {
        let reply = json!({ "jsonrpc": "2.0", "id": 7, "result": [{"id": "1"}] });
        let out = parse_rpc_reply(reply, 7).expect("should succeed");
        assert_eq!(out, json!([{"id": "1"}]));
    }

    #[test]
    fn parses_rpc_error() {
        let reply = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "error": {
                "code": 1001,
                "message": "Note not found.",
                "data": { "type": "notFound" }
            }
        });
        let err = parse_rpc_reply(reply, 7).unwrap_err();
        assert!(err.is_rpc_error());
        assert_eq!(err.rpc_code(), Some(1001));
    }

    #[test]
    fn rejects_mismatched_reply_id() {
        let reply = json!({ "jsonrpc": "2.0", "id": 99, "result": {} });
        let err = parse_rpc_reply(reply, 7).unwrap_err();
        assert!(matches!(err, AnkiError::InvalidInput(_)));
    }

    #[test]
    fn detects_operation_handle() {
        let handle = json!({
            "operationId": "op-123",
            "method": "collection.sync",
            "session": "s-1"
        });
        assert_eq!(extract_operation_id(&handle).as_deref(), Some("op-123"));
        assert_eq!(extract_operation_id(&json!({"id": "1"})), None);
    }

    #[test]
    fn parse_operation_poll_state_handles_all_states() {
        let succeeded = parse_operation_poll_state(
            "op-1",
            &json!({ "state": "succeeded", "result": { "ok": true } }),
        );
        assert!(matches!(
            succeeded,
            OperationPollOutcome::Succeeded(val) if val == json!({ "ok": true })
        ));

        let failed = parse_operation_poll_state(
            "op-2",
            &json!({ "state": "failed", "error": { "message": "disk full" } }),
        );
        assert!(matches!(
            failed,
            OperationPollOutcome::Failed(AnkiError::OperationFailed { ref state, ref message, .. })
                if state == "failed" && message == "disk full"
        ));

        let cancelled = parse_operation_poll_state("op-3", &json!({ "state": "cancelled" }));
        assert!(matches!(
            cancelled,
            OperationPollOutcome::Failed(AnkiError::OperationFailed { ref state, .. })
                if state == "cancelled"
        ));

        let pending = parse_operation_poll_state("op-4", &json!({ "state": "running" }));
        assert!(matches!(pending, OperationPollOutcome::Pending));
    }

    #[test]
    fn next_poll_delay_grows_and_caps() {
        let max = Duration::from_millis(500);
        assert_eq!(
            next_poll_delay(Duration::from_millis(200), max),
            Duration::from_millis(300)
        );
        assert_eq!(
            next_poll_delay(Duration::from_millis(400), max),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn prepare_params_injects_session_only_for_collection_scope() {
        let decks = prepare_params("decks.list", json!({}), Some("sess-123"), true)
            .expect("decks.list should succeed");
        assert_eq!(decks, json!({ "session": "sess-123" }));

        let op_get = prepare_params(
            "operations.get",
            json!({ "operationId": "op-1" }),
            Some("sess-123"),
            true,
        )
        .expect("operations.get should succeed without session");
        assert_eq!(op_get, json!({ "operationId": "op-1" }));

        let api_desc = prepare_params("api.describe", json!({}), Some("sess-123"), true)
            .expect("api.describe should succeed without session");
        assert_eq!(api_desc, json!({}));

        let explicit = prepare_params(
            "decks.list",
            json!({ "session": "explicit-sess" }),
            Some("sess-123"),
            true,
        )
        .expect("explicit session should be preserved");
        assert_eq!(explicit, json!({ "session": "explicit-sess" }));
    }

    #[test]
    fn build_transport_batch_rejects_invalid_sizes() {
        let empty = build_transport_batch(vec![], None, false);
        assert!(matches!(empty, Err(AnkiError::InvalidInput(_))));

        let too_many: Vec<(String, Value)> = (0..101)
            .map(|_| ("decks.list".to_string(), json!({})))
            .collect();
        let over = build_transport_batch(too_many, None, false);
        assert!(matches!(over, Err(AnkiError::InvalidInput(_))));
    }

    #[test]
    fn build_execute_commands_applies_scope() {
        let items = build_execute_commands(
            vec![
                ("decks.list".to_string(), json!({})),
                ("api.describe".to_string(), json!({})),
            ],
            Some("sess-batch"),
            true,
        )
        .expect("valid inner commands should prepare");

        assert_eq!(
            items,
            vec![
                json!({ "method": "decks.list", "params": { "session": "sess-batch" } }),
                json!({ "method": "api.describe", "params": {} }),
            ]
        );
    }

    #[test]
    fn parse_transport_batch_reply_handles_array_error_and_invalid() {
        let ok = parse_transport_batch_reply(json!([{ "jsonrpc": "2.0", "id": 1, "result": {} }]))
            .expect("array reply should succeed");
        assert!(ok.is_array());

        let rpc_err = parse_transport_batch_reply(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": { "code": -32600, "message": "Invalid Request" }
        }))
        .unwrap_err();
        assert_eq!(rpc_err.rpc_code(), Some(-32600));

        let invalid = parse_transport_batch_reply(json!({ "unexpected": true })).unwrap_err();
        assert!(matches!(invalid, AnkiError::InvalidInput(_)));
    }

    #[cfg(feature = "schema")]
    #[test]
    fn static_application_scoped_methods_matches_embedded_schema() {
        assert!(
            APPLICATION_SCOPED_METHODS.windows(2).all(|w| w[0] < w[1]),
            "APPLICATION_SCOPED_METHODS must be strictly sorted for binary_search"
        );

        let schema_app_methods: Vec<&str> = crate::schema::all_method_names()
            .into_iter()
            .filter(|m| crate::schema::method_scope(m) == Some("application"))
            .collect();

        assert_eq!(
            APPLICATION_SCOPED_METHODS,
            schema_app_methods.as_slice(),
            "APPLICATION_SCOPED_METHODS must match schemas/api-v1.schema.json 100%"
        );

        for method in crate::schema::all_method_names() {
            let expected_collection = crate::schema::method_scope(method) == Some("collection");
            let static_collection = APPLICATION_SCOPED_METHODS.binary_search(&method).is_err();
            assert_eq!(
                static_collection, expected_collection,
                "scope mismatch for method '{method}'"
            );
        }
    }

    #[cfg(feature = "schema")]
    #[test]
    fn prepare_params_respects_validation_flag() {
        let err = prepare_params("notes.get", json!({}), Some("sess-123"), true)
            .expect_err("missing required id should fail validation");
        assert!(matches!(err, AnkiError::InvalidInput(_)));

        let ok = prepare_params("notes.get", json!({}), Some("sess-123"), false)
            .expect("validation disabled should pass");
        assert_eq!(ok, json!({ "session": "sess-123" }));
    }
}

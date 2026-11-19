use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde_json::{json, Map, Value};

use crate::error::CliError;
use crate::schema;

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Blocking JSON-RPC 2.0 client for Anki-Connect API v1.
pub struct AnkiClient {
    http: Client,
    url: String,
    api_key: Option<String>,
    session: Option<String>,
    validate: bool,
    poll_initial_delay: Duration,
    poll_max_delay: Duration,
}

impl AnkiClient {
    pub fn new(
        url: impl Into<String>,
        api_key: Option<String>,
        session: Option<String>,
    ) -> Result<Self, CliError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            url: url.into(),
            api_key: api_key.filter(|k| !k.is_empty()),
            session: session.filter(|s| !s.is_empty()),
            validate: true,
            poll_initial_delay: Duration::from_millis(250),
            poll_max_delay: Duration::from_secs(2),
        })
    }

    /// Enable or disable pre-flight schema validation on outgoing parameters.
    pub fn with_validation(mut self, validate: bool) -> Self {
        self.validate = validate;
        self
    }

    /// Configure initial and maximum polling delays for `wait_for_operation`.
    pub fn with_poll_delays(mut self, initial: Duration, max: Duration) -> Self {
        self.poll_initial_delay = initial;
        self.poll_max_delay = max;
        self
    }

    /// Return the configured session token, if any.
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// Execute a single JSON-RPC 2.0 call and return its `result`.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, CliError> {
        let params = self.prepare_params(method, params)?;
        let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let reply = self.post_json(&body)?;
        parse_rpc_reply(reply, id)
    }

    /// Execute multiple commands sequentially via `api.execute`.
    pub fn execute_batch(
        &self,
        commands: Vec<(String, Value)>,
        wait: bool,
        timeout_secs: u64,
    ) -> Result<Value, CliError> {
        let items = self.prepare_execute_commands(commands)?;
        self.call_and_maybe_wait("api.execute", json!({ "commands": items }), wait, timeout_secs)
    }

    /// Execute multiple JSON-RPC 2.0 requests in a single HTTP `TransportBatch` array POST.
    pub fn transport_batch(&self, requests: Vec<(String, Value)>) -> Result<Value, CliError> {
        if requests.is_empty() || requests.len() > 100 {
            return Err(CliError::InvalidInput(
                "Transport batch must contain between 1 and 100 requests".into(),
            ));
        }

        let mut payload = Vec::with_capacity(requests.len());
        for (method, params) in requests {
            let prepared = self.prepare_params(&method, params)?;
            let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
            payload.push(json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": prepared,
            }));
        }

        let reply = self.post_json(&Value::Array(payload))?;
        if reply.is_array() {
            Ok(reply)
        } else if let Some(err) = reply.get("error") {
            Err(rpc_error_from_value(err))
        } else {
            Err(CliError::InvalidInput(
                "Expected JSON array response for transport batch".into(),
            ))
        }
    }

    fn prepare_execute_commands(
        &self,
        commands: Vec<(String, Value)>,
    ) -> Result<Vec<Value>, CliError> {
        let mut items = Vec::with_capacity(commands.len());
        for (method, params) in commands {
            let prepared = self.prepare_params(&method, params)?;
            items.push(json!({
                "method": method,
                "params": prepared,
            }));
        }
        Ok(items)
    }

    fn post_json(&self, body: &Value) -> Result<Value, CliError> {
        let mut req = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .json(body);

        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }

        let resp = req.send()?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(CliError::HttpStatus {
                status: status.as_u16(),
                message: if text.is_empty() {
                    status.canonical_reason().unwrap_or("HTTP error").to_string()
                } else {
                    text
                },
            });
        }

        Ok(resp.json()?)
    }

    /// Execute a call and, if `wait` is true and the response is an `OperationHandle`,
    /// poll `operations.get` with exponential backoff until a terminal state.
    pub fn call_and_maybe_wait(
        &self,
        method: &str,
        params: Value,
        wait: bool,
        timeout_secs: u64,
    ) -> Result<Value, CliError> {
        let result = self.call(method, params)?;
        if !wait {
            return Ok(result);
        }

        if let Some(op_id) = extract_operation_id(&result) {
            self.wait_for_operation(&op_id, timeout_secs)
        } else {
            Ok(result)
        }
    }

    /// Poll `operations.get` until `succeeded`, `failed`, or `cancelled`.
    pub fn wait_for_operation(
        &self,
        operation_id: &str,
        timeout_secs: u64,
    ) -> Result<Value, CliError> {
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let mut delay = self.poll_initial_delay;
        let max_delay = self.poll_max_delay;

        loop {
            let op = self.call("operations.get", json!({ "operationId": operation_id }))?;
            let state = op
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("unknown");

            match state {
                "succeeded" => {
                    return Ok(op.get("result").cloned().unwrap_or(Value::Null));
                }
                "failed" | "cancelled" => {
                    let err_val = op.get("error").cloned();
                    let msg = err_val
                        .as_ref()
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("Operation did not succeed")
                        .to_string();
                    return Err(CliError::OperationFailed {
                        operation_id: operation_id.to_string(),
                        state: state.to_string(),
                        message: msg,
                        error: err_val,
                    });
                }
                _ => {
                    if Instant::now() >= deadline {
                        return Err(CliError::Timeout {
                            operation_id: operation_id.to_string(),
                            timeout_secs,
                        });
                    }
                    thread::sleep(delay);
                    delay = (delay * 3 / 2).min(max_delay);
                }
            }
        }
    }

    /// Exhaustively paginate a cursor-based method (e.g. `notes.search`, `cards.search`),
    /// merging all items from `array_key` across pages.
    pub fn paginate_all(
        &self,
        method: &str,
        mut params: Map<String, Value>,
        array_key: &str,
    ) -> Result<Value, CliError> {
        let mut all_items: Vec<Value> = Vec::new();
        let mut pages = 0usize;

        loop {
            let page = self.call(method, Value::Object(params.clone()))?;
            pages += 1;

            if let Some(items) = page.get(array_key).and_then(Value::as_array) {
                all_items.extend(items.iter().cloned());
            }

            match page.get("nextCursor").and_then(Value::as_str) {
                Some(cursor) if !cursor.is_empty() => {
                    params.insert("cursor".to_string(), Value::String(cursor.to_string()));
                }
                _ => {
                    return Ok(json!({
                        array_key: all_items,
                        "pages": pages,
                        "total": all_items.len(),
                    }));
                }
            }
        }
    }

    /// Inject `"session"` for collection-scoped methods and optionally validate
    /// parameters against the embedded API v1 schema.
    pub fn prepare_params(&self, method: &str, params: Value) -> Result<Value, CliError> {
        let params = if let Some(session) = &self.session {
            if schema::method_scope(method) == Some("collection") {
                match params {
                    Value::Object(mut map) => {
                        map.entry("session".to_string())
                            .or_insert_with(|| Value::String(session.clone()));
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

        if self.validate {
            schema::validate_method_params(method, &params)?;
        }

        Ok(params)
    }
}

fn rpc_error_from_value(err: &Value) -> CliError {
    let code = err.get("code").and_then(Value::as_i64).unwrap_or(-32603);
    let message = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Unknown JSON-RPC error")
        .to_string();
    let data = err.get("data").cloned();
    CliError::Rpc {
        code,
        message,
        data,
    }
}

fn parse_rpc_reply(reply: Value, expected_id: u64) -> Result<Value, CliError> {
    if let Some(err) = reply.get("error") {
        return Err(rpc_error_from_value(err));
    }

    if reply.get("id").and_then(Value::as_u64) != Some(expected_id) {
        return Err(CliError::InvalidInput(format!(
            "Unexpected JSON-RPC response id (expected {expected_id})"
        )));
    }

    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

fn extract_operation_id(result: &Value) -> Option<String> {
    let obj = result.as_object()?;
    if obj.contains_key("method") {
        obj.get("operationId")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(err.exit_code(), 4);
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
    fn prepare_params_injects_session_only_for_collection_scope() {
        let client = AnkiClient::new(
            "http://127.0.0.1:8765/api/v1",
            None,
            Some("sess-123".to_string()),
        )
        .expect("client should build");

        assert_eq!(client.session(), Some("sess-123"));

        let decks = client
            .prepare_params("decks.list", json!({}))
            .expect("decks.list should succeed");
        assert_eq!(decks, json!({ "session": "sess-123" }));

        let op_get = client
            .prepare_params("operations.get", json!({ "operationId": "op-1" }))
            .expect("operations.get should succeed without session");
        assert_eq!(op_get, json!({ "operationId": "op-1" }));

        let api_desc = client
            .prepare_params("api.describe", json!({}))
            .expect("api.describe should succeed without session");
        assert_eq!(api_desc, json!({}));

        let explicit = client
            .prepare_params("decks.list", json!({ "session": "explicit-sess" }))
            .expect("explicit session should be preserved");
        assert_eq!(explicit, json!({ "session": "explicit-sess" }));
    }

    #[test]
    fn prepare_params_respects_validation_flag() {
        let client = AnkiClient::new(
            "http://127.0.0.1:8765/api/v1",
            None,
            Some("sess-123".to_string()),
        )
        .expect("client should build");

        let err = client
            .prepare_params("notes.get", json!({}))
            .expect_err("missing required id should fail validation");
        assert_eq!(err.exit_code(), 2);

        let unvalidated = client.with_validation(false);
        let ok = unvalidated
            .prepare_params("notes.get", json!({}))
            .expect("validation disabled should pass");
        assert_eq!(ok, json!({ "session": "sess-123" }));
    }

    #[test]
    fn with_poll_delays_updates_polling_configuration() {
        let client = AnkiClient::new("http://127.0.0.1:8765/api/v1", None, None)
            .expect("client should build")
            .with_poll_delays(Duration::from_millis(5), Duration::from_millis(25));

        assert_eq!(client.poll_initial_delay, Duration::from_millis(5));
        assert_eq!(client.poll_max_delay, Duration::from_millis(25));
    }

    #[test]
    fn transport_batch_rejects_invalid_batch_sizes() {
        let client = AnkiClient::new("http://127.0.0.1:8765/api/v1", None, None)
            .expect("client should build");

        let empty_err = client
            .transport_batch(vec![])
            .expect_err("empty batch must fail");
        assert_eq!(empty_err.exit_code(), 2);

        let too_many: Vec<(String, Value)> = (0..101)
            .map(|_| ("decks.list".to_string(), json!({})))
            .collect();
        let over_limit_err = client
            .transport_batch(too_many)
            .expect_err("batch over 100 must fail");
        assert_eq!(over_limit_err.exit_code(), 2);
    }

    #[test]
    fn prepare_execute_commands_applies_scope_and_validation() {
        let client = AnkiClient::new(
            "http://127.0.0.1:8765/api/v1",
            None,
            Some("sess-batch".to_string()),
        )
        .expect("client should build");

        let items = client
            .prepare_execute_commands(vec![
                ("decks.list".to_string(), json!({})),
                ("api.describe".to_string(), json!({})),
            ])
            .expect("valid inner commands should prepare");

        assert_eq!(
            items,
            vec![
                json!({ "method": "decks.list", "params": { "session": "sess-batch" } }),
                json!({ "method": "api.describe", "params": {} }),
            ]
        );

        let invalid_err = client
            .prepare_execute_commands(vec![("notes.get".to_string(), json!({}))])
            .expect_err("invalid inner command should fail pre-flight validation");
        assert_eq!(invalid_err.exit_code(), 2);
    }

    #[test]
    fn transport_batch_posts_array_and_returns_response_array() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api/v1")
            .match_header("authorization", "Bearer secret-key")
            .match_body(mockito::Matcher::PartialJson(json!([
                {
                    "jsonrpc": "2.0",
                    "method": "decks.list",
                    "params": { "session": "sess-batch" }
                },
                {
                    "jsonrpc": "2.0",
                    "method": "api.describe",
                    "params": {}
                }
            ])))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                json!([
                    { "jsonrpc": "2.0", "id": 1, "result": { "decks": [] } },
                    { "jsonrpc": "2.0", "id": 2, "result": { "version": "1.0" } }
                ])
                .to_string(),
            )
            .create();

        let client = AnkiClient::new(
            format!("{}/api/v1", server.url()),
            Some("secret-key".to_string()),
            Some("sess-batch".to_string()),
        )
        .expect("client should build");

        let resp = client
            .transport_batch(vec![
                ("decks.list".to_string(), json!({})),
                ("api.describe".to_string(), json!({})),
            ])
            .expect("transport_batch should succeed");

        mock.assert();
        assert!(resp.is_array());
        assert_eq!(resp.as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn execute_batch_posts_api_execute_and_returns_handle_when_no_wait() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api/v1")
            .match_body(mockito::Matcher::PartialJson(json!({
                "jsonrpc": "2.0",
                "method": "api.execute",
                "params": {
                    "commands": [
                        { "method": "decks.list", "params": { "session": "sess-batch" } },
                        { "method": "api.describe", "params": {} }
                    ]
                }
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body_from_request(|req| {
                let body: Value =
                    serde_json::from_slice(req.body().map(Vec::as_slice).unwrap_or(&[]))
                        .unwrap_or_default();
                let id = body.get("id").cloned().unwrap_or(json!(1));
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "operationId": "op-batch-1",
                        "method": "api.execute",
                        "session": null
                    }
                })
                .to_string()
                .into_bytes()
            })
            .create();

        let client = AnkiClient::new(
            format!("{}/api/v1", server.url()),
            None,
            Some("sess-batch".to_string()),
        )
        .expect("client should build");

        let resp = client
            .execute_batch(
                vec![
                    ("decks.list".to_string(), json!({})),
                    ("api.describe".to_string(), json!({})),
                ],
                false,
                30,
            )
            .expect("execute_batch should succeed");

        mock.assert();
        assert_eq!(resp["operationId"], "op-batch-1");
    }
}

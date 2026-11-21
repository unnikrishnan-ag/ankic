use std::fmt;
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::client::AnkiClientBuilder;
use crate::error::{AnkiError, Result};
use crate::protocol::{self, ClientConfig, OperationPollOutcome};

/// Blocking (synchronous) JSON-RPC 2.0 client for Anki-Connect API v1.
#[derive(Clone)]
pub struct AnkiClient {
    http: Client,
    config: ClientConfig,
}

impl fmt::Debug for AnkiClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnkiClient")
            .field("config", &self.config)
            .finish()
    }
}

impl AnkiClient {
    pub(crate) fn from_parts(http: Client, config: ClientConfig) -> Self {
        Self { http, config }
    }

    /// Return a new [`AnkiClientBuilder`].
    pub fn builder() -> AnkiClientBuilder {
        AnkiClientBuilder::new()
    }

    /// Create a new blocking client with the given URL, optional API key, and optional session.
    pub fn new(
        url: impl Into<String>,
        api_key: Option<String>,
        session: Option<String>,
    ) -> Result<Self> {
        Self::builder()
            .url(url)
            .api_key_opt(api_key)
            .session_opt(session)
            .build_blocking()
    }

    /// Create a new blocking client targeting `url` with default settings.
    pub fn from_url(url: impl Into<String>) -> Result<Self> {
        Self::builder().url(url).build_blocking()
    }

    /// Enable or disable pre-flight schema validation on outgoing parameters.
    pub fn with_validation(mut self, validate: bool) -> Self {
        self.config.validate = validate;
        self
    }

    /// Configure initial and maximum polling delays for `wait_for_operation`.
    pub fn with_poll_delays(mut self, initial: Duration, max: Duration) -> Self {
        self.config.poll_initial_delay = initial;
        self.config.poll_max_delay = max;
        self
    }

    /// Return a cloned client bound to a new collection session token.
    pub fn with_session(&self, session: impl Into<String>) -> Self {
        let mut cloned = self.clone();
        cloned.config.session = Some(session.into()).filter(|s| !s.is_empty());
        cloned
    }

    /// Return the configured session token, if any.
    pub fn session(&self) -> Option<&str> {
        self.config.session()
    }

    /// Return the configured endpoint URL.
    pub fn url(&self) -> &str {
        self.config.url()
    }

    /// Execute a typed JSON-RPC 2.0 call and deserialize its `result` into `R`.
    pub fn call<P: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
    ) -> Result<R> {
        let params_val = serde_json::to_value(params)?;
        let result = self.call_raw(method, params_val)?;
        Ok(serde_json::from_value(result)?)
    }

    /// Execute a single JSON-RPC 2.0 call and return its raw JSON `result`.
    pub fn call_raw(&self, method: &str, params: Value) -> Result<Value> {
        let prepared = self.prepare_params(method, params)?;
        let (id, body) = protocol::build_rpc_request(method, prepared);
        let reply = self.post_json(&body)?;
        protocol::parse_rpc_reply(reply, id)
    }

    /// Execute a call and, if `wait` is true and the response is an `OperationHandle`,
    /// poll `operations.get` with exponential backoff until a terminal state.
    pub fn call_and_maybe_wait(
        &self,
        method: &str,
        params: Value,
        wait: bool,
        timeout_secs: u64,
    ) -> Result<Value> {
        let result = self.call_raw(method, params)?;
        if !wait {
            return Ok(result);
        }

        if let Some(op_id) = protocol::extract_operation_id(&result) {
            self.wait_for_operation_raw(&op_id, timeout_secs)
        } else {
            Ok(result)
        }
    }

    /// Poll `operations.get` until `succeeded`, `failed`, or `cancelled`, deserializing
    /// the final operation `result` into `R`.
    pub fn wait_for_operation<R: DeserializeOwned>(
        &self,
        operation_id: &str,
        timeout_secs: u64,
    ) -> Result<R> {
        let result = self.wait_for_operation_raw(operation_id, timeout_secs)?;
        Ok(serde_json::from_value(result)?)
    }

    /// Poll `operations.get` until `succeeded`, `failed`, or `cancelled`, returning the raw JSON `result`.
    pub fn wait_for_operation_raw(
        &self,
        operation_id: &str,
        timeout_secs: u64,
    ) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let mut delay = self.config.poll_initial_delay;
        let max_delay = self.config.poll_max_delay;

        loop {
            let op = self.call_raw("operations.get", json!({ "operationId": operation_id }))?;

            match protocol::parse_operation_poll_state(operation_id, &op) {
                OperationPollOutcome::Succeeded(result) => return Ok(result),
                OperationPollOutcome::Failed(err) => return Err(err),
                OperationPollOutcome::Pending => {
                    if Instant::now() >= deadline {
                        return Err(AnkiError::Timeout {
                            operation_id: operation_id.to_string(),
                            timeout_secs,
                        });
                    }
                    thread::sleep(delay);
                    delay = protocol::next_poll_delay(delay, max_delay);
                }
            }
        }
    }

    /// Execute multiple commands sequentially via `api.execute`.
    pub fn execute_batch(
        &self,
        commands: Vec<(String, Value)>,
        wait: bool,
        timeout_secs: u64,
    ) -> Result<Value> {
        let items = protocol::build_execute_commands(
            commands,
            self.config.session(),
            self.config.validate,
        )?;
        self.call_and_maybe_wait("api.execute", json!({ "commands": items }), wait, timeout_secs)
    }

    /// Execute multiple JSON-RPC 2.0 requests in a single HTTP `TransportBatch` array POST.
    pub fn transport_batch(&self, requests: Vec<(String, Value)>) -> Result<Value> {
        let payload = protocol::build_transport_batch(
            requests,
            self.config.session(),
            self.config.validate,
        )?;
        let reply = self.post_json(&payload)?;
        protocol::parse_transport_batch_reply(reply)
    }

    /// Exhaustively paginate a cursor-based method (e.g. `notes.search`, `cards.search`),
    /// merging all items from `array_key` across pages.
    pub fn paginate_all(
        &self,
        method: &str,
        mut params: Map<String, Value>,
        array_key: &str,
    ) -> Result<Value> {
        let mut all_items: Vec<Value> = Vec::new();
        let mut pages = 0usize;

        loop {
            let page = self.call_raw(method, Value::Object(params.clone()))?;
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
    pub fn prepare_params(&self, method: &str, params: Value) -> Result<Value> {
        protocol::prepare_params(method, params, self.config.session(), self.config.validate)
    }

    fn post_json(&self, body: &Value) -> Result<Value> {
        let mut req = self
            .http
            .post(&self.config.url)
            .header("Content-Type", "application/json")
            .json(body);

        if let Some(key) = &self.config.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }

        let resp = req.send()?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(AnkiError::HttpStatus {
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
}

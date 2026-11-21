use std::fmt;
use std::time::Duration;

#[cfg(any(feature = "async", feature = "blocking"))]
use crate::error::Result;
use crate::protocol::ClientConfig;

#[cfg(feature = "async")]
mod async_client;
#[cfg(feature = "async")]
pub use async_client::AnkiClient;

#[cfg(feature = "blocking")]
pub mod blocking;

/// Builder for configuring an async [`AnkiClient`] or [`blocking::AnkiClient`].
#[derive(Clone)]
pub struct AnkiClientBuilder {
    config: ClientConfig,
}

impl Default for AnkiClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for AnkiClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnkiClientBuilder")
            .field("config", &self.config)
            .finish()
    }
}

impl AnkiClientBuilder {
    /// Create a new builder with default settings (`http://127.0.0.1:8765/api/v1`,
    /// 30s timeout, pre-flight validation enabled, 250ms..2s polling backoff).
    pub fn new() -> Self {
        Self {
            config: ClientConfig::default(),
        }
    }

    /// Set the Anki-Connect API v1 endpoint URL.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.config.url = url.into();
        self
    }

    /// Set the Bearer API key (empty strings are treated as unset).
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.config.api_key = Some(api_key.into()).filter(|k| !k.is_empty());
        self
    }

    /// Set an optional Bearer API key (empty strings are treated as unset).
    pub fn api_key_opt(mut self, api_key: Option<String>) -> Self {
        self.config.api_key = api_key.filter(|k| !k.is_empty());
        self
    }

    /// Set the default collection session token (empty strings are treated as unset).
    pub fn session(mut self, session: impl Into<String>) -> Self {
        self.config.session = Some(session.into()).filter(|s| !s.is_empty());
        self
    }

    /// Set an optional default collection session token (empty strings are treated as unset).
    pub fn session_opt(mut self, session: Option<String>) -> Self {
        self.config.session = session.filter(|s| !s.is_empty());
        self
    }

    /// Set the HTTP request timeout used when constructing the default `reqwest` client.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Enable or disable pre-flight schema validation on outgoing parameters.
    pub fn validation(mut self, validate: bool) -> Self {
        self.config.validate = validate;
        self
    }

    /// Configure initial and maximum polling delays for `wait_for_operation`.
    pub fn poll_delays(mut self, initial: Duration, max: Duration) -> Self {
        self.config.poll_initial_delay = initial;
        self.config.poll_max_delay = max;
        self
    }

    /// Build an asynchronous [`AnkiClient`] with a freshly configured `reqwest::Client`.
    #[cfg(feature = "async")]
    pub fn build(self) -> Result<AnkiClient> {
        let http = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()?;
        Ok(AnkiClient::from_parts(http, self.config))
    }

    /// Build an asynchronous [`AnkiClient`] using a caller-supplied `reqwest::Client`.
    #[cfg(feature = "async")]
    pub fn build_with_http_client(self, http: reqwest::Client) -> AnkiClient {
        AnkiClient::from_parts(http, self.config)
    }

    /// Build a synchronous [`blocking::AnkiClient`] with a freshly configured `reqwest::blocking::Client`.
    #[cfg(feature = "blocking")]
    pub fn build_blocking(self) -> Result<blocking::AnkiClient> {
        let http = reqwest::blocking::Client::builder()
            .timeout(self.config.timeout)
            .build()?;
        Ok(blocking::AnkiClient::from_parts(http, self.config))
    }

    /// Build a synchronous [`blocking::AnkiClient`] using a caller-supplied `reqwest::blocking::Client`.
    #[cfg(feature = "blocking")]
    pub fn build_blocking_with_http_client(
        self,
        http: reqwest::blocking::Client,
    ) -> blocking::AnkiClient {
        blocking::AnkiClient::from_parts(http, self.config)
    }
}

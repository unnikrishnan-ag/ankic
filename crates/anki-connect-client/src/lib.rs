//! Rust client library for the [Anki-Connect API v1](https://git.sr.ht/~foosoft/anki-connect/tree/master/API_V1.md) (`POST /api/v1`).
//!
//! `anki-connect-client` provides 100% coverage of Anki-Connect API v1 with **96 typed methods
//! across 21 namespaces**, both asynchronous and blocking (synchronous) HTTP clients, automatic
//! scope-aware collection session injection, operation polling with exponential backoff, cursor
//! pagination, and optional pre-flight JSON Schema validation.
//!
//! # Cargo Features
//!
//! | Feature | Default | Description |
//! | --- | --- | --- |
//! | `async` | Yes | Enables the asynchronous [`AnkiClient`] (backed by `tokio` and `reqwest`). |
//! | `schema` | Yes | Embeds `schemas/api-v1.schema.json` for pre-flight parameter validation and offline introspection via [`schema`]. |
//! | `blocking` | No | Enables the synchronous [`blocking::AnkiClient`] (backed by `reqwest::blocking`). |
//!
//! # Quickstart (Async)
//!
//! ```rust,no_run
//! # #[cfg(feature = "async")]
//! # async fn run() -> anki_connect_client::Result<()> {
//! use anki_connect_client::types::NotesSearchParams;
//! use anki_connect_client::AnkiClient;
//!
//! let client = AnkiClient::builder()
//!     .url("http://127.0.0.1:8765/api/v1")
//!     .build()?;
//!
//! // Inspect active collection and bind its session token for subsequent calls.
//! let collection = client.collection().get().await?;
//! let client = client.with_session(collection.session);
//!
//! // List decks and search notes with typed namespace APIs.
//! let decks = client.decks().list().await?;
//! println!("Decks: {}", decks.len());
//!
//! let page = client
//!     .notes()
//!     .search(&NotesSearchParams {
//!         query: "deck:Default".to_string(),
//!         cursor: None,
//!         limit: Some(10),
//!         session: None,
//!         sort: None,
//!     })
//!     .await?;
//! println!("Found {} note IDs", page.ids.len());
//! # Ok(())
//! # }
//! ```
//!
//! # Quickstart (Blocking)
//!
//! Enable the `blocking` feature in `Cargo.toml` to use the synchronous client without a Tokio runtime:
//!
//! ```rust,no_run
//! # #[cfg(feature = "blocking")]
//! # fn run() -> anki_connect_client::Result<()> {
//! use anki_connect_client::blocking::AnkiClient;
//!
//! let client = AnkiClient::builder()
//!     .url("http://127.0.0.1:8765/api/v1")
//!     .build_blocking()?;
//!
//! let collection = client.collection().get()?;
//! let client = client.with_session(collection.session);
//! let decks = client.decks().list()?;
//! println!("Profile {} has {} decks", collection.profile, decks.len());
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod api;
pub mod client;
pub mod error;
pub mod protocol;
#[cfg(feature = "schema")]
pub mod schema;
pub mod types;

#[cfg(feature = "blocking")]
pub use client::blocking;
#[cfg(feature = "async")]
pub use client::AnkiClient;
pub use client::AnkiClientBuilder;
pub use error::{AnkiError, Result};

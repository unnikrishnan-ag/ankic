//! Typed namespace sub-clients for all 21 Anki-Connect API v1 namespaces.

#[cfg(feature = "async")]
pub mod generated_async;

#[cfg(feature = "blocking")]
pub mod generated_blocking;

#[cfg(feature = "async")]
pub use generated_async::*;

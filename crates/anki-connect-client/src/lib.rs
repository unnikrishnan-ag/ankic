pub mod client;
pub mod error;
pub mod protocol;
#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "blocking")]
pub use client::blocking;
#[cfg(feature = "async")]
pub use client::AnkiClient;
pub use client::AnkiClientBuilder;
pub use error::{AnkiError, Result};

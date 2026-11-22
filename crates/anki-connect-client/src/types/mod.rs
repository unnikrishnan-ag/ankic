//! Schema-generated domain, parameter, and result types for Anki-Connect API v1.

mod generated;

pub use generated::*;

impl Reference {
    /// Construct a [`Reference`] identifying an entity by name.
    pub fn name(name: impl Into<String>) -> Self {
        Self::Name { name: name.into() }
    }

    /// Construct a [`Reference`] identifying an entity by decimal entity ID.
    pub fn id(id: impl Into<String>) -> Self {
        Self::Id { id: id.into() }
    }
}

impl MediaSource {
    /// Construct a [`MediaSource`] from base64-encoded bytes.
    pub fn data(data: impl Into<String>) -> Self {
        Self::Data { data: data.into() }
    }

    /// Construct a [`MediaSource`] from a local file path.
    pub fn path(path: impl Into<String>) -> Self {
        Self::Path { path: path.into() }
    }

    /// Construct a [`MediaSource`] from an HTTP(S) URL.
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }
}

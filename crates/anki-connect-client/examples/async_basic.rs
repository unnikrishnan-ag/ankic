//! Basic asynchronous example for `anki-connect-client`.
//!
//! Run against a local Anki instance with Anki-Connect v1 enabled:
//! ```bash
//! cargo run -p anki-connect-client --example async_basic
//! ```

use anki_connect_client::protocol::DEFAULT_URL;
use anki_connect_client::types::NotesSearchParams;
use anki_connect_client::{AnkiClient, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let url = std::env::var("ANKI_CONNECT_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let api_key = std::env::var("ANKI_CONNECT_API_KEY").ok();

    let client = AnkiClient::builder()
        .url(url)
        .api_key_opt(api_key)
        .build()?;

    // 1. Inspect the active collection and bind its session token.
    let collection = client.collection().get().await?;
    println!(
        "Connected to profile `{}` (session: {})",
        collection.profile, collection.session
    );
    let client = client.with_session(collection.session);

    // 2. List all decks in the collection.
    let decks = client.decks().list().await?;
    println!("Found {} decks:", decks.len());
    for deck in &decks {
        println!("  - {} (id: {})", deck.name, deck.id);
    }

    // 3. Search notes in the Default deck (first 10 IDs).
    let page = client
        .notes()
        .search(&NotesSearchParams {
            query: "deck:Default".to_string(),
            cursor: None,
            limit: Some(10),
            session: None,
            sort: None,
        })
        .await?;
    println!(
        "Search returned {} note IDs (next_cursor: {:?})",
        page.ids.len(),
        page.next_cursor
    );

    Ok(())
}

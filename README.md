# ankic

A Rust CLI for [Anki-Connect API v1](https://git.sr.ht/~foosoft/anki-connect/tree/master/API_V1.md) (`POST /api/v1`), built for **100% API coverage** (all 96 methods across 21 namespaces) and **LLM agent skill** usage.

## Build & Test

Requires Rust (1.80+ / 2021 edition):

```bash
cargo build --release
```

The compiled binary is at `target/release/ankic`.

Run the full test suite (unit, schema contract, mock HTTP, and compiled binary E2E tests):

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## Features

- **YAML-by-default output:** Clean, token-efficient YAML on `stdout` and `stderr` by default, with `--format <yaml|json|json-pretty>` (and `--json` / `--pretty` aliases) when JSON is needed.
- **YAML & JSON parameter inputs:** `--params`, `--fields`, `--stdin`, and `batch` inputs (`--commands`, `--file`, `--stdin`) accept both YAML and JSON interchangeably.
- **Pre-flight schema validation:** Validates parameters and method names client-side against the embedded `schemas/api-v1.schema.json` before sending HTTP requests (fail-fast exit code `2`; bypass with `--no-validate`).
- **Scope-aware session handling:** Automatically injects `--session` / `ANKI_CONNECT_SESSION` only into `scope: "collection"` methods and omits it on `scope: "application"` methods (e.g., `operations.get`, `api.describe`, `api.execute`, `profiles.*`).
- **Offline schema introspection (`schema`):** Inspect all 96 V1 methods, parameter/result schemas, and error codes offline without a running Anki instance.
- **Complete subcommand coverage (96/96 methods across 21 namespaces):** Ergonomic subcommands for `collection`, `decks`, `deck-presets`, `notes`, `cards`, `note-types`, `tags`, `media`, `scheduling`, `reviews`, `statistics`, `profiles`, `packages`, `access`, `advanced`, `gui` (`browser`, `editors`, `reviewer`, `application`), and `operations`, plus `--all` cursor auto-pagination on search commands and automatic `OperationHandle` polling (unless `--no-wait` is passed).
- **Batch & raw escape hatches (`batch`, `call`, `describe`):** Run multi-command batches via `api.execute` (`--mode execute`) or JSON-RPC array batching (`--mode transport`), or invoke any method directly with `call`.
- **Deterministic exit codes:** `0` = success, `2` = CLI / input / pre-flight validation error, `3` = transport / HTTP error, `4` = JSON-RPC or failed/cancelled operation, `5` = operation polling timeout.

## Configuration

| Flag | Env Var | Default | Description |
| --- | --- | --- | --- |
| `--url` | `ANKI_CONNECT_URL` | `http://127.0.0.1:8765/api/v1` | Anki-Connect v1 JSON-RPC endpoint |
| `--api-key` | `ANKI_CONNECT_API_KEY` | unset | Bearer token (`Authorization: Bearer <key>`) |
| `--session` | `ANKI_CONNECT_SESSION` | unset | Collection session ID from `collection get` |
| `-f, --format` | — | `yaml` | Output format: `yaml`, `json`, or `json-pretty` |
| `--json` | — | `false` | Alias for `--format json` |
| `--pretty` | — | `false` | Alias for `--format json-pretty` |
| `--no-validate` | — | `false` | Bypass client-side pre-flight schema validation |

## Examples

```bash
# List all 96 V1 methods offline (YAML output by default)
ankic schema

# Inspect a method's schema in pretty JSON
ankic schema notes.create --pretty

# Check active collection & capture session
ankic collection get

# Create a note using inline YAML --fields
ankic notes create --deck Default --note-type Basic --fields $'Front: 猫\nBack: cat'

# Search notes with cursor auto-pagination across all pages
ankic notes search "deck:Default" --all

# Execute a sequential batch via api.execute (polls OperationHandle automatically)
ankic batch --mode execute --commands '
- method: decks.create
  params:
    name: Japanese::Vocab
- method: collection.sync
  params: {}
'

# Call any V1 method directly with YAML or JSON --params
ankic call statistics.getToday
ankic call packages.export --params '{"deck":{"name":"Default"},"path":"/tmp/out.apkg"}'
```

## Using as a Rust Library (`anki-connect-client`)

The `anki-connect-client` crate provides the typed Anki-Connect API v1 client (96 methods across 21 namespaces, async and blocking clients, automatic session scoping, operation polling, and optional pre-flight schema validation) with zero CLI dependencies (`clap`, `serde_norway`, etc.).

### Adding to `Cargo.toml`

```toml
# Default: async client + embedded schema validation
anki-connect-client = "0.1"

# Blocking (synchronous) client only, with schema validation
# anki-connect-client = { version = "0.1", default-features = false, features = ["blocking", "schema"] }

# Minimal async client without the embedded JSON Schema
# anki-connect-client = { version = "0.1", default-features = false, features = ["async"] }
```

### Async Example

```rust,no_run
use std::collections::BTreeMap;
use anki_connect_client::types::{NotesCreateParams, Reference};
use anki_connect_client::{AnkiClient, Result};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let client = AnkiClient::builder()
        .url("http://127.0.0.1:8765/api/v1")
        .build()?;

    let collection = client.collection().get().await?;
    let client = client.with_session(collection.session);

    let decks = client.decks().list().await?;
    println!("Decks: {}", decks.len());

    let fields = BTreeMap::from([
        ("Front".to_string(), "猫".to_string()),
        ("Back".to_string(), "cat".to_string()),
    ]);
    let created = client
        .notes()
        .create(&NotesCreateParams {
            deck: Reference::name("Default"),
            note_type: Reference::name("Basic"),
            fields,
            duplicates: None,
            reject_empty_questions: None,
            session: None,
            tags: Some(vec!["vocab".to_string()]),
        })
        .await?;
    println!("Created note {}", created.note_id);
    Ok(())
}
```

### Blocking Example

```rust,no_run
use anki_connect_client::blocking::AnkiClient;
use anki_connect_client::Result;

fn main() -> Result<()> {
    let client = AnkiClient::builder()
        .url("http://127.0.0.1:8765/api/v1")
        .build_blocking()?;

    let collection = client.collection().get()?;
    let client = client.with_session(collection.session);

    let decks = client.decks().list()?;
    println!("Profile `{}` has {} decks", collection.profile, decks.len());
    Ok(())
}
```

### Running the Crate Examples

```bash
cargo run -p anki-connect-client --example async_basic
cargo run -p anki-connect-client --example blocking_basic --features blocking
```

## Workspace Layout & Code Generation

This repository is organized as a Cargo workspace:

- `crates/anki-connect-client` — Standalone Rust client library (typed models, 21 async/blocking namespace sub-clients, JSON-RPC protocol helpers, and embedded schema validator).
- `crates/ankic` — The `ankic` CLI binary built on top of `anki-connect-client`.
- `crates/ankic-codegen` — Internal code generator that produces the typed models and namespace APIs from `schemas/api-v1.schema.json`.

To regenerate the typed client code after updating `schemas/api-v1.schema.json`, or verify that checked-in generated files are up to date:

```bash
cargo run -p ankic-codegen
cargo run -p ankic-codegen -- --check
```

use std::collections::BTreeMap;
use std::sync::LazyLock;

use anki_connect_client::types::{
    BatchItem, CollectionSyncParams, Deck, DeckKind, DecksCreateParams, DecksDeleteParams,
    DuplicatePolicy, DuplicatePolicyScope, Error as RpcErrorDetail, MediaSource, NoteCreated,
    NoteInput, NotesCreateParams, OperationHandle, Reference, SyncResult,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

const RAW_SCHEMA: &str = include_str!("../../../schemas/api-v1.schema.json");

static ROOT_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(RAW_SCHEMA).expect("schemas/api-v1.schema.json must be valid JSON")
});

fn assert_valid_against_def(def_name: &str, value: &Value) {
    let defs = ROOT_SCHEMA
        .get("$defs")
        .expect("schema must contain $defs")
        .clone();
    assert!(
        defs.get(def_name).is_some(),
        "definition `{def_name}` must exist in schemas/api-v1.schema.json"
    );

    let wrapper = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": format!("#/$defs/{def_name}"),
        "$defs": defs,
    });

    let validator = jsonschema::draft202012::new(&wrapper)
        .unwrap_or_else(|err| panic!("failed to compile schema for `{def_name}`: {err}"));

    if let Err(err) = validator.validate(value) {
        panic!("value failed validation against `$defs/{def_name}`:\nerror: {err}\nvalue: {value}");
    }
}

fn assert_roundtrip_and_schema<T>(def_name: &str, value: &T) -> Value
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let serialized = serde_json::to_value(value).expect("serialization should succeed");
    assert_valid_against_def(def_name, &serialized);
    let deserialized: T =
        serde_json::from_value(serialized.clone()).expect("deserialization should succeed");
    assert_eq!(
        &deserialized, value,
        "round-trip mismatch for `{def_name}`"
    );
    serialized
}

#[test]
fn reference_helpers_roundtrip_and_validate() {
    let by_name = Reference::name("Default");
    let name_json = assert_roundtrip_and_schema("Reference", &by_name);
    assert_eq!(name_json, json!({ "name": "Default" }));

    let by_id = Reference::id("1700000000000");
    let id_json = assert_roundtrip_and_schema("Reference", &by_id);
    assert_eq!(id_json, json!({ "id": "1700000000000" }));
}

#[test]
fn media_source_helpers_roundtrip_and_validate() {
    let data = MediaSource::data("SGVsbG8=");
    assert_eq!(
        assert_roundtrip_and_schema("MediaSource", &data),
        json!({ "data": "SGVsbG8=" })
    );

    let path = MediaSource::path("/tmp/audio.mp3");
    assert_eq!(
        assert_roundtrip_and_schema("MediaSource", &path),
        json!({ "path": "/tmp/audio.mp3" })
    );

    let url = MediaSource::url("https://example.com/image.png");
    assert_eq!(
        assert_roundtrip_and_schema("MediaSource", &url),
        json!({ "url": "https://example.com/image.png" })
    );
}

#[test]
fn deck_and_params_roundtrip_and_validate() {
    let create_params = DecksCreateParams {
        name: "Japanese::Core2k".to_string(),
        session: Some("sess-1".to_string()),
    };
    let create_json = assert_roundtrip_and_schema("decks_create_Params", &create_params);
    assert_eq!(
        create_json,
        json!({
            "name": "Japanese::Core2k",
            "session": "sess-1"
        })
    );

    let delete_params = DecksDeleteParams {
        decks: vec![Reference::name("Temp"), Reference::id("1700000000001")],
        delete_cards: true,
        session: None,
    };
    assert_roundtrip_and_schema("decks_delete_Params", &delete_params);

    let deck = Deck {
        id: "1700000000001".to_string(),
        kind: DeckKind::Normal,
        name: "Japanese::Core2k".to_string(),
        preset_id: Some("1".to_string()),
    };
    assert_roundtrip_and_schema("Deck", &deck);
}

#[test]
fn notes_and_note_input_roundtrip_and_validate() {
    let mut fields = BTreeMap::new();
    fields.insert("Front".to_string(), "食べる".to_string());
    fields.insert("Back".to_string(), "to eat".to_string());

    let dup_policy = DuplicatePolicy {
        across_note_types: Some(false),
        allow: Some(false),
        deck: Some(Reference::name("Japanese")),
        include_children: Some(true),
        scope: Some(DuplicatePolicyScope::Deck),
    };

    let note_input = NoteInput {
        deck: Reference::name("Japanese"),
        duplicates: Some(dup_policy.clone()),
        fields: fields.clone(),
        note_type: Reference::name("Basic"),
        reject_empty_questions: Some(true),
        tags: Some(vec!["vocab".to_string(), "jlpt-n5".to_string()]),
    };
    assert_roundtrip_and_schema("NoteInput", &note_input);

    let create_params = NotesCreateParams {
        deck: Reference::id("1"),
        duplicates: Some(dup_policy),
        fields,
        note_type: Reference::name("Basic"),
        reject_empty_questions: Some(true),
        session: None,
        tags: Some(vec!["vocab".to_string()]),
    };
    assert_roundtrip_and_schema("notes_create_Params", &create_params);

    let created = NoteCreated {
        note_id: "1700000000100".to_string(),
        card_ids: vec!["1700000000101".to_string()],
    };
    assert_roundtrip_and_schema("NoteCreated", &created);
}

#[test]
fn operation_sync_and_batch_types_roundtrip_and_validate() {
    let sync_params = CollectionSyncParams {
        include_media: Some(true),
        session: None,
    };
    assert_roundtrip_and_schema("collection_sync_Params", &sync_params);

    let handle = OperationHandle {
        method: "collection.sync".to_string(),
        operation_id: "op-sync-123".to_string(),
        session: Some("sess-42".to_string()),
    };
    assert_roundtrip_and_schema("OperationHandle", &handle);

    let sync_result = SyncResult {
        collection_complete: true,
        media_complete: true,
    };
    assert_roundtrip_and_schema("SyncResult", &sync_result);

    let batch_ok = BatchItem::Success {
        index: 0,
        method: "decks.list".to_string(),
        result: json!([]),
    };
    assert_roundtrip_and_schema("BatchItem", &batch_ok);

    let batch_err = BatchItem::Error {
        index: 1,
        method: "decks.get".to_string(),
        error: RpcErrorDetail {
            code: -32001,
            data: None,
            message: "Deck not found".to_string(),
        },
    };
    assert_roundtrip_and_schema("BatchItem", &batch_err);
}

#[cfg(any(feature = "async", feature = "blocking"))]
fn rpc_ok_mock(
    server: &mut mockito::Server,
    expected_method: &str,
    expected_params: Value,
    result: Value,
) -> mockito::Mock {
    let method = expected_method.to_string();
    server
        .mock("POST", "/api/v1")
        .match_header("content-type", "application/json")
        .match_request(move |req| {
            let Ok(body) = req.body() else {
                return false;
            };
            let Ok(parsed): Result<Value, _> = serde_json::from_slice(body) else {
                return false;
            };
            parsed.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
                && parsed.get("id").and_then(Value::as_u64).is_some()
                && parsed.get("method").and_then(Value::as_str) == Some(method.as_str())
                && parsed.get("params") == Some(&expected_params)
        })
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body_from_request(move |req| {
            let parsed: Value =
                serde_json::from_slice(req.body().expect("request body")).expect("valid json");
            let id = parsed["id"].clone();
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            })
            .to_string()
            .into_bytes()
        })
        .create()
}

#[cfg(feature = "async")]
mod async_typed_tests {
    use std::time::Duration;

    use anki_connect_client::AnkiClientBuilder;
    use mockito::Server;

    use super::*;

    #[tokio::test]
    async fn decks_list_and_create_typed_async() {
        let mut server = Server::new_async().await;
        let list_mock = rpc_ok_mock(
            &mut server,
            "decks.list",
            json!({}),
            json!([
                {
                    "id": "1",
                    "name": "Default",
                    "kind": "normal",
                    "presetId": "1"
                }
            ]),
        );
        let create_mock = rpc_ok_mock(
            &mut server,
            "decks.create",
            json!({ "name": "Rust" }),
            json!({
                "id": "1700000000010",
                "name": "Rust",
                "kind": "normal",
                "presetId": "1"
            }),
        );

        let client = AnkiClientBuilder::default()
            .url(format!("{}/api/v1", server.url()))
            .build()
            .expect("client builds");

        let decks = client.decks().list().await.expect("decks.list succeeds");
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, "Default");
        assert_eq!(decks[0].kind, DeckKind::Normal);

        let created = client
            .decks()
            .create(&DecksCreateParams {
                name: "Rust".to_string(),
                session: None,
            })
            .await
            .expect("decks.create succeeds");
        assert_eq!(created.id, "1700000000010");
        assert_eq!(created.name, "Rust");

        list_mock.assert_async().await;
        create_mock.assert_async().await;
    }

    #[tokio::test]
    async fn notes_create_typed_async() {
        let mut server = Server::new_async().await;
        let create_mock = rpc_ok_mock(
            &mut server,
            "notes.create",
            json!({
                "deck": { "name": "Default" },
                "noteType": { "name": "Basic" },
                "fields": { "Back": "A", "Front": "Q" },
                "tags": ["unit-test"]
            }),
            json!({
                "noteId": "1700000000200",
                "cardIds": ["1700000000201"]
            }),
        );

        let client = AnkiClientBuilder::default()
            .url(format!("{}/api/v1", server.url()))
            .build()
            .expect("client builds");

        let mut fields = BTreeMap::new();
        fields.insert("Front".to_string(), "Q".to_string());
        fields.insert("Back".to_string(), "A".to_string());

        let created = client
            .notes()
            .create(&NotesCreateParams {
                deck: Reference::name("Default"),
                note_type: Reference::name("Basic"),
                fields,
                tags: Some(vec!["unit-test".to_string()]),
                duplicates: None,
                reject_empty_questions: None,
                session: None,
            })
            .await
            .expect("notes.create succeeds");

        assert_eq!(created.note_id, "1700000000200");
        assert_eq!(created.card_ids, vec!["1700000000201"]);
        create_mock.assert_async().await;
    }

    #[tokio::test]
    async fn collection_sync_and_wait_typed_async() {
        let mut server = Server::new_async().await;
        let sync_mock = rpc_ok_mock(
            &mut server,
            "collection.sync",
            json!({ "includeMedia": true }),
            json!({
                "operationId": "op-sync-1",
                "method": "collection.sync",
                "session": "sess-1"
            }),
        );
        let running_mock = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-sync-1" }),
            json!({
                "operationId": "op-sync-1",
                "method": "collection.sync",
                "state": "running",
                "session": "sess-1",
                "startedAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-01T00:00:01Z"
            }),
        )
        .expect(1);
        let done_mock = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-sync-1" }),
            json!({
                "operationId": "op-sync-1",
                "method": "collection.sync",
                "state": "succeeded",
                "session": "sess-1",
                "startedAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-01T00:00:02Z",
                "result": {
                    "collectionComplete": true,
                    "mediaComplete": true
                }
            }),
        )
        .expect(1);

        let client = AnkiClientBuilder::default()
            .url(format!("{}/api/v1", server.url()))
            .poll_delays(Duration::from_millis(5), Duration::from_millis(10))
            .build()
            .expect("client builds");

        let result = client
            .collection()
            .sync_and_wait(
                &CollectionSyncParams {
                    include_media: Some(true),
                    session: None,
                },
                5,
            )
            .await
            .expect("collection.sync_and_wait succeeds");

        assert_eq!(
            result,
            SyncResult {
                collection_complete: true,
                media_complete: true,
            }
        );

        sync_mock.assert_async().await;
        running_mock.assert_async().await;
        done_mock.assert_async().await;
    }
}

#[cfg(feature = "blocking")]
mod blocking_typed_tests {
    use std::time::Duration;

    use anki_connect_client::AnkiClientBuilder;
    use mockito::Server;

    use super::*;

    #[test]
    fn decks_list_and_create_typed_blocking() {
        let mut server = Server::new();
        let list_mock = rpc_ok_mock(
            &mut server,
            "decks.list",
            json!({}),
            json!([
                {
                    "id": "1",
                    "name": "Default",
                    "kind": "normal",
                    "presetId": "1"
                }
            ]),
        );
        let create_mock = rpc_ok_mock(
            &mut server,
            "decks.create",
            json!({ "name": "BlockingDeck" }),
            json!({
                "id": "1700000000020",
                "name": "BlockingDeck",
                "kind": "normal",
                "presetId": "1"
            }),
        );

        let client = AnkiClientBuilder::default()
            .url(format!("{}/api/v1", server.url()))
            .build_blocking()
            .expect("blocking client builds");

        let decks = client.decks().list().expect("decks.list succeeds");
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, "Default");

        let created = client
            .decks()
            .create(&DecksCreateParams {
                name: "BlockingDeck".to_string(),
                session: None,
            })
            .expect("decks.create succeeds");
        assert_eq!(created.id, "1700000000020");
        assert_eq!(created.name, "BlockingDeck");

        list_mock.assert();
        create_mock.assert();
    }

    #[test]
    fn notes_create_and_collection_sync_and_wait_typed_blocking() {
        let mut server = Server::new();
        let note_mock = rpc_ok_mock(
            &mut server,
            "notes.create",
            json!({
                "deck": { "name": "Default" },
                "noteType": { "name": "Basic" },
                "fields": { "Back": "B", "Front": "F" }
            }),
            json!({
                "noteId": "1700000000300",
                "cardIds": ["1700000000301"]
            }),
        );
        let sync_mock = rpc_ok_mock(
            &mut server,
            "collection.sync",
            json!({ "includeMedia": false }),
            json!({
                "operationId": "op-sync-blocking",
                "method": "collection.sync",
                "session": "sess-2"
            }),
        );
        let poll_mock = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-sync-blocking" }),
            json!({
                "operationId": "op-sync-blocking",
                "method": "collection.sync",
                "state": "succeeded",
                "session": "sess-2",
                "startedAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-01T00:00:01Z",
                "result": {
                    "collectionComplete": true,
                    "mediaComplete": false
                }
            }),
        );

        let client = AnkiClientBuilder::default()
            .url(format!("{}/api/v1", server.url()))
            .poll_delays(Duration::from_millis(5), Duration::from_millis(10))
            .build_blocking()
            .expect("blocking client builds");

        let mut fields = BTreeMap::new();
        fields.insert("Front".to_string(), "F".to_string());
        fields.insert("Back".to_string(), "B".to_string());

        let created = client
            .notes()
            .create(&NotesCreateParams {
                deck: Reference::name("Default"),
                note_type: Reference::name("Basic"),
                fields,
                tags: None,
                duplicates: None,
                reject_empty_questions: None,
                session: None,
            })
            .expect("notes.create succeeds");
        assert_eq!(created.note_id, "1700000000300");

        let result = client
            .collection()
            .sync_and_wait(
                &CollectionSyncParams {
                    include_media: Some(false),
                    session: None,
                },
                5,
            )
            .expect("collection.sync_and_wait succeeds");

        assert_eq!(
            result,
            SyncResult {
                collection_complete: true,
                media_complete: false,
            }
        );

        note_mock.assert();
        sync_mock.assert();
        poll_mock.assert();
    }
}

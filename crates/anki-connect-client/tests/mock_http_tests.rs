#![cfg(any(feature = "async", feature = "blocking"))]

use std::time::Duration;

use anki_connect_client::AnkiClientBuilder;
use mockito::{Mock, Server};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NoteGetParams {
    id: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct NoteResult {
    id: String,
    fields: Map<String, Value>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SyncResult {
    collection_complete: bool,
    media_complete: bool,
}

fn rpc_ok_mock(
    server: &mut Server,
    expected_method: &str,
    expected_params: Value,
    result: Value,
) -> Mock {
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
mod async_tests {
    use super::*;
    use anki_connect_client::{AnkiClient, AnkiError};
    use mockito::Matcher;

    #[test]
    fn builder_clone_debug_redacts_api_key_and_with_session() {
        let builder = AnkiClientBuilder::default()
            .url("http://127.0.0.1:9999/api/v1")
            .api_key("my-top-secret-key")
            .session("sess-a")
            .timeout(Duration::from_secs(15))
            .validation(false)
            .poll_delays(Duration::from_millis(5), Duration::from_millis(50));

        let builder_debug = format!("{builder:?}");
        assert!(builder_debug.contains("<redacted>"));
        assert!(!builder_debug.contains("my-top-secret-key"));

        let client = builder.build().expect("async client should build");
        assert_eq!(client.url(), "http://127.0.0.1:9999/api/v1");
        assert_eq!(client.session(), Some("sess-a"));

        let client_debug = format!("{client:?}");
        assert!(client_debug.contains("<redacted>"));
        assert!(!client_debug.contains("my-top-secret-key"));

        let cloned = client.clone();
        assert_eq!(cloned.url(), client.url());
        assert_eq!(cloned.session(), Some("sess-a"));

        let rebound = client.with_session("sess-b");
        assert_eq!(client.session(), Some("sess-a"));
        assert_eq!(rebound.session(), Some("sess-b"));

        let custom_http = reqwest::Client::new();
        let injected = AnkiClient::builder()
            .url("http://127.0.0.1:8765/api/v1")
            .build_with_http_client(custom_http);
        assert_eq!(injected.url(), "http://127.0.0.1:8765/api/v1");
    }

    #[tokio::test]
    async fn typed_call_and_auth_and_session_injection() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/api/v1")
            .match_header("authorization", "Bearer secret-async")
            .match_body(Matcher::PartialJson(json!({
                "jsonrpc": "2.0",
                "method": "notes.get",
                "params": { "id": "1700000000001", "session": "sess-1" }
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body_from_request(|req| {
                let parsed: Value =
                    serde_json::from_slice(req.body().expect("request body")).expect("valid json");
                json!({
                    "jsonrpc": "2.0",
                    "id": parsed["id"],
                    "result": {
                        "id": "1700000000001",
                        "fields": { "Front": "Hello", "Back": "World" }
                    }
                })
                .to_string()
                .into_bytes()
            })
            .create_async()
            .await;

        let client = AnkiClient::builder()
            .url(format!("{}/api/v1", server.url()))
            .api_key("secret-async")
            .session("sess-1")
            .build()
            .expect("client should build");

        let note: NoteResult = client
            .call(
                "notes.get",
                &NoteGetParams {
                    id: "1700000000001".into(),
                },
            )
            .await
            .expect("typed call should succeed");

        assert_eq!(note.id, "1700000000001");
        assert_eq!(note.fields["Front"], "Hello");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn wait_for_operation_typed_polls_until_succeeded() {
        let mut server = Server::new_async().await;

        let op_pending = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-1" }),
            json!({ "operationId": "op-1", "method": "collection.sync", "state": "pending" }),
        )
        .expect(1);
        let op_running = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-1" }),
            json!({ "operationId": "op-1", "method": "collection.sync", "state": "running" }),
        )
        .expect(1);
        let op_succeeded = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-1" }),
            json!({
                "operationId": "op-1",
                "method": "collection.sync",
                "state": "succeeded",
                "result": { "collectionComplete": true, "mediaComplete": true }
            }),
        )
        .expect(1);

        let client = AnkiClient::from_url(format!("{}/api/v1", server.url()))
            .expect("client should build")
            .with_session("sess-1")
            .with_poll_delays(Duration::from_millis(5), Duration::from_millis(15));

        let res: SyncResult = client
            .wait_for_operation("op-1", 5)
            .await
            .expect("operation should succeed");
        assert_eq!(
            res,
            SyncResult {
                collection_complete: true,
                media_complete: true,
            }
        );

        op_pending.assert_async().await;
        op_running.assert_async().await;
        op_succeeded.assert_async().await;
    }

    #[tokio::test]
    async fn wait_for_operation_failed_and_timeout() {
        let mut server = Server::new_async().await;

        let op_failed = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-fail" }),
            json!({
                "operationId": "op-fail",
                "method": "collection.sync",
                "state": "failed",
                "error": { "code": 1012, "message": "Sync failed" }
            }),
        )
        .expect(1);

        let op_running = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-slow" }),
            json!({ "operationId": "op-slow", "method": "collection.sync", "state": "running" }),
        )
        .expect_at_least(1);

        let client = AnkiClient::from_url(format!("{}/api/v1", server.url()))
            .expect("client should build")
            .with_poll_delays(Duration::from_millis(5), Duration::from_millis(10));

        let err = client
            .wait_for_operation_raw("op-fail", 5)
            .await
            .expect_err("failed op should return error");
        assert!(matches!(
            err,
            AnkiError::OperationFailed { ref state, ref message, .. }
                if state == "failed" && message == "Sync failed"
        ));

        let timeout_err = client
            .wait_for_operation_raw("op-slow", 0)
            .await
            .expect_err("running op with 0s timeout should time out");
        assert!(timeout_err.is_timeout());

        op_failed.assert_async().await;
        op_running.assert_async().await;
    }

    #[tokio::test]
    async fn transport_batch_and_execute_batch() {
        let mut server = Server::new_async().await;

        let transport_mock = server
            .mock("POST", "/api/v1")
            .match_body(Matcher::PartialJson(json!([
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
                    { "jsonrpc": "2.0", "id": 1, "result": [] },
                    { "jsonrpc": "2.0", "id": 2, "result": { "apiVersion": "1" } }
                ])
                .to_string(),
            )
            .create_async()
            .await;

        let execute_mock = rpc_ok_mock(
            &mut server,
            "api.execute",
            json!({
                "commands": [
                    { "method": "decks.list", "params": { "session": "sess-batch" } },
                    { "method": "api.describe", "params": {} }
                ]
            }),
            json!({
                "operationId": "op-batch-async",
                "method": "api.execute"
            }),
        )
        .expect(1);

        let client = AnkiClient::new(
            format!("{}/api/v1", server.url()),
            None,
            Some("sess-batch".to_string()),
        )
        .expect("client should build");

        let batch_reply = client
            .transport_batch(vec![
                ("decks.list".to_string(), json!({})),
                ("api.describe".to_string(), json!({})),
            ])
            .await
            .expect("transport_batch should succeed");
        assert_eq!(batch_reply.as_array().map(Vec::len), Some(2));

        let exec_reply = client
            .execute_batch(
                vec![
                    ("decks.list".to_string(), json!({})),
                    ("api.describe".to_string(), json!({})),
                ],
                false,
                30,
            )
            .await
            .expect("execute_batch should succeed");
        assert_eq!(exec_reply["operationId"], "op-batch-async");

        transport_mock.assert_async().await;
        execute_mock.assert_async().await;
    }

    #[tokio::test]
    async fn paginate_all_merges_pages() {
        let mut server = Server::new_async().await;

        let p1 = rpc_ok_mock(
            &mut server,
            "notes.search",
            json!({ "query": "deck:Default", "limit": 2, "session": "s-1" }),
            json!({ "ids": ["1", "2"], "nextCursor": "c2" }),
        )
        .expect(1);
        let p2 = rpc_ok_mock(
            &mut server,
            "notes.search",
            json!({ "query": "deck:Default", "limit": 2, "cursor": "c2", "session": "s-1" }),
            json!({ "ids": ["3"], "nextCursor": null }),
        )
        .expect(1);

        let client = AnkiClient::new(
            format!("{}/api/v1", server.url()),
            None,
            Some("s-1".to_string()),
        )
        .expect("client should build");

        let mut params = Map::new();
        params.insert("query".into(), json!("deck:Default"));
        params.insert("limit".into(), json!(2));

        let merged = client
            .paginate_all("notes.search", params, "ids")
            .await
            .expect("paginate_all should succeed");

        assert_eq!(
            merged,
            json!({
                "ids": ["1", "2", "3"],
                "pages": 2,
                "total": 3,
            })
        );

        p1.assert_async().await;
        p2.assert_async().await;
    }
}

#[cfg(feature = "blocking")]
mod blocking_tests {
    use super::*;
    use anki_connect_client::blocking::AnkiClient;

    #[test]
    fn builder_clone_debug_redacts_api_key_and_with_session() {
        let client = AnkiClient::builder()
            .url("http://127.0.0.1:9998/api/v1")
            .api_key("blocking-secret")
            .session("sess-1")
            .timeout(Duration::from_secs(10))
            .validation(false)
            .poll_delays(Duration::from_millis(5), Duration::from_millis(25))
            .build_blocking()
            .expect("blocking client should build");

        let dbg = format!("{client:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(!dbg.contains("blocking-secret"));

        let cloned = client.clone();
        assert_eq!(cloned.url(), "http://127.0.0.1:9998/api/v1");
        assert_eq!(cloned.session(), Some("sess-1"));

        let rebound = client.with_session("sess-2");
        assert_eq!(client.session(), Some("sess-1"));
        assert_eq!(rebound.session(), Some("sess-2"));

        let custom_http = reqwest::blocking::Client::new();
        let injected = AnkiClientBuilder::new()
            .url("http://127.0.0.1:8765/api/v1")
            .build_blocking_with_http_client(custom_http);
        assert_eq!(injected.url(), "http://127.0.0.1:8765/api/v1");
    }

    #[test]
    fn typed_call_and_wait_for_operation_blocking() {
        let mut server = Server::new();

        let note_mock = rpc_ok_mock(
            &mut server,
            "notes.get",
            json!({ "id": "1700000000002", "session": "s-blk" }),
            json!({
                "id": "1700000000002",
                "fields": { "Front": "Q", "Back": "A" }
            }),
        )
        .expect(1);

        let op_pending = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-blk" }),
            json!({ "operationId": "op-blk", "method": "collection.sync", "state": "pending" }),
        )
        .expect(1);
        let op_done = rpc_ok_mock(
            &mut server,
            "operations.get",
            json!({ "operationId": "op-blk" }),
            json!({
                "operationId": "op-blk",
                "method": "collection.sync",
                "state": "succeeded",
                "result": { "collectionComplete": true, "mediaComplete": false }
            }),
        )
        .expect(1);

        let client = AnkiClient::from_url(format!("{}/api/v1", server.url()))
            .expect("client should build")
            .with_session("s-blk")
            .with_poll_delays(Duration::from_millis(5), Duration::from_millis(15));

        let note: NoteResult = client
            .call(
                "notes.get",
                &NoteGetParams {
                    id: "1700000000002".into(),
                },
            )
            .expect("typed blocking call should succeed");
        assert_eq!(note.id, "1700000000002");
        assert_eq!(note.fields["Back"], "A");

        let sync: SyncResult = client
            .wait_for_operation("op-blk", 5)
            .expect("typed blocking wait_for_operation should succeed");
        assert_eq!(
            sync,
            SyncResult {
                collection_complete: true,
                media_complete: false,
            }
        );

        note_mock.assert();
        op_pending.assert();
        op_done.assert();
    }
}

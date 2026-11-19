use std::time::Duration;

use anki_skill::client::AnkiClient;
use anki_skill::commands::{execute_plan, to_rpc_plan, Cli};
use anki_skill::error::CliError;
use clap::Parser;
use mockito::{Matcher, Mock, Server};
use serde_json::{json, Value};

fn rpc_ok_mock(
    server: &mut Server,
    expected_method: &str,
    expected_params: Value,
    result: Value,
) -> Mock {
    let method = expected_method.to_string();
    server
        .mock("POST", "/")
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

#[test]
fn jsonrpc_envelope_and_bearer_auth_header() {
    let mut server = Server::new();
    let expected_result = json!([{ "id": "1", "name": "Default" }]);

    let mock = server
        .mock("POST", "/")
        .match_header("authorization", "Bearer secret-token")
        .match_header("content-type", "application/json")
        .match_body(Matcher::PartialJson(json!({
            "jsonrpc": "2.0",
            "method": "decks.list",
            "params": {}
        })))
        .match_request(|req| {
            let Ok(body) = req.body() else {
                return false;
            };
            let Ok(parsed): Result<Value, _> = serde_json::from_slice(body) else {
                return false;
            };
            parsed.get("id").and_then(Value::as_u64).is_some()
                && parsed.get("params") == Some(&json!({}))
        })
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body_from_request({
            let expected_result = expected_result.clone();
            move |req| {
                let parsed: Value =
                    serde_json::from_slice(req.body().expect("request body")).expect("valid json");
                json!({
                    "jsonrpc": "2.0",
                    "id": parsed["id"],
                    "result": expected_result,
                })
                .to_string()
                .into_bytes()
            }
        })
        .create();

    let client =
        AnkiClient::new(server.url(), Some("secret-token".into()), None).expect("client init");

    let cli = Cli::try_parse_from(["anki-cli", "decks", "list"]).expect("parse cli");
    let plan = to_rpc_plan(&cli.command).expect("build plan");
    let result = execute_plan(&client, plan).expect("execute decks.list");

    assert_eq!(result, expected_result);
    mock.assert();
}

#[test]
fn scope_aware_session_injected_for_collection_methods() {
    let mut server = Server::new();

    let decks_mock = rpc_ok_mock(
        &mut server,
        "decks.list",
        json!({ "session": "s-1" }),
        json!([{ "id": "1", "name": "Default" }]),
    );
    let notes_mock = rpc_ok_mock(
        &mut server,
        "notes.get",
        json!({ "id": "1700000000001", "session": "s-1" }),
        json!({ "id": "1700000000001", "fields": { "Front": "Q", "Back": "A" } }),
    );

    let client = AnkiClient::new(server.url(), None, Some("s-1".into())).expect("client init");

    let decks = client.call("decks.list", json!({})).expect("decks.list");
    assert_eq!(decks[0]["name"], "Default");

    let note = client
        .call("notes.get", json!({ "id": "1700000000001" }))
        .expect("notes.get");
    assert_eq!(note["id"], "1700000000001");

    decks_mock.assert();
    notes_mock.assert();
}

#[test]
fn scope_aware_session_omitted_for_application_methods() {
    let mut server = Server::new();

    let op_mock = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-1" }),
        json!({ "operationId": "op-1", "method": "collection.sync", "state": "running" }),
    );
    let describe_mock = rpc_ok_mock(
        &mut server,
        "api.describe",
        json!({}),
        json!({ "apiVersion": "1" }),
    );
    let profiles_mock = rpc_ok_mock(
        &mut server,
        "profiles.list",
        json!({}),
        json!(["User 1"]),
    );
    let access_mock = rpc_ok_mock(
        &mut server,
        "access.requestPermission",
        json!({}),
        json!({ "permission": "granted" }),
    );

    let client = AnkiClient::new(server.url(), None, Some("s-1".into())).expect("client init");

    let op = client
        .call("operations.get", json!({ "operationId": "op-1" }))
        .expect("operations.get");
    assert_eq!(op["operationId"], "op-1");

    let desc = client.call("api.describe", json!({})).expect("api.describe");
    assert_eq!(desc["apiVersion"], "1");

    let profiles = client
        .call("profiles.list", json!({}))
        .expect("profiles.list");
    assert_eq!(profiles, json!(["User 1"]));

    let access = client
        .call("access.requestPermission", json!({}))
        .expect("access.requestPermission");
    assert_eq!(access["permission"], "granted");

    op_mock.assert();
    describe_mock.assert();
    profiles_mock.assert();
    access_mock.assert();
}

#[test]
fn paginate_all_notes_and_cards_search_across_three_pages() {
    let mut server = Server::new();
    let client = AnkiClient::new(server.url(), None, Some("s-1".into())).expect("client init");

    // 1. notes.search across 3 pages
    let notes_p1 = rpc_ok_mock(
        &mut server,
        "notes.search",
        json!({ "query": "deck:Default", "limit": 2, "session": "s-1" }),
        json!({ "ids": ["101", "102"], "nextCursor": "cur-2" }),
    );
    let notes_p2 = rpc_ok_mock(
        &mut server,
        "notes.search",
        json!({ "query": "deck:Default", "limit": 2, "cursor": "cur-2", "session": "s-1" }),
        json!({ "ids": ["103", "104"], "nextCursor": "cur-3" }),
    );
    let notes_p3 = rpc_ok_mock(
        &mut server,
        "notes.search",
        json!({ "query": "deck:Default", "limit": 2, "cursor": "cur-3", "session": "s-1" }),
        json!({ "ids": ["105"], "nextCursor": null }),
    );

    let cli = Cli::try_parse_from(["anki-cli", "notes", "search", "deck:Default", "--limit", "2", "--all"])
        .expect("parse notes search");
    let plan = to_rpc_plan(&cli.command).expect("build notes search plan");
    let notes_result = execute_plan(&client, plan).expect("execute notes.search --all");
    assert_eq!(
        notes_result,
        json!({
            "ids": ["101", "102", "103", "104", "105"],
            "pages": 3,
            "total": 5,
        })
    );
    notes_p1.assert();
    notes_p2.assert();
    notes_p3.assert();

    // 2. cards.search across 3 pages
    let cards_p1 = rpc_ok_mock(
        &mut server,
        "cards.search",
        json!({ "query": "is:due", "limit": 100, "session": "s-1" }),
        json!({ "ids": ["201", "202"], "nextCursor": "cur-2" }),
    );
    let cards_p2 = rpc_ok_mock(
        &mut server,
        "cards.search",
        json!({ "query": "is:due", "limit": 100, "cursor": "cur-2", "session": "s-1" }),
        json!({ "ids": ["203", "204"], "nextCursor": "cur-3" }),
    );
    let cards_p3 = rpc_ok_mock(
        &mut server,
        "cards.search",
        json!({ "query": "is:due", "limit": 100, "cursor": "cur-3", "session": "s-1" }),
        json!({ "ids": ["205"], "nextCursor": null }),
    );

    let cli = Cli::try_parse_from(["anki-cli", "cards", "search", "is:due", "--all"])
        .expect("parse cards search");
    let plan = to_rpc_plan(&cli.command).expect("build cards search plan");
    let cards_result = execute_plan(&client, plan).expect("execute cards.search --all");
    assert_eq!(
        cards_result,
        json!({
            "ids": ["201", "202", "203", "204", "205"],
            "pages": 3,
            "total": 5,
        })
    );
    cards_p1.assert();
    cards_p2.assert();
    cards_p3.assert();
}

#[test]
fn async_operation_polls_pending_running_to_succeeded() {
    let mut server = Server::new();
    let client = AnkiClient::new(server.url(), None, Some("s-1".into()))
        .expect("client init")
        .with_poll_delays(Duration::from_millis(10), Duration::from_millis(25));

    let sync_mock = rpc_ok_mock(
        &mut server,
        "collection.sync",
        json!({ "session": "s-1" }),
        json!({ "operationId": "op-sync-1", "method": "collection.sync", "session": "s-1" }),
    );

    // operations.get is application-scoped: must NOT receive "session"
    let op_pending = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-sync-1" }),
        json!({ "operationId": "op-sync-1", "method": "collection.sync", "state": "pending" }),
    )
    .expect(1);
    let op_running = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-sync-1" }),
        json!({ "operationId": "op-sync-1", "method": "collection.sync", "state": "running" }),
    )
    .expect(1);
    let op_succeeded = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-sync-1" }),
        json!({
            "operationId": "op-sync-1",
            "method": "collection.sync",
            "state": "succeeded",
            "result": { "collectionComplete": true, "mediaComplete": true }
        }),
    )
    .expect(1);

    let result = client
        .call_and_maybe_wait("collection.sync", json!({}), true, 5)
        .expect("poll operation to succeeded");
    assert_eq!(
        result,
        json!({ "collectionComplete": true, "mediaComplete": true })
    );

    sync_mock.assert();
    op_pending.assert();
    op_running.assert();
    op_succeeded.assert();
}

#[test]
fn async_operation_no_wait_returns_handle_immediately() {
    let mut server = Server::new();
    let client = AnkiClient::new(server.url(), None, Some("s-1".into()))
        .expect("client init")
        .with_poll_delays(Duration::from_millis(10), Duration::from_millis(25));

    let handle = json!({ "operationId": "op-sync-2", "method": "collection.sync", "session": "s-1" });
    let sync_mock = rpc_ok_mock(
        &mut server,
        "collection.sync",
        json!({ "session": "s-1" }),
        handle.clone(),
    );
    let op_get_mock = server
        .mock("POST", "/")
        .match_body(Matcher::PartialJson(json!({ "method": "operations.get" })))
        .expect(0)
        .create();

    let result = client
        .call_and_maybe_wait("collection.sync", json!({}), false, 5)
        .expect("no-wait returns handle");
    assert_eq!(result, handle);

    sync_mock.assert();
    op_get_mock.assert();
}

#[test]
fn async_operation_failed_and_cancelled_return_exit_code_4() {
    let mut server = Server::new();
    let client = AnkiClient::new(server.url(), None, Some("s-1".into()))
        .expect("client init")
        .with_poll_delays(Duration::from_millis(10), Duration::from_millis(25));

    // Failed operation
    let failed_mock = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-fail" }),
        json!({
            "operationId": "op-fail",
            "method": "collection.sync",
            "state": "failed",
            "error": {
                "code": 1012,
                "message": "Network I/O error during sync",
                "data": { "type": "ioError", "retryable": true }
            }
        }),
    );

    let err = client
        .wait_for_operation("op-fail", 5)
        .expect_err("failed operation should return error");
    assert_eq!(err.exit_code(), 4);
    assert!(matches!(err, CliError::OperationFailed { .. }));
    let err_json = err.to_json();
    assert_eq!(err_json["error"]["kind"], "operation");
    assert_eq!(err_json["error"]["state"], "failed");
    assert_eq!(err_json["error"]["details"]["code"], 1012);
    assert_eq!(err_json["error"]["details"]["data"]["type"], "ioError");
    failed_mock.assert();

    // Cancelled operation
    let cancelled_mock = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-cancel" }),
        json!({
            "operationId": "op-cancel",
            "method": "collection.sync",
            "state": "cancelled",
            "error": {
                "code": 1007,
                "message": "Operation was cancelled",
                "data": { "type": "cancelled" }
            }
        }),
    );

    let err = client
        .wait_for_operation("op-cancel", 5)
        .expect_err("cancelled operation should return error");
    assert_eq!(err.exit_code(), 4);
    assert!(matches!(err, CliError::OperationFailed { .. }));
    let err_json = err.to_json();
    assert_eq!(err_json["error"]["kind"], "operation");
    assert_eq!(err_json["error"]["state"], "cancelled");
    assert_eq!(err_json["error"]["details"]["code"], 1007);
    assert_eq!(err_json["error"]["details"]["data"]["type"], "cancelled");
    cancelled_mock.assert();
}

#[test]
fn async_operation_polling_timeout_returns_exit_code_5() {
    let mut server = Server::new();
    let client = AnkiClient::new(server.url(), None, Some("s-1".into()))
        .expect("client init")
        .with_poll_delays(Duration::from_millis(10), Duration::from_millis(25));

    let op_running = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-timeout" }),
        json!({ "operationId": "op-timeout", "method": "collection.sync", "state": "running" }),
    )
    .expect_at_least(1);

    // timeout_secs = 0 triggers immediate deadline check after first non-terminal poll
    let err = client
        .wait_for_operation("op-timeout", 0)
        .expect_err("polling timeout should return Timeout");
    assert_eq!(err.exit_code(), 5);
    assert!(matches!(err, CliError::Timeout { .. }));
    let err_json = err.to_json();
    assert_eq!(err_json["error"]["kind"], "timeout");
    assert_eq!(err_json["error"]["operationId"], "op-timeout");
    op_running.assert();
}

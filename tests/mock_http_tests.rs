use anki_skill::client::AnkiClient;
use anki_skill::commands::{execute_plan, to_rpc_plan, Cli};
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

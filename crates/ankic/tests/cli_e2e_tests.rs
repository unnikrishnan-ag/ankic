use assert_cmd::Command;
use mockito::Server;
use serde_json::{json, Value};

fn anki_cmd() -> Command {
    Command::cargo_bin("ankic").expect("ankic binary should exist")
}

fn rpc_ok_mock(
    server: &mut Server,
    expected_method: &str,
    expected_params: Value,
    result: Value,
) -> mockito::Mock {
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
            json!({
                "jsonrpc": "2.0",
                "id": parsed["id"],
                "result": result,
            })
            .to_string()
            .into_bytes()
        })
        .create()
}

fn parse_yaml(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).expect("valid utf-8 output");
    serde_norway::from_str::<Value>(text).expect("valid yaml output")
}

fn parse_json(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).expect("valid utf-8 output");
    serde_json::from_str::<Value>(text).expect("valid json output")
}

#[test]
fn default_yaml_output_for_offline_schema_and_mocked_rpc() {
    let schema_out = anki_cmd()
        .args(["schema", "notes.create"])
        .output()
        .expect("run schema notes.create");
    assert_eq!(schema_out.status.code(), Some(0));

    let schema_stdout = std::str::from_utf8(&schema_out.stdout).expect("utf-8");
    assert!(
        !schema_stdout.trim_start().starts_with('{'),
        "default output should be YAML, not JSON: {schema_stdout}"
    );
    let schema_val = parse_yaml(&schema_out.stdout);
    assert_eq!(schema_val["method"], "notes.create");
    assert!(schema_val.get("$defs").is_some());

    let mut server = Server::new();
    let mock = rpc_ok_mock(
        &mut server,
        "decks.list",
        json!({}),
        json!([{ "id": "1", "name": "Default" }]),
    );

    let rpc_out = anki_cmd()
        .args(["--url", &server.url(), "decks", "list"])
        .output()
        .expect("run decks list");
    assert_eq!(rpc_out.status.code(), Some(0));
    mock.assert();

    let rpc_stdout = std::str::from_utf8(&rpc_out.stdout).expect("utf-8");
    assert!(
        !rpc_stdout.trim_start().starts_with('['),
        "default list output should be YAML, not JSON: {rpc_stdout}"
    );
    let rpc_val = parse_yaml(&rpc_out.stdout);
    assert_eq!(rpc_val, json!([{ "id": "1", "name": "Default" }]));
}

#[test]
fn format_json_and_json_pretty_plus_aliases() {
    for flag in ["--format=json", "--json"] {
        let out = anki_cmd()
            .args([flag, "schema", "notes.create"])
            .output()
            .expect("run schema notes.create with json");
        assert_eq!(out.status.code(), Some(0));

        let stdout = std::str::from_utf8(&out.stdout).expect("utf-8");
        let trimmed = stdout.trim();
        assert!(trimmed.starts_with('{'));
        assert!(
            !trimmed.contains('\n'),
            "compact JSON should be single-line for {flag}"
        );
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed["method"], "notes.create");
    }

    let mut server = Server::new();
    let mock = rpc_ok_mock(
        &mut server,
        "decks.list",
        json!({}),
        json!([{ "id": "1", "name": "Default" }]),
    )
    .expect(2);

    for args in [
        vec!["--format", "json-pretty", "--url"],
        vec!["--pretty", "--url"],
    ] {
        let mut cmd_args = args;
        let url = server.url();
        cmd_args.push(&url);
        cmd_args.extend(["decks", "list"]);

        let out = anki_cmd().args(&cmd_args).output().expect("run decks list");
        assert_eq!(out.status.code(), Some(0));

        let stdout = std::str::from_utf8(&out.stdout).expect("utf-8");
        assert!(
            stdout.contains("\n  "),
            "pretty JSON should be indented: {stdout}"
        );
        let parsed = parse_json(&out.stdout);
        assert_eq!(parsed, json!([{ "id": "1", "name": "Default" }]));
    }

    mock.assert();
}

#[test]
fn yaml_and_json_inputs_for_fields_params_and_stdin() {
    let mut server = Server::new();
    let expected_create_params = json!({
        "deck": { "name": "Default" },
        "noteType": { "name": "Basic" },
        "fields": { "Front": "猫", "Back": "cat" },
    });
    let create_mock = rpc_ok_mock(
        &mut server,
        "notes.create",
        expected_create_params,
        json!({ "noteId": "1700000000001", "cardIds": ["1700000000002"] }),
    )
    .expect(2);

    let yaml_fields_out = anki_cmd()
        .args([
            "--url",
            &server.url(),
            "notes",
            "create",
            "--deck",
            "Default",
            "--note-type",
            "Basic",
            "--fields",
            "Front: 猫\nBack: cat",
        ])
        .output()
        .expect("run notes create with yaml fields");
    assert_eq!(yaml_fields_out.status.code(), Some(0));
    assert_eq!(
        parse_yaml(&yaml_fields_out.stdout)["noteId"],
        "1700000000001"
    );

    let json_fields_out = anki_cmd()
        .args([
            "--url",
            &server.url(),
            "notes",
            "create",
            "--deck",
            "Default",
            "--note-type",
            "Basic",
            "--fields",
            r#"{"Front":"猫","Back":"cat"}"#,
        ])
        .output()
        .expect("run notes create with json fields");
    assert_eq!(json_fields_out.status.code(), Some(0));
    assert_eq!(
        parse_yaml(&json_fields_out.stdout)["noteId"],
        "1700000000001"
    );
    create_mock.assert();

    let get_mock = rpc_ok_mock(
        &mut server,
        "notes.get",
        json!({ "id": "1700000000001" }),
        json!({ "id": "1700000000001", "tags": ["vocab"] }),
    )
    .expect(2);

    let stdin_out = anki_cmd()
        .args(["--url", &server.url(), "call", "notes.get", "--stdin"])
        .write_stdin("id: \"1700000000001\"\n")
        .output()
        .expect("run call notes.get --stdin");
    assert_eq!(stdin_out.status.code(), Some(0));
    assert_eq!(parse_yaml(&stdin_out.stdout)["id"], "1700000000001");

    let params_out = anki_cmd()
        .args([
            "--url",
            &server.url(),
            "call",
            "notes.get",
            "--params",
            "id: '1700000000001'",
        ])
        .output()
        .expect("run call notes.get --params");
    assert_eq!(params_out.status.code(), Some(0));
    assert_eq!(parse_yaml(&params_out.stdout)["id"], "1700000000001");
    get_mock.assert();
}

#[test]
fn exit_code_2_for_invalid_input_and_preflight_validation_errors() {
    let unknown_schema = anki_cmd()
        .args(["schema", "nonexistent.method"])
        .output()
        .expect("run schema nonexistent.method");
    assert_eq!(unknown_schema.status.code(), Some(2));
    assert!(unknown_schema.stdout.is_empty());
    let yaml_err = parse_yaml(&unknown_schema.stderr);
    assert_eq!(yaml_err["error"]["kind"], "cli");

    let invalid_params = anki_cmd()
        .args([
            "--format",
            "json",
            "call",
            "notes.get",
            "--params",
            "id: 123",
        ])
        .output()
        .expect("run call notes.get with invalid params");
    assert_eq!(invalid_params.status.code(), Some(2));
    assert!(invalid_params.stdout.is_empty());
    let json_err = parse_json(&invalid_params.stderr);
    assert_eq!(json_err["error"]["kind"], "cli");
}

#[test]
fn exit_code_3_for_http_401_and_403() {
    let mut server = Server::new();
    let mock_401 = server
        .mock("POST", "/")
        .with_status(401)
        .with_body("Unauthorized")
        .create();

    let out_401 = anki_cmd()
        .args(["--url", &server.url(), "decks", "list"])
        .output()
        .expect("run decks list against 401");
    assert_eq!(out_401.status.code(), Some(3));
    mock_401.assert();
    let err_401 = parse_yaml(&out_401.stderr);
    assert_eq!(err_401["error"]["kind"], "http");
    assert_eq!(err_401["error"]["status"], 401);

    let mock_403 = server
        .mock("POST", "/")
        .with_status(403)
        .with_body("Forbidden")
        .create();

    let out_403 = anki_cmd()
        .args(["--format", "json", "--url", &server.url(), "decks", "list"])
        .output()
        .expect("run decks list against 403");
    assert_eq!(out_403.status.code(), Some(3));
    mock_403.assert();
    let err_403 = parse_json(&out_403.stderr);
    assert_eq!(err_403["error"]["kind"], "http");
    assert_eq!(err_403["error"]["status"], 403);
}

#[test]
fn exit_code_4_for_rpc_error_1003_and_failed_operation() {
    let mut server = Server::new();
    let rpc_err_mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body_from_request(|req| {
            let parsed: Value =
                serde_json::from_slice(req.body().expect("request body")).expect("valid json");
            json!({
                "jsonrpc": "2.0",
                "id": parsed["id"],
                "error": {
                    "code": 1003,
                    "message": "Editor conflict.",
                    "data": {
                        "type": "editorConflict",
                        "retryable": false,
                    }
                }
            })
            .to_string()
            .into_bytes()
        })
        .create();

    let rpc_out = anki_cmd()
        .args([
            "--url",
            &server.url(),
            "notes",
            "update",
            "1700000000001",
            "--fields",
            "Front: updated",
        ])
        .output()
        .expect("run notes update against 1003");
    assert_eq!(rpc_out.status.code(), Some(4));
    rpc_err_mock.assert();

    let rpc_err = parse_yaml(&rpc_out.stderr);
    assert_eq!(rpc_err["error"]["kind"], "rpc");
    assert_eq!(rpc_err["error"]["code"], 1003);
    assert_eq!(rpc_err["error"]["data"]["type"], "editorConflict");

    let failed_op_mock = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-fail" }),
        json!({
            "operationId": "op-fail",
            "method": "collection.sync",
            "state": "failed",
            "error": {
                "code": 1012,
                "message": "Sync I/O failure",
                "data": { "type": "ioError", "retryable": true }
            }
        }),
    );

    let op_out = anki_cmd()
        .args(["--url", &server.url(), "operations", "wait", "op-fail"])
        .output()
        .expect("run operations wait op-fail");
    assert_eq!(op_out.status.code(), Some(4));
    failed_op_mock.assert();

    let op_err = parse_yaml(&op_out.stderr);
    assert_eq!(op_err["error"]["kind"], "operation");
    assert_eq!(op_err["error"]["state"], "failed");
    assert_eq!(op_err["error"]["details"]["code"], 1012);
}

#[test]
fn exit_code_5_for_operation_polling_timeout() {
    let mut server = Server::new();
    let running_mock = rpc_ok_mock(
        &mut server,
        "operations.get",
        json!({ "operationId": "op-1" }),
        json!({
            "operationId": "op-1",
            "method": "collection.sync",
            "state": "running",
        }),
    );

    let out = anki_cmd()
        .args([
            "--url",
            &server.url(),
            "operations",
            "wait",
            "op-1",
            "--timeout",
            "0",
        ])
        .output()
        .expect("run operations wait op-1 --timeout 0");
    assert_eq!(out.status.code(), Some(5));
    running_mock.assert();

    let err = parse_yaml(&out.stderr);
    assert_eq!(err["error"]["kind"], "timeout");
    assert_eq!(err["error"]["operationId"], "op-1");
    assert_eq!(err["error"]["timeoutSecs"], 0);
}

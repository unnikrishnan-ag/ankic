use std::collections::BTreeMap;
use std::sync::LazyLock;

use ankic::commands::{to_rpc_plan, BatchMode, Cli, RpcPlan};
use ankic::schema::{method_scope, validate_method_params};
use clap::Parser;
use serde_json::{json, Value};

static ROOT_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../schemas/api-v1.schema.json"))
        .expect("schemas/api-v1.schema.json must be valid JSON")
});

struct MethodValidators {
    params: jsonschema::Validator,
    request: jsonschema::Validator,
}

static METHOD_VALIDATORS: LazyLock<BTreeMap<String, MethodValidators>> = LazyLock::new(|| {
    let root = &*ROOT_SCHEMA;
    let defs = root
        .get("$defs")
        .cloned()
        .expect("root schema must contain $defs");
    let methods = root
        .get("x-methods")
        .and_then(Value::as_object)
        .expect("root schema must contain x-methods object");

    let mut map = BTreeMap::new();
    for (method, spec) in methods {
        let params_sub = build_subschema(
            spec.get("params").expect("method must define params"),
            &defs,
        );
        let request_sub = build_subschema(
            spec.get("request").expect("method must define request"),
            &defs,
        );

        let params_validator = jsonschema::draft202012::new(&params_sub)
            .unwrap_or_else(|e| panic!("failed to compile params schema for {method}: {e}"));
        let request_validator = jsonschema::draft202012::new(&request_sub)
            .unwrap_or_else(|e| panic!("failed to compile request schema for {method}: {e}"));

        map.insert(
            method.clone(),
            MethodValidators {
                params: params_validator,
                request: request_validator,
            },
        );
    }
    map
});

fn build_subschema(target: &Value, defs: &Value) -> Value {
    let mut obj = match target.as_object() {
        Some(map) => map.clone(),
        None => panic!("schema target must be an object: {target}"),
    };
    obj.insert(
        "$schema".to_string(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    obj.insert("$defs".to_string(), defs.clone());
    Value::Object(obj)
}

fn validate_single_mode(
    validators: &MethodValidators,
    method: &str,
    params: &Value,
    mode_label: &str,
) -> Result<(), String> {
    // 1. Validate params against x-methods.<method>.params
    if let Err(err) = validators.params.validate(params) {
        return Err(format!(
            "[{mode_label}] params schema validation failed for '{method}': {err} (params={params})"
        ));
    }

    // 2. Validate full JSON-RPC 2.0 request envelope against x-methods.<method>.request
    let envelope = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    if let Err(err) = validators.request.validate(&envelope) {
        return Err(format!(
            "[{mode_label}] request envelope schema validation failed for '{method}': {err} (envelope={envelope})"
        ));
    }

    // 3. Verify client-side pre-flight validation agrees with JSON Schema
    validate_method_params(method, params).map_err(|e| {
        format!(
            "[{mode_label}] client-side validate_method_params failed for '{method}': {e} (params={params})"
        )
    })?;

    Ok(())
}

/// Apply scope-aware session injection matching `AnkiClient::prepare_params`.
fn inject_session_for_scope(method: &str, raw_params: &Value, session_id: &str) -> Value {
    if method_scope(method) != Some("collection") {
        return raw_params.clone();
    }
    match raw_params {
        Value::Null => json!({ "session": session_id }),
        Value::Object(map) => {
            let mut next = map.clone();
            next.entry("session".to_string())
                .or_insert_with(|| Value::String(session_id.to_string()));
            Value::Object(next)
        }
        other => other.clone(),
    }
}

/// Validate `(method, raw_params)` against `schemas/api-v1.schema.json` and
/// `ankic::schema::validate_method_params` in both session modes:
/// 1. Without a session
/// 2. With `--session "test-session"` applied via scope-aware session injection
pub fn validate_rpc_contract(method: &str, raw_params: &Value) -> Result<(), String> {
    let validators = METHOD_VALIDATORS
        .get(method)
        .ok_or_else(|| format!("Unknown method '{method}' not found in x-methods"))?;

    // Mode 1: Without session
    validate_single_mode(validators, method, raw_params, "no-session")?;

    // Mode 2: With scope-aware session injection
    let with_session = inject_session_for_scope(method, raw_params, "test-session");
    if method_scope(method) == Some("application") {
        assert!(
            with_session.get("session").is_none(),
            "application-scoped method '{method}' must not receive session"
        );
    }
    validate_single_mode(validators, method, &with_session, "with-session")?;

    Ok(())
}

/// Parse CLI args, convert to `RpcPlan`, validate every emitted `(method, params)`
/// pair against the JSON Schema contract, and return the exercised method names.
pub fn assert_cli_args_conform(args: &[&str]) -> Vec<String> {
    let cli = Cli::try_parse_from(args)
        .unwrap_or_else(|e| panic!("failed to parse CLI args {args:?}: {e}"));
    let plan = to_rpc_plan(&cli.command)
        .unwrap_or_else(|e| panic!("failed to build RpcPlan for {args:?}: {e}"));

    match plan {
        RpcPlan::Local(_) => Vec::new(),
        RpcPlan::Call { method, params, .. } => {
            validate_rpc_contract(&method, &params)
                .unwrap_or_else(|e| panic!("contract failed for args {args:?}: {e}"));
            vec![method]
        }
        RpcPlan::Paginate { method, params, .. } => {
            let params_val = Value::Object(params);
            validate_rpc_contract(&method, &params_val)
                .unwrap_or_else(|e| panic!("contract failed for args {args:?}: {e}"));
            vec![method]
        }
        RpcPlan::WaitOperation { operation_id, .. } => {
            let params = json!({ "operationId": operation_id });
            validate_rpc_contract("operations.get", &params)
                .unwrap_or_else(|e| panic!("contract failed for args {args:?}: {e}"));
            vec!["operations.get".to_string()]
        }
        RpcPlan::Batch {
            mode, requests, ..
        } => {
            let mut methods = Vec::new();
            let mut execute_items = Vec::new();
            for (method, params) in &requests {
                validate_rpc_contract(method, params).unwrap_or_else(|e| {
                    panic!("batch inner contract failed for args {args:?}: {e}")
                });
                methods.push(method.clone());
                execute_items.push(json!({
                    "method": method,
                    "params": params,
                }));
            }
            if mode == BatchMode::Execute {
                let exec_params = json!({ "commands": execute_items });
                validate_rpc_contract("api.execute", &exec_params).unwrap_or_else(|e| {
                    panic!("api.execute contract failed for args {args:?}: {e}")
                });
                methods.push("api.execute".to_string());
            }
            methods
        }
    }
}

#[test]
fn schema_x_examples_pass_contract_validation() {
    let examples = ROOT_SCHEMA
        .get("x-examples")
        .and_then(Value::as_array)
        .expect("x-examples must be an array");
    assert!(!examples.is_empty(), "x-examples should not be empty");

    for (idx, ex) in examples.iter().enumerate() {
        let method = ex
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("x-examples[{idx}] missing method"));
        let params = ex
            .get("request")
            .and_then(|r| r.get("params"))
            .unwrap_or_else(|| panic!("x-examples[{idx}] missing request.params"));

        validate_rpc_contract(method, params).unwrap_or_else(|e| {
            panic!("x-examples[{idx}] ({method}) failed validation: {e}")
        });
    }
}

#[test]
fn schema_x_invalid_examples_are_rejected() {
    let invalid_examples = ROOT_SCHEMA
        .get("x-invalidExamples")
        .and_then(Value::as_array)
        .expect("x-invalidExamples must be an array");
    assert!(
        !invalid_examples.is_empty(),
        "x-invalidExamples should not be empty"
    );

    for (idx, ex) in invalid_examples.iter().enumerate() {
        let method = ex
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("x-invalidExamples[{idx}] missing method"));
        let params = ex
            .get("params")
            .unwrap_or_else(|| panic!("x-invalidExamples[{idx}] missing params"));

        let validators = METHOD_VALIDATORS
            .get(method)
            .unwrap_or_else(|| panic!("unknown method '{method}' in x-invalidExamples[{idx}]"));

        assert!(
            validators.params.validate(params).is_err(),
            "x-invalidExamples[{idx}] ({method}) unexpectedly passed jsonschema validation: {params}"
        );
        assert!(
            validate_method_params(method, params).is_err(),
            "x-invalidExamples[{idx}] ({method}) unexpectedly passed validate_method_params: {params}"
        );
    }
}

pub(crate) fn core_domain_cli_cases() -> &'static [&'static [&'static str]] {
    &[
        // Escape hatches (2 RPC methods)
        &["ankic", "describe"],
        &["ankic", "call", "decks.list"],
        &[
            "ankic",
            "call",
            "notes.get",
            "--params",
            "id: '1700000000001'",
        ],
        &[
            "ankic",
            "call",
            "notes.get",
            "--params",
            r#"{"id":"1700000000001"}"#,
        ],
        &[
            "ankic",
            "batch",
            "--mode",
            "execute",
            "--commands",
            r#"[{"method":"notes.create","params":{"deck":{"name":"Default"},"noteType":{"name":"Basic"},"fields":{"Front":"Q","Back":"A"}}},{"method":"collection.sync","params":{}}]"#,
        ],
        &[
            "ankic",
            "batch",
            "--mode",
            "transport",
            "--commands",
            "- method: decks.list\n  params: {}\n- method: api.describe\n  params: {}\n",
        ],
        // collection (5 methods)
        &["ankic", "collection", "get"],
        &["ankic", "collection", "sync"],
        &[
            "ankic",
            "collection",
            "sync",
            "--include-media",
            "--no-wait",
        ],
        &["ankic", "collection", "refresh"],
        &[
            "ankic",
            "collection",
            "refresh",
            "--target",
            "cachesAndWindows",
        ],
        &["ankic", "collection", "check-integrity"],
        &["ankic", "collection", "undo"],
        // operations (2 methods + wait)
        &["ankic", "operations", "get", "op-1"],
        &["ankic", "operations", "cancel", "op-1"],
        &["ankic", "operations", "wait", "op-1"],
        // decks (6 methods)
        &["ankic", "decks", "list"],
        &["ankic", "decks", "get", "--name", "Default"],
        &["ankic", "decks", "get", "--id", "1"],
        &["ankic", "decks", "create", "Japanese::Vocab"],
        &[
            "ankic",
            "decks",
            "update",
            "--name",
            "OldDeck",
            "--new-name",
            "NewDeck",
        ],
        &[
            "ankic",
            "decks",
            "update",
            "--id",
            "1",
            "--new-name",
            "RenamedDeck",
        ],
        &[
            "ankic",
            "decks",
            "delete",
            "--name",
            "TempDeck",
            "--delete-cards",
        ],
        &[
            "ankic",
            "decks",
            "counts",
            "--name",
            "Default",
            "--include-children",
        ],
        // deck-presets (6 methods)
        &["ankic", "deck-presets", "list"],
        &["ankic", "deck-presets", "get", "--name", "Default"],
        &["ankic", "deck-presets", "get", "--id", "1"],
        &[
            "ankic",
            "deck-presets",
            "create",
            "FastReview",
            "--clone-from-name",
            "Default",
        ],
        &[
            "ankic",
            "deck-presets",
            "update",
            "--name",
            "FastReview",
            "--new-name",
            "FastReviewV2",
            "--settings",
            "newPerDay: 30",
        ],
        &[
            "ankic",
            "deck-presets",
            "assign",
            "--preset-name",
            "Default",
            "--deck-name",
            "Japanese",
            "--reschedule-fsrs",
        ],
        &[
            "ankic",
            "deck-presets",
            "delete",
            "--name",
            "FastReviewV2",
            "--replacement-name",
            "Default",
        ],
        // notes (12 methods)
        &["ankic", "notes", "get", "1700000000001"],
        &[
            "ankic",
            "notes",
            "get-many",
            "1700000000001",
            "1700000000002",
        ],
        &["ankic", "notes", "search", "deck:Default", "--limit", "10"],
        &["ankic", "notes", "search", "deck:Default", "--all"],
        &[
            "ankic",
            "notes",
            "validate",
            "--deck",
            "Default",
            "--note-type",
            "Basic",
            "--fields",
            "Front: hello\nBack: world",
            "--tag",
            "vocab",
        ],
        &[
            "ankic",
            "notes",
            "validate-many",
            "--notes",
            r#"[{"deck":{"name":"Default"},"noteType":{"name":"Basic"},"fields":{"Front":"Q","Back":"A"}}]"#,
        ],
        &[
            "ankic",
            "notes",
            "create",
            "--deck",
            "Default",
            "--note-type",
            "Basic",
            "--fields",
            r#"{"Front":"猫","Back":"cat"}"#,
            "--tag",
            "jlpt",
        ],
        &[
            "ankic",
            "notes",
            "create",
            "--deck-id",
            "1",
            "--note-type-id",
            "1700000000000",
            "--fields",
            "Front: 犬\nBack: dog",
        ],
        &[
            "ankic",
            "notes",
            "create-many",
            "--notes",
            "[{deck: {name: Default}, noteType: {name: Basic}, fields: {Front: Q1, Back: A1}}]",
            "--atomic",
        ],
        &[
            "ankic",
            "notes",
            "update",
            "1700000000001",
            "--fields",
            "Back: updated",
            "--tag",
            "reviewed",
        ],
        &["ankic", "notes", "delete", "1700000000001"],
        &[
            "ankic",
            "notes",
            "plan-conversion",
            "--source-name",
            "Basic",
            "--target-name",
            "Basic (and reversed card)",
        ],
        &[
            "ankic",
            "notes",
            "convert",
            "1700000000001",
            "--target-name",
            "Basic",
            "--fields",
            r#"[{"target":"Front","source":"Front"}]"#,
            "--templates",
            r#"[{"target":"Card 1","source":"Card 1"}]"#,
            "--plan-token",
            "plan-tok-1",
        ],
        &[
            "ankic",
            "notes",
            "find-empty",
            "--criterion",
            "allFieldsEmpty",
            "--query",
            "deck:Default",
        ],
        // cards (7 methods)
        &["ankic", "cards", "get", "1700000000101"],
        &[
            "ankic",
            "cards",
            "get-many",
            "1700000000101",
            "1700000000102",
        ],
        &["ankic", "cards", "search", "is:due", "--limit", "25"],
        &["ankic", "cards", "search", "is:due", "--all"],
        &["ankic", "cards", "render", "1700000000101"],
        &[
            "ankic",
            "cards",
            "move",
            "--deck",
            "Archive",
            "1700000000101",
        ],
        &["ankic", "cards", "suspend", "1700000000101"],
        &["ankic", "cards", "unsuspend", "1700000000101"],
        &[
            "ankic",
            "cards",
            "flags",
            "--flag",
            "2",
            "1700000000101",
        ],
        // tags (5 methods)
        &["ankic", "tags", "list"],
        &[
            "ankic",
            "tags",
            "add",
            "--note-id",
            "1700000000001",
            "--tag",
            "important",
        ],
        &[
            "ankic",
            "tags",
            "remove",
            "--note-id",
            "1700000000001",
            "--tag",
            "important",
        ],
        &[
            "ankic",
            "tags",
            "replace",
            "--from",
            "old_tag",
            "--to",
            "new_tag",
            "--note-id",
            "1700000000001",
        ],
        &[
            "ankic",
            "tags",
            "replace",
            "--from",
            "old_tag",
            "--to",
            "new_tag",
            "--all-notes",
        ],
        &["ankic", "tags", "delete-unused"],
        // note-types (8 methods)
        &["ankic", "note-types", "list"],
        &["ankic", "note-types", "get", "--name", "Basic"],
        &["ankic", "note-types", "get", "--id", "1700000000000"],
        &[
            "ankic",
            "note-types",
            "create",
            "--name",
            "Vocab",
            "--fields",
            r#"["Word","Meaning"]"#,
            "--templates",
            r#"[{"name":"Recognition","questionHtml":"{{Word}}","answerHtml":"{{FrontSide}}<hr>{{Meaning}}"}]"#,
        ],
        &[
            "ankic",
            "note-types",
            "update",
            "--name",
            "Vocab",
            "--css",
            ".card { font-size: 20px; }",
        ],
        &[
            "ankic",
            "note-types",
            "edit-structure",
            "--name",
            "Vocab",
            "--edits",
            r#"[{"op":"addField","field":{"name":"Example"}}]"#,
        ],
        &[
            "ankic",
            "note-types",
            "template-usage",
            "--name",
            "Basic",
        ],
        &[
            "ankic",
            "note-types",
            "replace-text",
            "--find",
            "20px",
            "--replace",
            "22px",
            "--name",
            "Vocab",
            "--target",
            "css",
        ],
        &[
            "ankic",
            "note-types",
            "replace-text",
            "--find",
            "20px",
            "--replace",
            "22px",
            "--all-note-types",
        ],
        &[
            "ankic",
            "note-types",
            "delete-unused",
            "--name",
            "UnusedType",
        ],
    ]
}

#[test]
fn escape_hatches_and_schema_local_contract() {
    // Local schema listing
    let cli_list = Cli::try_parse_from(["ankic", "schema"]).unwrap();
    match to_rpc_plan(&cli_list.command).unwrap() {
        RpcPlan::Local(val) => {
            assert_eq!(val["count"], 96);
            assert!(val["methods"].as_array().is_some());
        }
        other => panic!("expected RpcPlan::Local for `schema`, got {other:?}"),
    }

    // Local schema method inspection
    let cli_method = Cli::try_parse_from(["ankic", "schema", "notes.create"]).unwrap();
    match to_rpc_plan(&cli_method.command).unwrap() {
        RpcPlan::Local(val) => {
            assert_eq!(val["method"], "notes.create");
            assert_eq!(val["scope"], "collection");
            assert!(val["$defs"].is_object());
        }
        other => panic!("expected RpcPlan::Local for `schema notes.create`, got {other:?}"),
    }

    // Local schema errors and limits
    let cli_errors = Cli::try_parse_from(["ankic", "schema", "--errors"]).unwrap();
    match to_rpc_plan(&cli_errors.command).unwrap() {
        RpcPlan::Local(val) => {
            assert!(val["errors"].is_object());
            assert!(val["limits"].is_object());
        }
        other => panic!("expected RpcPlan::Local for `schema --errors`, got {other:?}"),
    }

    // Paginate plan check
    let cli_paginate =
        Cli::try_parse_from(["ankic", "notes", "search", "deck:Default", "--all"]).unwrap();
    assert!(matches!(
        to_rpc_plan(&cli_paginate.command).unwrap(),
        RpcPlan::Paginate { .. }
    ));

    // WaitOperation plan check
    let cli_wait = Cli::try_parse_from(["ankic", "operations", "wait", "op-1"]).unwrap();
    assert!(matches!(
        to_rpc_plan(&cli_wait.command).unwrap(),
        RpcPlan::WaitOperation { .. }
    ));
}

#[test]
fn core_domain_subcommands_conform_to_schema() {
    let mut covered = std::collections::BTreeSet::new();
    for args in core_domain_cli_cases() {
        for method in assert_cli_args_conform(args) {
            covered.insert(method);
        }
    }

    // 2 escape hatches + 5 collection + 2 operations + 6 decks + 6 deckPresets
    // + 12 notes + 7 cards + 5 tags + 8 noteTypes = 53 methods
    let expected_core_methods = [
        "api.describe",
        "api.execute",
        "collection.get",
        "collection.sync",
        "collection.refresh",
        "collection.checkIntegrity",
        "collection.undo",
        "operations.get",
        "operations.cancel",
        "decks.list",
        "decks.get",
        "decks.create",
        "decks.update",
        "decks.delete",
        "decks.getCounts",
        "deckPresets.list",
        "deckPresets.get",
        "deckPresets.create",
        "deckPresets.update",
        "deckPresets.assign",
        "deckPresets.delete",
        "notes.get",
        "notes.getMany",
        "notes.search",
        "notes.validate",
        "notes.validateMany",
        "notes.create",
        "notes.createMany",
        "notes.update",
        "notes.delete",
        "notes.planConversion",
        "notes.convert",
        "notes.findEmpty",
        "cards.get",
        "cards.getMany",
        "cards.search",
        "cards.render",
        "cards.move",
        "cards.setSuspended",
        "cards.setFlags",
        "tags.list",
        "tags.add",
        "tags.remove",
        "tags.replace",
        "tags.deleteUnused",
        "noteTypes.list",
        "noteTypes.get",
        "noteTypes.create",
        "noteTypes.update",
        "noteTypes.editStructure",
        "noteTypes.getTemplateUsage",
        "noteTypes.replaceText",
        "noteTypes.deleteUnused",
    ];
    for method in expected_core_methods {
        assert!(
            covered.contains(method),
            "expected core method '{method}' to be covered by core_domain_cli_cases"
        );
    }
}

fn extended_domain_cli_cases() -> &'static [&'static [&'static str]] {
    &[
        // media (5 methods)
        &["ankic", "media", "list"],
        &["ankic", "media", "list", "--pattern", "*.png"],
        &["ankic", "media", "get", "cat.png"],
        &[
            "ankic",
            "media",
            "store",
            "--filename",
            "cat.png",
            "--data",
            "aGVsbG8=",
            "--collision",
            "rename",
            "--skip-hash-alg",
            "sha256",
            "--skip-hash-digest",
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ],
        &[
            "ankic",
            "media",
            "store",
            "--filename",
            "audio.mp3",
            "--path",
            "/tmp/audio.mp3",
            "--collision",
            "replace",
        ],
        &[
            "ankic",
            "media",
            "store",
            "--filename",
            "remote.jpg",
            "--source-url",
            "https://example.com/remote.jpg",
            "--collision",
            "error",
        ],
        &["ankic", "media", "delete", "cat.png"],
        &["ankic", "media", "directory"],
        // packages (2 methods)
        &["ankic", "packages", "import", "/tmp/deck.apkg"],
        &[
            "ankic",
            "packages",
            "export",
            "--name",
            "Default",
            "--path",
            "/tmp/out.apkg",
            "--include-scheduling",
        ],
        &[
            "ankic",
            "packages",
            "export",
            "--id",
            "1",
            "--path",
            "/tmp/out.apkg",
        ],
        // scheduling (9 methods)
        &["ankic", "scheduling", "get", "1700000000010"],
        &["ankic", "scheduling", "queue"],
        &["ankic", "scheduling", "queue", "--fetch-limit", "25"],
        &["ankic", "scheduling", "predict", "1700000000010"],
        &["ankic", "scheduling", "due", "1700000000010"],
        &[
            "ankic",
            "scheduling",
            "due",
            "1700000000010",
            "--include-learn-ahead",
        ],
        &[
            "ankic",
            "scheduling",
            "answer",
            "1700000000010",
            "--rating",
            "good",
            "--state-token",
            "tok-1",
            "--elapsed-ms",
            "2400",
        ],
        &[
            "ankic",
            "scheduling",
            "answer-many",
            "--answers",
            r#"[{"cardId":"1700000000010","rating":"good","stateToken":"tok-1"}]"#,
        ],
        &[
            "ankic",
            "scheduling",
            "answer-many",
            "--answers=- cardId: '1700000000010'\n  rating: easy\n  stateToken: tok-2\n  elapsedMs: 1200",
        ],
        &[
            "ankic",
            "scheduling",
            "reposition",
            "--starting-from",
            "10",
            "--step-size",
            "5",
            "--randomize",
            "--shift-existing",
            "1700000000010",
            "1700000000011",
        ],
        &[
            "ankic",
            "scheduling",
            "reset",
            "--no-restore-position",
            "--reset-counts",
            "--no-log",
            "1700000000010",
        ],
        &[
            "ankic",
            "scheduling",
            "set-due-date",
            "--days",
            "3-7!",
            "1700000000010",
        ],
        // reviews (2 methods)
        &["ankic", "reviews", "list"],
        &[
            "ankic",
            "reviews",
            "list",
            "--card-id",
            "1700000000010",
            "--after-id",
            "1700000000000",
            "--no-manual",
            "--limit",
            "50",
        ],
        &[
            "ankic",
            "reviews",
            "list",
            "--deck-name",
            "Default",
            "--include-children",
            "--membership",
            "current",
            "--all",
        ],
        &["ankic", "reviews", "latest-id"],
        &[
            "ankic",
            "reviews",
            "latest-id",
            "--deck-id",
            "1",
            "--membership",
            "home",
        ],
        // statistics (4 methods)
        &["ankic", "statistics", "today"],
        &["ankic", "statistics", "review-days"],
        &["ankic", "statistics", "summary"],
        &["ankic", "statistics", "summary", "--name", "Default"],
        &["ankic", "statistics", "summary", "--id", "1"],
        &["ankic", "statistics", "html"],
        &["ankic", "statistics", "html", "--name", "Default"],
        &["ankic", "statistics", "html", "--id", "1"],
        // profiles (3 methods)
        &["ankic", "profiles", "list"],
        &["ankic", "profiles", "active"],
        &["ankic", "profiles", "switch", "User 1"],
        // access (1 method)
        &["ankic", "access", "request-permission"],
        // advanced (2 methods)
        &[
            "ankic",
            "advanced",
            "cards-update",
            "1700000000010",
            "--interval-days",
            "14",
            "--sm2-ease-permille",
            "2500",
            "--repetitions",
            "3",
            "--lapses",
            "0",
            "--remaining-steps",
            "0",
            "--fsrs-memory",
            "stabilityDays: 12.5\ndifficulty: 4.2",
            "--custom-data",
            "{\"k\":1}",
        ],
        &[
            "ankic",
            "advanced",
            "cards-update",
            "1700000000010",
            "--patch",
            r#"{"intervalDays": 7}"#,
        ],
        &[
            "ankic",
            "advanced",
            "reviews-import",
            "--reviews",
            r#"[{"id":"1700000000000","cardId":"1700000000010","rating":"good","interval":{"value":5,"unit":"days"},"previousInterval":{"value":1,"unit":"days"},"factor":{"encoding":"legacySm2EasePermille","value":2500},"elapsedMs":1500,"kind":"review"}]"#,
        ],
        &[
            "ankic",
            "advanced",
            "reviews-import",
            "--reviews=- id: '1700000000001'\n  cardId: '1700000000010'\n  rating: hard\n  interval:\n    value: 60\n    unit: seconds\n  previousInterval:\n    value: 0\n    unit: seconds\n  factor:\n    encoding: legacySm2EasePermille\n    value: 2500\n  elapsedMs: 2200\n  kind: learning",
        ],
        // gui (15 methods: browser [3], editors [3], reviewer [6], application [3])
        &["ankic", "gui", "browser-open"],
        &[
            "ankic",
            "gui",
            "browser-open",
            "--query",
            "deck:Default",
            "--sort-by",
            "due",
            "--sort-dir",
            "ascending",
        ],
        &["ankic", "gui", "browser-selection"],
        &[
            "ankic",
            "gui",
            "browser-select",
            "--card-id",
            "1700000000010",
        ],
        &[
            "ankic",
            "gui",
            "browser-select",
            "--note-id",
            "1700000000001",
        ],
        &["ankic", "gui", "editor-open-note", "1700000000001"],
        &["ankic", "gui", "editor-open-add"],
        &[
            "ankic",
            "gui",
            "editor-open-add",
            "--deck-name",
            "Default",
            "--note-type-name",
            "Basic",
            "--fields",
            "Front: Question\nBack: Answer",
            "--tag",
            "study",
        ],
        &[
            "ankic",
            "gui",
            "editor-open-add",
            "--note",
            r#"{"deck":{"name":"Default"},"noteType":{"name":"Basic"},"fields":{"Front":"Q","Back":"A"},"tags":["t1"]}"#,
        ],
        &[
            "ankic",
            "gui",
            "editor-update-draft",
            "editor-1",
            "--deck-id",
            "1",
            "--note-type-id",
            "1700000000005",
            "--fields",
            r#"{"Front":"Updated"}"#,
            "--tag",
            "revised",
            "--mode",
            "replace",
        ],
        &[
            "ankic",
            "gui",
            "editor-update-draft",
            "editor-1",
            "--patch",
            "fields:\n  Front: Hello\ntags:\n  - extra",
            "--mode",
            "append",
        ],
        &["ankic", "gui", "reviewer-get"],
        &[
            "ankic",
            "gui",
            "reviewer-start-timer",
            "--state-token",
            "rev-tok-1",
        ],
        &[
            "ankic",
            "gui",
            "reviewer-show-question",
            "--state-token",
            "rev-tok-1",
        ],
        &[
            "ankic",
            "gui",
            "reviewer-show-answer",
            "--state-token",
            "rev-tok-1",
        ],
        &[
            "ankic",
            "gui",
            "reviewer-answer",
            "--state-token",
            "rev-tok-1",
            "--rating",
            "good",
        ],
        &[
            "ankic",
            "gui",
            "reviewer-play-audio",
            "--state-token",
            "rev-tok-1",
        ],
        &["ankic", "gui", "navigate", "deckBrowser"],
        &[
            "ankic",
            "gui",
            "navigate",
            "deckOverview",
            "--deck-name",
            "Default",
        ],
        &[
            "ankic",
            "gui",
            "navigate",
            "review",
            "--deck-id",
            "1",
        ],
        &["ankic", "gui", "import-file"],
        &[
            "ankic",
            "gui",
            "import-file",
            "--path",
            "/tmp/deck.apkg",
        ],
        &["ankic", "gui", "exit"],
    ]
}

#[test]
fn extended_domain_subcommands_conform_to_schema() {
    let mut covered = std::collections::BTreeSet::new();
    for args in extended_domain_cli_cases() {
        for method in assert_cli_args_conform(args) {
            covered.insert(method);
        }
    }

    // 5 media + 2 packages + 9 scheduling + 2 reviews + 4 statistics
    // + 3 profiles + 1 access + 2 advanced + 15 gui = 43 methods
    let expected_extended_methods = [
        "media.list",
        "media.get",
        "media.store",
        "media.delete",
        "media.getDirectory",
        "packages.import",
        "packages.export",
        "scheduling.get",
        "scheduling.getQueue",
        "scheduling.predict",
        "scheduling.getDue",
        "scheduling.answer",
        "scheduling.answerMany",
        "scheduling.repositionNewCards",
        "scheduling.reset",
        "scheduling.setDueDate",
        "reviews.list",
        "reviews.getLatestId",
        "statistics.getToday",
        "statistics.getReviewDays",
        "statistics.getSummary",
        "statistics.getHtml",
        "profiles.list",
        "profiles.getActive",
        "profiles.switch",
        "access.requestPermission",
        "advanced.cards.update",
        "advanced.reviews.import",
        "browser.open",
        "browser.getSelection",
        "browser.select",
        "editors.openNote",
        "editors.openAdd",
        "editors.updateDraft",
        "reviewer.get",
        "reviewer.startTimer",
        "reviewer.showQuestion",
        "reviewer.showAnswer",
        "reviewer.answer",
        "reviewer.playAudio",
        "application.navigate",
        "application.importFile",
        "application.exit",
    ];
    for method in expected_extended_methods {
        assert!(
            covered.contains(method),
            "expected extended method '{method}' to be covered by extended_domain_cli_cases"
        );
    }
}

#[test]
fn all_96_schema_methods_covered_by_cli_subcommands() {
    let x_methods = ROOT_SCHEMA
        .get("x-methods")
        .and_then(Value::as_object)
        .expect("root schema must contain x-methods object");
    assert_eq!(
        x_methods.len(),
        96,
        "expected exactly 96 methods in x-methods"
    );

    let schema_methods: std::collections::BTreeSet<String> =
        x_methods.keys().cloned().collect();

    let mut tested_methods = std::collections::BTreeSet::new();
    for args in core_domain_cli_cases()
        .iter()
        .chain(extended_domain_cli_cases())
    {
        for method in assert_cli_args_conform(args) {
            tested_methods.insert(method);
        }
    }

    let missing: Vec<&String> = schema_methods.difference(&tested_methods).collect();
    assert!(
        missing.is_empty(),
        "CLI subcommands are missing schema contract coverage for methods: {missing:?}"
    );
    assert_eq!(tested_methods.len(), 96);
}

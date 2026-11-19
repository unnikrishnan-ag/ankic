use std::collections::BTreeMap;
use std::sync::LazyLock;

use anki_skill::commands::{to_rpc_plan, BatchMode, Cli, RpcPlan};
use anki_skill::schema::{method_scope, validate_method_params};
use clap::Parser;
use serde_json::{json, Value};

static ROOT_SCHEMA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../schemas/api-v1.schema.json"))
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
/// `anki_skill::schema::validate_method_params` in both session modes:
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

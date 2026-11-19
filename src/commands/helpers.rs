use std::io::{self, Read};

use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::RpcPlan;

/// Build a `RpcPlan::Call` with `wait: false` for synchronous methods.
pub(crate) fn call_plan(method: impl Into<String>, params: Value) -> RpcPlan {
    RpcPlan::Call {
        method: method.into(),
        params,
        wait: false,
        timeout_secs: 0,
    }
}

/// Build a `#/$defs/Reference` object from mutually exclusive `name` or `id` options.
pub fn make_ref(name: Option<&str>, id: Option<&str>, label: &str) -> Result<Value, CliError> {
    match (name, id) {
        (Some(n), None) if !n.trim().is_empty() => Ok(json!({ "name": n })),
        (None, Some(i)) if !i.trim().is_empty() => Ok(json!({ "id": i })),
        _ => Err(CliError::InvalidInput(format!(
            "Specify exactly one non-empty name or ID for {label}"
        ))),
    }
}

/// Build a non-empty list of `#/$defs/Reference` objects from repeatable name and ID flags.
pub fn make_refs(names: &[String], ids: &[String], label: &str) -> Result<Vec<Value>, CliError> {
    let mut refs = Vec::with_capacity(names.len() + ids.len());
    for name in names {
        refs.push(make_ref(Some(name.as_str()), None, label)?);
    }
    for id in ids {
        refs.push(make_ref(None, Some(id.as_str()), label)?);
    }
    if refs.is_empty() {
        return Err(CliError::InvalidInput(format!(
            "Specify at least one {label} name or ID"
        )));
    }
    Ok(refs)
}

/// Build an optional `#/$defs/Sort` object from `--sort-by` and `--sort-dir`.
pub fn make_sort(sort_by: Option<&str>, sort_dir: Option<&str>) -> Result<Option<Value>, CliError> {
    let Some(col) = sort_by else {
        if sort_dir.is_some() {
            return Err(CliError::InvalidInput(
                "--sort-dir requires --sort-by".into(),
            ));
        }
        return Ok(None);
    };

    if col.trim().is_empty() {
        return Err(CliError::InvalidInput("--sort-by must not be empty".into()));
    }

    let direction = match sort_dir.unwrap_or("ascending") {
        "ascending" | "asc" => "ascending",
        "descending" | "desc" => "descending",
        other => {
            return Err(CliError::InvalidInput(format!(
                "Invalid sort direction '{other}': expected 'ascending' or 'descending'"
            )));
        }
    };

    Ok(Some(json!({
        "column": col,
        "direction": direction,
    })))
}

/// Parse a YAML or JSON string into a `serde_json::Value`.
pub fn parse_yaml_or_json_value(raw: &str, flag_name: &str) -> Result<Value, CliError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CliError::InvalidInput(format!(
            "{flag_name} must not be empty"
        )));
    }
    serde_norway::from_str(trimmed)
        .map_err(|e| CliError::InvalidInput(format!("Invalid YAML/JSON for {flag_name}: {e}")))
}

/// Parse a YAML or JSON string and require it to be a JSON object.
pub fn parse_yaml_or_json_object(
    raw: &str,
    flag_name: &str,
) -> Result<Map<String, Value>, CliError> {
    match parse_yaml_or_json_value(raw, flag_name)? {
        Value::Object(map) => Ok(map),
        _ => Err(CliError::InvalidInput(format!(
            "{flag_name} must be a YAML or JSON object"
        ))),
    }
}

/// Parse a YAML or JSON string and require it to be a JSON array.
pub fn parse_yaml_or_json_array(raw: &str, flag_name: &str) -> Result<Vec<Value>, CliError> {
    match parse_yaml_or_json_value(raw, flag_name)? {
        Value::Array(arr) => Ok(arr),
        _ => Err(CliError::InvalidInput(format!(
            "{flag_name} must be a YAML or JSON array"
        ))),
    }
}

/// Read structured input from an inline CLI flag or `stdin`.
pub fn read_input_or_stdin(
    inline: Option<&str>,
    use_stdin: bool,
    flag_name: &str,
) -> Result<String, CliError> {
    match (inline, use_stdin) {
        (Some(_), true) => Err(CliError::InvalidInput(format!(
            "Provide either {flag_name} or --stdin, not both"
        ))),
        (None, true) => {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| CliError::InvalidInput(format!("Failed to read stdin: {e}")))?;
            Ok(buf)
        }
        (Some(raw), false) => Ok(raw.to_string()),
        (None, false) => Err(CliError::InvalidInput(format!(
            "Provide {flag_name} or --stdin"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_ref_accepts_name_or_id_and_rejects_ambiguous_or_empty() {
        assert_eq!(
            make_ref(Some("Default"), None, "deck").unwrap(),
            json!({ "name": "Default" })
        );
        assert_eq!(
            make_ref(None, Some("123"), "deck").unwrap(),
            json!({ "id": "123" })
        );

        let both = make_ref(Some("Default"), Some("123"), "deck").unwrap_err();
        assert_eq!(both.exit_code(), 2);
        assert!(both.to_string().contains("deck"));

        let neither = make_ref(None, None, "preset").unwrap_err();
        assert_eq!(neither.exit_code(), 2);
        assert!(neither.to_string().contains("preset"));

        let blank = make_ref(Some("   "), None, "deck").unwrap_err();
        assert_eq!(blank.exit_code(), 2);
    }

    #[test]
    fn make_refs_combines_names_and_ids_and_rejects_empty() {
        let refs = make_refs(
            &["Default".to_string()],
            &["42".to_string(), "43".to_string()],
            "deck",
        )
        .unwrap();
        assert_eq!(
            refs,
            vec![
                json!({ "name": "Default" }),
                json!({ "id": "42" }),
                json!({ "id": "43" }),
            ]
        );

        let empty = make_refs(&[], &[], "deck").unwrap_err();
        assert_eq!(empty.exit_code(), 2);
        assert!(empty.to_string().contains("deck"));
    }

    #[test]
    fn make_sort_builds_valid_sort_or_rejects_invalid() {
        assert_eq!(make_sort(None, None).unwrap(), None);
        assert_eq!(
            make_sort(Some("noteFld"), None).unwrap(),
            Some(json!({ "column": "noteFld", "direction": "ascending" }))
        );
        assert_eq!(
            make_sort(Some("cardMod"), Some("descending")).unwrap(),
            Some(json!({ "column": "cardMod", "direction": "descending" }))
        );

        assert_eq!(
            make_sort(None, Some("ascending")).unwrap_err().exit_code(),
            2
        );
        assert_eq!(
            make_sort(Some("noteFld"), Some("sideways"))
                .unwrap_err()
                .exit_code(),
            2
        );
    }

    #[test]
    fn yaml_and_json_parsers_handle_objects_arrays_and_errors() {
        let obj = parse_yaml_or_json_object("Front: Hello\nBack: World", "--fields").unwrap();
        assert_eq!(obj.get("Front"), Some(&json!("Hello")));
        assert_eq!(obj.get("Back"), Some(&json!("World")));

        let json_obj = parse_yaml_or_json_object(r#"{"Front":"Hi"}"#, "--fields").unwrap();
        assert_eq!(json_obj.get("Front"), Some(&json!("Hi")));

        let arr = parse_yaml_or_json_array("- a\n- b", "--items").unwrap();
        assert_eq!(arr, vec![json!("a"), json!("b")]);

        assert_eq!(
            parse_yaml_or_json_object("- a\n- b", "--fields")
                .unwrap_err()
                .exit_code(),
            2
        );
        assert_eq!(
            parse_yaml_or_json_array("a: 1", "--items")
                .unwrap_err()
                .exit_code(),
            2
        );
        assert_eq!(
            parse_yaml_or_json_value("   ", "--params")
                .unwrap_err()
                .exit_code(),
            2
        );
    }

    #[test]
    fn read_input_or_stdin_returns_inline_or_errors() {
        assert_eq!(
            read_input_or_stdin(Some("a: 1"), false, "--fields").unwrap(),
            "a: 1"
        );
        assert_eq!(
            read_input_or_stdin(Some("a: 1"), true, "--fields")
                .unwrap_err()
                .exit_code(),
            2
        );
        assert_eq!(
            read_input_or_stdin(None, false, "--fields")
                .unwrap_err()
                .exit_code(),
            2
        );
    }
}

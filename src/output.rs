//! Output formatting utilities (YAML / JSON) for `anki-cli`.

use clap::ValueEnum;
use serde_json::Value;

use crate::error::CliError;

/// Supported CLI output formats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Yaml,
    Json,
    #[value(name = "json-pretty")]
    JsonPretty,
}

impl OutputFormat {
    /// Resolve the effective output format from `--format`, `--json`, and `--pretty` flags.
    pub fn resolve(format: Option<OutputFormat>, json_flag: bool, pretty_flag: bool) -> Self {
        match format {
            Some(fmt) => {
                if pretty_flag && fmt == OutputFormat::Json {
                    OutputFormat::JsonPretty
                } else {
                    fmt
                }
            }
            None => {
                if pretty_flag {
                    OutputFormat::JsonPretty
                } else if json_flag {
                    OutputFormat::Json
                } else {
                    OutputFormat::Yaml
                }
            }
        }
    }
}

/// Format a `serde_json::Value` according to the selected `OutputFormat`.
pub fn format_value(value: &Value, format: OutputFormat) -> Result<String, CliError> {
    match format {
        OutputFormat::Yaml => {
            let rendered = serde_norway::to_string(value)?;
            Ok(rendered.trim_end().to_string())
        }
        OutputFormat::Json => Ok(serde_json::to_string(value)?),
        OutputFormat::JsonPretty => Ok(serde_json::to_string_pretty(value)?),
    }
}

/// Format a `CliError` as YAML or JSON for stderr, falling back to a safe inline
/// representation if serialization fails.
pub fn format_error(err: &CliError, format: OutputFormat) -> String {
    let json_err = err.to_json();
    match format_value(&json_err, format) {
        Ok(rendered) => rendered,
        Err(_) => {
            let escaped = serde_json::to_string(&err.to_string())
                .unwrap_or_else(|_| "\"unknown error\"".to_string());
            match format {
                OutputFormat::Yaml => format!("error:\n  kind: cli\n  message: {escaped}"),
                OutputFormat::Json | OutputFormat::JsonPretty => {
                    format!(r#"{{"error":{{"kind":"cli","message":{escaped}}}}}"#)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_output_format_is_yaml() {
        assert_eq!(OutputFormat::default(), OutputFormat::Yaml);
    }

    #[test]
    fn resolve_precedence_respects_format_json_and_pretty() {
        assert_eq!(OutputFormat::resolve(None, false, false), OutputFormat::Yaml);
        assert_eq!(OutputFormat::resolve(None, true, false), OutputFormat::Json);
        assert_eq!(
            OutputFormat::resolve(None, false, true),
            OutputFormat::JsonPretty
        );
        assert_eq!(
            OutputFormat::resolve(None, true, true),
            OutputFormat::JsonPretty
        );
        assert_eq!(
            OutputFormat::resolve(Some(OutputFormat::Yaml), true, true),
            OutputFormat::Yaml
        );
        assert_eq!(
            OutputFormat::resolve(Some(OutputFormat::Json), false, true),
            OutputFormat::JsonPretty
        );
        assert_eq!(
            OutputFormat::resolve(Some(OutputFormat::Json), false, false),
            OutputFormat::Json
        );
        assert_eq!(
            OutputFormat::resolve(Some(OutputFormat::JsonPretty), false, false),
            OutputFormat::JsonPretty
        );
    }

    #[test]
    fn format_value_renders_yaml_json_and_json_pretty() {
        let value = json!({
            "deck": "Default",
            "count": 2,
        });

        let yaml = format_value(&value, OutputFormat::Yaml).expect("yaml format");
        assert!(!yaml.ends_with('\n'));
        let parsed_yaml: Value = serde_norway::from_str(&yaml).expect("valid yaml");
        assert_eq!(parsed_yaml, value);

        let json_compact = format_value(&value, OutputFormat::Json).expect("json format");
        assert!(!json_compact.contains('\n'));
        let parsed_json: Value = serde_json::from_str(&json_compact).expect("valid json");
        assert_eq!(parsed_json, value);

        let json_pretty =
            format_value(&value, OutputFormat::JsonPretty).expect("json-pretty format");
        assert!(json_pretty.contains('\n'));
        assert!(json_pretty.contains("  \"deck\": \"Default\""));
        let parsed_pretty: Value = serde_json::from_str(&json_pretty).expect("valid pretty json");
        assert_eq!(parsed_pretty, value);
    }

    #[test]
    fn format_error_renders_rpc_and_invalid_input_in_yaml_and_json() {
        let rpc_err = CliError::Rpc {
            code: -32602,
            message: "Invalid params".to_string(),
            data: Some(json!({"field": "deck"})),
        };

        let rpc_yaml = format_error(&rpc_err, OutputFormat::Yaml);
        let parsed_rpc_yaml: Value = serde_norway::from_str(&rpc_yaml).expect("valid yaml error");
        assert_eq!(parsed_rpc_yaml["error"]["kind"], "rpc");
        assert_eq!(parsed_rpc_yaml["error"]["code"], -32602);
        assert_eq!(parsed_rpc_yaml["error"]["message"], "Invalid params");
        assert_eq!(parsed_rpc_yaml["error"]["data"]["field"], "deck");

        let rpc_json = format_error(&rpc_err, OutputFormat::Json);
        let parsed_rpc_json: Value = serde_json::from_str(&rpc_json).expect("valid json error");
        assert_eq!(parsed_rpc_json["error"]["kind"], "rpc");
        assert_eq!(parsed_rpc_json["error"]["code"], -32602);

        let input_err = CliError::InvalidInput("missing field".to_string());

        let input_yaml = format_error(&input_err, OutputFormat::Yaml);
        let parsed_input_yaml: Value =
            serde_norway::from_str(&input_yaml).expect("valid yaml input error");
        assert_eq!(parsed_input_yaml["error"]["kind"], "cli");
        assert_eq!(
            parsed_input_yaml["error"]["message"],
            "Invalid input: missing field"
        );

        let input_json = format_error(&input_err, OutputFormat::Json);
        let parsed_input_json: Value =
            serde_json::from_str(&input_json).expect("valid json input error");
        assert_eq!(parsed_input_json["error"]["kind"], "cli");
        assert_eq!(
            parsed_input_json["error"]["message"],
            "Invalid input: missing field"
        );
    }
}

use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{
    call_plan, parse_yaml_or_json_array, parse_yaml_or_json_object, parse_yaml_or_json_value,
    read_input_or_stdin,
};
use super::RpcPlan;

#[derive(Subcommand)]
pub enum AccessCmd {
    /// Request permission from the user in Anki to grant API access
    #[command(name = "request-permission")]
    RequestPermission,
}

#[derive(Subcommand)]
pub enum AdvancedCmd {
    /// Update low-level scheduling fields of a card (interval, ease, FSRS memory, custom data)
    #[command(name = "cards-update")]
    CardsUpdate {
        /// Card ID
        card_id: String,
        /// YAML or JSON RawCardPatch object
        #[arg(long)]
        patch: Option<String>,
        /// Card interval in days
        #[arg(long)]
        interval_days: Option<u64>,
        /// SM-2 ease factor in permille (e.g. 2500 for 250%)
        #[arg(long)]
        sm2_ease_permille: Option<u64>,
        /// Total number of repetitions
        #[arg(long)]
        repetitions: Option<u64>,
        /// Total number of lapses
        #[arg(long)]
        lapses: Option<u64>,
        /// Remaining learning steps
        #[arg(long)]
        remaining_steps: Option<u64>,
        /// FSRS memory state as YAML/JSON object (or null)
        #[arg(long)]
        fsrs_memory: Option<String>,
        /// Custom data string stored on the card
        #[arg(long)]
        custom_data: Option<String>,
    },
    /// Import historical review log entries (YAML/JSON array via --reviews or --stdin)
    #[command(name = "reviews-import")]
    ReviewsImport {
        /// YAML or JSON array of ReviewImport objects
        #[arg(long)]
        reviews: Option<String>,
        /// Read YAML or JSON array of ReviewImport objects from stdin
        #[arg(long)]
        stdin: bool,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
}

/// Translate an `access` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan_access(cmd: &AccessCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        AccessCmd::RequestPermission => Ok(call_plan("access.requestPermission", json!({}))),
    }
}

/// Translate an `advanced` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan_advanced(cmd: &AdvancedCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        AdvancedCmd::CardsUpdate {
            card_id,
            patch,
            interval_days,
            sm2_ease_permille,
            repetitions,
            lapses,
            remaining_steps,
            fsrs_memory,
            custom_data,
        } => {
            if card_id.trim().is_empty() {
                return Err(CliError::InvalidInput(
                    "Card ID must not be empty".into(),
                ));
            }

            let mut patch_map = match patch {
                Some(raw) => parse_yaml_or_json_object(raw, "--patch")?,
                None => Map::new(),
            };

            if let Some(value) = interval_days {
                patch_map.insert("intervalDays".into(), json!(value));
            }
            if let Some(value) = sm2_ease_permille {
                patch_map.insert("sm2EasePermille".into(), json!(value));
            }
            if let Some(value) = repetitions {
                patch_map.insert("repetitions".into(), json!(value));
            }
            if let Some(value) = lapses {
                patch_map.insert("lapses".into(), json!(value));
            }
            if let Some(value) = remaining_steps {
                patch_map.insert("remainingSteps".into(), json!(value));
            }
            if let Some(raw) = fsrs_memory {
                patch_map.insert(
                    "fsrsMemory".into(),
                    parse_yaml_or_json_value(raw, "--fsrs-memory")?,
                );
            }
            if let Some(value) = custom_data {
                patch_map.insert("customData".into(), json!(value));
            }

            if patch_map.is_empty() {
                return Err(CliError::InvalidInput(
                    "Specify --patch or at least one card patch field".into(),
                ));
            }

            Ok(call_plan(
                "advanced.cards.update",
                json!({
                    "cardId": card_id,
                    "patch": Value::Object(patch_map),
                }),
            ))
        }
        AdvancedCmd::ReviewsImport {
            reviews,
            stdin,
            no_wait,
            timeout,
        } => {
            let raw = read_input_or_stdin(reviews.as_deref(), *stdin, "--reviews")?;
            let reviews_array = parse_yaml_or_json_array(&raw, "--reviews")?;
            Ok(RpcPlan::Call {
                method: "advanced.reviews.import".into(),
                params: json!({ "reviews": reviews_array }),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_valid_call(
        plan: RpcPlan,
        expected_method: &str,
        expected_wait: bool,
        expected_timeout: u64,
    ) -> Value {
        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                timeout_secs,
            } => {
                assert_eq!(method, expected_method);
                assert_eq!(wait, expected_wait);
                assert_eq!(timeout_secs, expected_timeout);
                schema::validate_method_params(&method, &params)
                    .expect("params should satisfy api-v1 schema");
                params
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn access_request_permission_builds_empty_params() {
        let plan = to_rpc_plan_access(&AccessCmd::RequestPermission).unwrap();
        let params = assert_valid_call(plan, "access.requestPermission", false, 0);
        assert_eq!(params, json!({}));
    }

    #[test]
    fn advanced_cards_update_supports_patch_yaml_and_json() {
        let yaml_plan = to_rpc_plan_advanced(&AdvancedCmd::CardsUpdate {
            card_id: "1001".into(),
            patch: Some("intervalDays: 14\nsm2EasePermille: 2500\ncustomData: reviewed".into()),
            interval_days: None,
            sm2_ease_permille: None,
            repetitions: None,
            lapses: None,
            remaining_steps: None,
            fsrs_memory: None,
            custom_data: None,
        })
        .unwrap();
        let yaml_params = assert_valid_call(yaml_plan, "advanced.cards.update", false, 0);
        assert_eq!(
            yaml_params,
            json!({
                "cardId": "1001",
                "patch": {
                    "intervalDays": 14,
                    "sm2EasePermille": 2500,
                    "customData": "reviewed"
                }
            })
        );

        let json_plan = to_rpc_plan_advanced(&AdvancedCmd::CardsUpdate {
            card_id: "1002".into(),
            patch: Some(r#"{"repetitions":3,"fsrsMemory":{"difficulty":5.2,"stabilityDays":12.5}}"#.into()),
            interval_days: Some(21),
            sm2_ease_permille: None,
            repetitions: None,
            lapses: None,
            remaining_steps: None,
            fsrs_memory: None,
            custom_data: None,
        })
        .unwrap();
        let json_params = assert_valid_call(json_plan, "advanced.cards.update", false, 0);
        assert_eq!(
            json_params,
            json!({
                "cardId": "1002",
                "patch": {
                    "repetitions": 3,
                    "fsrsMemory": {
                        "difficulty": 5.2,
                        "stabilityDays": 12.5
                    },
                    "intervalDays": 21
                }
            })
        );
    }

    #[test]
    fn advanced_cards_update_supports_individual_flags_and_null_fsrs_memory() {
        let plan = to_rpc_plan_advanced(&AdvancedCmd::CardsUpdate {
            card_id: "1003".into(),
            patch: None,
            interval_days: Some(7),
            sm2_ease_permille: Some(2300),
            repetitions: Some(5),
            lapses: Some(1),
            remaining_steps: Some(0),
            fsrs_memory: Some("null".into()),
            custom_data: Some("sync-tag".into()),
        })
        .unwrap();
        let params = assert_valid_call(plan, "advanced.cards.update", false, 0);
        assert_eq!(
            params,
            json!({
                "cardId": "1003",
                "patch": {
                    "intervalDays": 7,
                    "sm2EasePermille": 2300,
                    "repetitions": 5,
                    "lapses": 1,
                    "remainingSteps": 0,
                    "fsrsMemory": null,
                    "customData": "sync-tag"
                }
            })
        );
    }

    #[test]
    fn advanced_cards_update_rejects_empty_patch_and_empty_card_id() {
        let empty_patch = to_rpc_plan_advanced(&AdvancedCmd::CardsUpdate {
            card_id: "1001".into(),
            patch: None,
            interval_days: None,
            sm2_ease_permille: None,
            repetitions: None,
            lapses: None,
            remaining_steps: None,
            fsrs_memory: None,
            custom_data: None,
        })
        .expect_err("empty patch should fail");
        assert!(matches!(empty_patch, CliError::InvalidInput(_)));

        let empty_json_patch = to_rpc_plan_advanced(&AdvancedCmd::CardsUpdate {
            card_id: "1001".into(),
            patch: Some("{}".into()),
            interval_days: None,
            sm2_ease_permille: None,
            repetitions: None,
            lapses: None,
            remaining_steps: None,
            fsrs_memory: None,
            custom_data: None,
        })
        .expect_err("empty JSON patch should fail");
        assert!(matches!(empty_json_patch, CliError::InvalidInput(_)));

        let empty_card_id = to_rpc_plan_advanced(&AdvancedCmd::CardsUpdate {
            card_id: "   ".into(),
            patch: None,
            interval_days: Some(1),
            sm2_ease_permille: None,
            repetitions: None,
            lapses: None,
            remaining_steps: None,
            fsrs_memory: None,
            custom_data: None,
        })
        .expect_err("empty card ID should fail");
        assert!(matches!(empty_card_id, CliError::InvalidInput(_)));
    }

    #[test]
    fn advanced_reviews_import_parses_yaml_and_json_with_wait_flags() {
        let yaml_reviews = r#"
- id: "1700000000000"
  cardId: "1001"
  rating: good
  interval:
    value: 6
    unit: days
  previousInterval:
    value: 1
    unit: days
  factor:
    encoding: legacySm2EasePermille
    value: 2500
  elapsedMs: 4200
  kind: review
"#;
        let plan = to_rpc_plan_advanced(&AdvancedCmd::ReviewsImport {
            reviews: Some(yaml_reviews.into()),
            stdin: false,
            no_wait: false,
            timeout: 300,
        })
        .unwrap();
        let params = assert_valid_call(plan, "advanced.reviews.import", true, 300);
        assert_eq!(params["reviews"].as_array().unwrap().len(), 1);

        let json_reviews = r#"[{"id":"1700000000001","cardId":"1002","rating":null,"interval":{"value":300,"unit":"seconds"},"previousInterval":{"value":0,"unit":"seconds"},"factor":{"encoding":"unknown","value":0},"elapsedMs":0,"kind":"manual"}]"#;
        let no_wait_plan = to_rpc_plan_advanced(&AdvancedCmd::ReviewsImport {
            reviews: Some(json_reviews.into()),
            stdin: false,
            no_wait: true,
            timeout: 60,
        })
        .unwrap();
        let no_wait_params =
            assert_valid_call(no_wait_plan, "advanced.reviews.import", false, 60);
        assert_eq!(no_wait_params["reviews"].as_array().unwrap().len(), 1);
    }
}

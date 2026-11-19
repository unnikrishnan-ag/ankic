use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::call_plan;
use super::RpcPlan;

#[derive(Subcommand)]
pub enum SchedulingCmd {
    /// Get scheduling state for a card
    Get {
        /// Card ID
        card_id: String,
    },
    /// Get the current review queue
    Queue {
        /// Maximum number of due cards to fetch (1..=1000)
        #[arg(long)]
        fetch_limit: Option<u64>,
    },
    /// Predict interval outcomes for each answer rating on a card
    Predict {
        /// Card ID
        card_id: String,
    },
    /// Get due timing information for a card
    Due {
        /// Card ID
        card_id: String,
        /// Include learn-ahead cards in due calculation
        #[arg(long)]
        include_learn_ahead: bool,
    },
}

/// Translate a `scheduling` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &SchedulingCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        SchedulingCmd::Get { card_id } => {
            Ok(call_plan("scheduling.get", json!({ "cardId": card_id })))
        }
        SchedulingCmd::Queue { fetch_limit } => {
            let mut params = Map::new();
            if let Some(limit) = fetch_limit {
                if *limit == 0 || *limit > 1000 {
                    return Err(CliError::InvalidInput(
                        "--fetch-limit must be between 1 and 1000".into(),
                    ));
                }
                params.insert("fetchLimit".into(), json!(limit));
            }
            Ok(call_plan("scheduling.getQueue", Value::Object(params)))
        }
        SchedulingCmd::Predict { card_id } => {
            Ok(call_plan("scheduling.predict", json!({ "cardId": card_id })))
        }
        SchedulingCmd::Due {
            card_id,
            include_learn_ahead,
        } => Ok(call_plan(
            "scheduling.getDue",
            json!({
                "cardId": card_id,
                "includeLearnAhead": include_learn_ahead,
            }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_valid_call(cmd: SchedulingCmd, expected_method: &str) -> Value {
        let plan = to_rpc_plan(&cmd).expect("to_rpc_plan should succeed");
        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                timeout_secs,
            } => {
                assert_eq!(method, expected_method);
                assert!(!wait);
                assert_eq!(timeout_secs, 0);
                schema::validate_method_params(&method, &params)
                    .expect("params should satisfy api-v1 schema");
                params
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn get_card_scheduling_state() {
        let params = assert_valid_call(
            SchedulingCmd::Get {
                card_id: "1234567890".into(),
            },
            "scheduling.get",
        );
        assert_eq!(params, json!({ "cardId": "1234567890" }));
    }

    #[test]
    fn queue_without_limit_omits_fetch_limit() {
        let params = assert_valid_call(
            SchedulingCmd::Queue { fetch_limit: None },
            "scheduling.getQueue",
        );
        assert_eq!(params, json!({}));
    }

    #[test]
    fn queue_with_valid_limit() {
        let params = assert_valid_call(
            SchedulingCmd::Queue {
                fetch_limit: Some(50),
            },
            "scheduling.getQueue",
        );
        assert_eq!(params, json!({ "fetchLimit": 50 }));
    }

    #[test]
    fn queue_rejects_out_of_range_fetch_limit() {
        let err_zero = to_rpc_plan(&SchedulingCmd::Queue {
            fetch_limit: Some(0),
        })
        .expect_err("0 should be rejected");
        assert!(matches!(err_zero, CliError::InvalidInput(_)));

        let err_high = to_rpc_plan(&SchedulingCmd::Queue {
            fetch_limit: Some(1001),
        })
        .expect_err("1001 should be rejected");
        assert!(matches!(err_high, CliError::InvalidInput(_)));
    }

    #[test]
    fn predict_card_intervals() {
        let params = assert_valid_call(
            SchedulingCmd::Predict {
                card_id: "987654321".into(),
            },
            "scheduling.predict",
        );
        assert_eq!(params, json!({ "cardId": "987654321" }));
    }

    #[test]
    fn due_with_and_without_learn_ahead() {
        let params_default = assert_valid_call(
            SchedulingCmd::Due {
                card_id: "42".into(),
                include_learn_ahead: false,
            },
            "scheduling.getDue",
        );
        assert_eq!(
            params_default,
            json!({ "cardId": "42", "includeLearnAhead": false })
        );

        let params_learn_ahead = assert_valid_call(
            SchedulingCmd::Due {
                card_id: "42".into(),
                include_learn_ahead: true,
            },
            "scheduling.getDue",
        );
        assert_eq!(
            params_learn_ahead,
            json!({ "cardId": "42", "includeLearnAhead": true })
        );
    }
}

use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{call_plan, parse_yaml_or_json_array, read_input_or_stdin};
use super::RpcPlan;

const VALID_RATINGS: [&str; 4] = ["again", "hard", "good", "easy"];
const MAX_I32_U64: u64 = 2_147_483_647;

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
    /// Answer a single card with a rating and state token
    Answer {
        /// Card ID
        card_id: String,
        /// Rating (again, hard, good, easy)
        #[arg(long)]
        rating: String,
        /// State token from scheduling.predict or scheduling.get
        #[arg(long)]
        state_token: String,
        /// Time spent on card in milliseconds
        #[arg(long)]
        elapsed_ms: Option<u64>,
    },
    /// Answer multiple cards in a single batch (YAML/JSON array via --answers or --stdin)
    #[command(name = "answer-many")]
    AnswerMany {
        /// YAML or JSON array of Answer objects
        #[arg(long)]
        answers: Option<String>,
        /// Read YAML or JSON array of Answer objects from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// Reposition new cards in the new-card queue
    Reposition {
        /// New position starting value (0..=2147483647)
        #[arg(long)]
        starting_from: u64,
        /// Step size between card positions (1..=2147483647)
        #[arg(long, default_value_t = 1)]
        step_size: u64,
        /// Randomize card order before assigning positions
        #[arg(long)]
        randomize: bool,
        /// Shift existing new cards to make room
        #[arg(long)]
        shift_existing: bool,
        /// Card IDs to reposition
        #[arg(required = true)]
        card_ids: Vec<String>,
    },
    /// Reset cards to new state
    Reset {
        /// Do not restore original new-card position
        #[arg(long)]
        no_restore_position: bool,
        /// Reset review and lapse counts to zero
        #[arg(long)]
        reset_counts: bool,
        /// Do not record a reset entry in the review log
        #[arg(long)]
        no_log: bool,
        /// Card IDs to reset
        #[arg(required = true)]
        card_ids: Vec<String>,
    },
    /// Set due date for cards (e.g. "0", "3", "3-7!")
    #[command(name = "set-due-date")]
    SetDueDate {
        /// Due date specification (e.g. "0", "3", "3-7!")
        #[arg(long)]
        days: String,
        /// Card IDs to update
        #[arg(required = true)]
        card_ids: Vec<String>,
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
        SchedulingCmd::Answer {
            card_id,
            rating,
            state_token,
            elapsed_ms,
        } => {
            if !VALID_RATINGS.contains(&rating.as_str()) {
                return Err(CliError::InvalidInput(format!(
                    "Invalid rating '{rating}': expected one of again, hard, good, easy"
                )));
            }
            if state_token.trim().is_empty() {
                return Err(CliError::InvalidInput(
                    "--state-token must not be empty".into(),
                ));
            }
            let mut params = Map::new();
            params.insert("cardId".into(), json!(card_id));
            params.insert("rating".into(), json!(rating));
            params.insert("stateToken".into(), json!(state_token));
            if let Some(ms) = elapsed_ms {
                params.insert("elapsedMs".into(), json!(ms));
            }
            Ok(call_plan("scheduling.answer", Value::Object(params)))
        }
        SchedulingCmd::AnswerMany { answers, stdin } => {
            let raw = read_input_or_stdin(answers.as_deref(), *stdin, "--answers")?;
            let answers_array = parse_yaml_or_json_array(&raw, "--answers")?;
            Ok(call_plan(
                "scheduling.answerMany",
                json!({ "answers": answers_array }),
            ))
        }
        SchedulingCmd::Reposition {
            card_ids,
            starting_from,
            step_size,
            randomize,
            shift_existing,
        } => {
            if card_ids.is_empty() {
                return Err(CliError::InvalidInput(
                    "At least one card ID is required".into(),
                ));
            }
            if *starting_from > MAX_I32_U64 {
                return Err(CliError::InvalidInput(
                    "--starting-from must be <= 2147483647".into(),
                ));
            }
            if *step_size == 0 || *step_size > MAX_I32_U64 {
                return Err(CliError::InvalidInput(
                    "--step-size must be between 1 and 2147483647".into(),
                ));
            }
            Ok(call_plan(
                "scheduling.repositionNewCards",
                json!({
                    "cardIds": card_ids,
                    "startingFrom": starting_from,
                    "stepSize": step_size,
                    "randomize": randomize,
                    "shiftExisting": shift_existing,
                }),
            ))
        }
        SchedulingCmd::Reset {
            card_ids,
            no_restore_position,
            reset_counts,
            no_log,
        } => Ok(call_plan(
            "scheduling.reset",
            json!({
                "cardIds": card_ids,
                "restorePosition": !no_restore_position,
                "resetCounts": reset_counts,
                "log": !no_log,
            }),
        )),
        SchedulingCmd::SetDueDate { card_ids, days } => {
            if days.trim().is_empty() {
                return Err(CliError::InvalidInput("--days must not be empty".into()));
            }
            Ok(call_plan(
                "scheduling.setDueDate",
                json!({
                    "cardIds": card_ids,
                    "days": days,
                }),
            ))
        }
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

    #[test]
    fn answer_without_and_with_elapsed_ms() {
        let params = assert_valid_call(
            SchedulingCmd::Answer {
                card_id: "100".into(),
                rating: "good".into(),
                state_token: "tok-1".into(),
                elapsed_ms: None,
            },
            "scheduling.answer",
        );
        assert_eq!(
            params,
            json!({
                "cardId": "100",
                "rating": "good",
                "stateToken": "tok-1",
            })
        );

        let params_with_ms = assert_valid_call(
            SchedulingCmd::Answer {
                card_id: "100".into(),
                rating: "easy".into(),
                state_token: "tok-2".into(),
                elapsed_ms: Some(3250),
            },
            "scheduling.answer",
        );
        assert_eq!(
            params_with_ms,
            json!({
                "cardId": "100",
                "rating": "easy",
                "stateToken": "tok-2",
                "elapsedMs": 3250,
            })
        );
    }

    #[test]
    fn answer_rejects_invalid_rating_and_empty_state_token() {
        let err_rating = to_rpc_plan(&SchedulingCmd::Answer {
            card_id: "100".into(),
            rating: "great".into(),
            state_token: "tok-1".into(),
            elapsed_ms: None,
        })
        .expect_err("invalid rating should be rejected");
        assert!(matches!(err_rating, CliError::InvalidInput(_)));

        let err_token = to_rpc_plan(&SchedulingCmd::Answer {
            card_id: "100".into(),
            rating: "again".into(),
            state_token: "   ".into(),
            elapsed_ms: None,
        })
        .expect_err("empty state token should be rejected");
        assert!(matches!(err_token, CliError::InvalidInput(_)));
    }

    #[test]
    fn answer_many_accepts_json_and_yaml() {
        let json_params = assert_valid_call(
            SchedulingCmd::AnswerMany {
                answers: Some(
                    r#"[{"cardId":"1","rating":"good","stateToken":"t1","elapsedMs":1200}]"#
                        .into(),
                ),
                stdin: false,
            },
            "scheduling.answerMany",
        );
        assert_eq!(
            json_params,
            json!({
                "answers": [{
                    "cardId": "1",
                    "rating": "good",
                    "stateToken": "t1",
                    "elapsedMs": 1200,
                }]
            })
        );

        let yaml_params = assert_valid_call(
            SchedulingCmd::AnswerMany {
                answers: Some(
                    "- cardId: \"2\"\n  rating: hard\n  stateToken: t2\n".into(),
                ),
                stdin: false,
            },
            "scheduling.answerMany",
        );
        assert_eq!(
            yaml_params,
            json!({
                "answers": [{
                    "cardId": "2",
                    "rating": "hard",
                    "stateToken": "t2",
                }]
            })
        );
    }

    #[test]
    fn reposition_valid_and_invalid_bounds() {
        let params = assert_valid_call(
            SchedulingCmd::Reposition {
                card_ids: vec!["10".into(), "11".into()],
                starting_from: 5,
                step_size: 2,
                randomize: true,
                shift_existing: false,
            },
            "scheduling.repositionNewCards",
        );
        assert_eq!(
            params,
            json!({
                "cardIds": ["10", "11"],
                "startingFrom": 5,
                "stepSize": 2,
                "randomize": true,
                "shiftExisting": false,
            })
        );

        let err_empty = to_rpc_plan(&SchedulingCmd::Reposition {
            card_ids: vec![],
            starting_from: 0,
            step_size: 1,
            randomize: false,
            shift_existing: false,
        })
        .expect_err("empty card_ids should be rejected");
        assert!(matches!(err_empty, CliError::InvalidInput(_)));

        let err_start = to_rpc_plan(&SchedulingCmd::Reposition {
            card_ids: vec!["10".into()],
            starting_from: MAX_I32_U64 + 1,
            step_size: 1,
            randomize: false,
            shift_existing: false,
        })
        .expect_err("starting_from > i32::MAX should be rejected");
        assert!(matches!(err_start, CliError::InvalidInput(_)));

        let err_step_zero = to_rpc_plan(&SchedulingCmd::Reposition {
            card_ids: vec!["10".into()],
            starting_from: 0,
            step_size: 0,
            randomize: false,
            shift_existing: false,
        })
        .expect_err("step_size == 0 should be rejected");
        assert!(matches!(err_step_zero, CliError::InvalidInput(_)));
    }

    #[test]
    fn reset_defaults_and_overrides() {
        let params_default = assert_valid_call(
            SchedulingCmd::Reset {
                card_ids: vec!["20".into(), "21".into()],
                no_restore_position: false,
                reset_counts: false,
                no_log: false,
            },
            "scheduling.reset",
        );
        assert_eq!(
            params_default,
            json!({
                "cardIds": ["20", "21"],
                "restorePosition": true,
                "resetCounts": false,
                "log": true,
            })
        );

        let params_override = assert_valid_call(
            SchedulingCmd::Reset {
                card_ids: vec!["20".into()],
                no_restore_position: true,
                reset_counts: true,
                no_log: true,
            },
            "scheduling.reset",
        );
        assert_eq!(
            params_override,
            json!({
                "cardIds": ["20"],
                "restorePosition": false,
                "resetCounts": true,
                "log": false,
            })
        );
    }

    #[test]
    fn set_due_date_valid_and_empty_days() {
        let params = assert_valid_call(
            SchedulingCmd::SetDueDate {
                card_ids: vec!["30".into()],
                days: "3-7!".into(),
            },
            "scheduling.setDueDate",
        );
        assert_eq!(
            params,
            json!({
                "cardIds": ["30"],
                "days": "3-7!",
            })
        );

        let err_empty = to_rpc_plan(&SchedulingCmd::SetDueDate {
            card_ids: vec!["30".into()],
            days: "   ".into(),
        })
        .expect_err("empty days should be rejected");
        assert!(matches!(err_empty, CliError::InvalidInput(_)));
    }
}

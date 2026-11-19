use clap::Subcommand;
use serde_json::{json, Value};

use crate::error::CliError;

use super::helpers::{call_plan, make_ref};
use super::RpcPlan;

#[derive(Subcommand)]
pub enum StatisticsCmd {
    /// Get today's review count and study time
    Today,
    /// Get per-day review statistics across collection history
    ReviewDays,
    /// Get summary statistics for the collection or a specific deck
    Summary {
        /// Optional deck name
        #[arg(long, visible_aliases = ["deck-name", "deck"], conflicts_with = "id")]
        name: Option<String>,
        /// Optional deck ID
        #[arg(long, visible_alias = "deck-id", conflicts_with = "name")]
        id: Option<String>,
    },
    /// Get legacy HTML statistics report for the collection or a specific deck
    Html {
        /// Optional deck name
        #[arg(long, visible_aliases = ["deck-name", "deck"], conflicts_with = "id")]
        name: Option<String>,
        /// Optional deck ID
        #[arg(long, visible_alias = "deck-id", conflicts_with = "name")]
        id: Option<String>,
    },
}

fn optional_deck_params(name: Option<&str>, id: Option<&str>) -> Result<Value, CliError> {
    if name.is_none() && id.is_none() {
        return Ok(json!({}));
    }
    let deck = make_ref(name, id, "deck")?;
    Ok(json!({ "deck": deck }))
}

/// Translate a `statistics` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &StatisticsCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        StatisticsCmd::Today => Ok(call_plan("statistics.getToday", json!({}))),
        StatisticsCmd::ReviewDays => Ok(call_plan("statistics.getReviewDays", json!({}))),
        StatisticsCmd::Summary { name, id } => {
            let params = optional_deck_params(name.as_deref(), id.as_deref())?;
            Ok(call_plan("statistics.getSummary", params))
        }
        StatisticsCmd::Html { name, id } => {
            let params = optional_deck_params(name.as_deref(), id.as_deref())?;
            Ok(call_plan("statistics.getHtml", params))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_valid_call(cmd: StatisticsCmd, expected_method: &str) -> Value {
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
    fn today_and_review_days_build_empty_params() {
        let today = assert_valid_call(StatisticsCmd::Today, "statistics.getToday");
        assert_eq!(today, json!({}));

        let days = assert_valid_call(StatisticsCmd::ReviewDays, "statistics.getReviewDays");
        assert_eq!(days, json!({}));
    }

    #[test]
    fn summary_supports_unfiltered_and_deck_ref() {
        let unfiltered = assert_valid_call(
            StatisticsCmd::Summary {
                name: None,
                id: None,
            },
            "statistics.getSummary",
        );
        assert_eq!(unfiltered, json!({}));

        let by_name = assert_valid_call(
            StatisticsCmd::Summary {
                name: Some("Default".into()),
                id: None,
            },
            "statistics.getSummary",
        );
        assert_eq!(by_name, json!({ "deck": { "name": "Default" } }));

        let by_id = assert_valid_call(
            StatisticsCmd::Summary {
                name: None,
                id: Some("1".into()),
            },
            "statistics.getSummary",
        );
        assert_eq!(by_id, json!({ "deck": { "id": "1" } }));
    }

    #[test]
    fn html_supports_unfiltered_and_deck_ref() {
        let unfiltered = assert_valid_call(
            StatisticsCmd::Html {
                name: None,
                id: None,
            },
            "statistics.getHtml",
        );
        assert_eq!(unfiltered, json!({}));

        let by_name = assert_valid_call(
            StatisticsCmd::Html {
                name: Some("Japanese::Vocab".into()),
                id: None,
            },
            "statistics.getHtml",
        );
        assert_eq!(by_name, json!({ "deck": { "name": "Japanese::Vocab" } }));
    }

    #[test]
    fn empty_deck_name_is_rejected() {
        let err = to_rpc_plan(&StatisticsCmd::Summary {
            name: Some("   ".into()),
            id: None,
        })
        .expect_err("empty deck name should fail");
        assert!(matches!(err, CliError::InvalidInput(_)));
    }
}

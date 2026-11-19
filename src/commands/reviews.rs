use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{call_plan, make_ref};
use super::RpcPlan;

const VALID_MEMBERSHIPS: [&str; 2] = ["current", "home"];

#[derive(Subcommand)]
pub enum ReviewsCmd {
    /// List review log entries with optional card/deck filters and cursor pagination
    List {
        /// Filter by one or more card IDs (mutually exclusive with --deck-name/--deck-id)
        #[arg(long = "card-id")]
        card_ids: Vec<String>,
        /// Filter by deck name (mutually exclusive with --card-id and --deck-id)
        #[arg(long, visible_alias = "deck", conflicts_with = "deck_id")]
        deck_name: Option<String>,
        /// Filter by deck ID (mutually exclusive with --card-id and --deck-name)
        #[arg(long, conflicts_with = "deck_name")]
        deck_id: Option<String>,
        /// Include child decks when filtering by deck
        #[arg(long)]
        include_children: bool,
        /// Deck membership mode when filtering by deck (current or home)
        #[arg(long)]
        membership: Option<String>,
        /// Return only reviews with ID strictly greater than this review ID
        #[arg(long)]
        after_id: Option<String>,
        /// Exclude manual scheduling/reschedule entries (default includes them)
        #[arg(long)]
        no_manual: bool,
        /// Maximum number of review entries per page (1..=1000)
        #[arg(long, default_value_t = 100)]
        limit: u64,
        /// Opaque pagination cursor from a previous reviews.list response
        #[arg(long)]
        cursor: Option<String>,
        /// Auto-paginate through all pages and merge `items`
        #[arg(long)]
        all: bool,
    },
    /// Get the latest review log ID matching optional card or deck filters
    #[command(name = "latest-id")]
    LatestId {
        /// Filter by one or more card IDs (mutually exclusive with --deck-name/--deck-id)
        #[arg(long = "card-id")]
        card_ids: Vec<String>,
        /// Filter by deck name (mutually exclusive with --card-id and --deck-id)
        #[arg(long, visible_alias = "deck", conflicts_with = "deck_id")]
        deck_name: Option<String>,
        /// Filter by deck ID (mutually exclusive with --card-id and --deck-name)
        #[arg(long, conflicts_with = "deck_name")]
        deck_id: Option<String>,
        /// Deck membership mode when filtering by deck (current or home)
        #[arg(long)]
        membership: Option<String>,
    },
}

fn build_filter_params(
    method: &str,
    card_ids: &[String],
    deck_name: Option<&str>,
    deck_id: Option<&str>,
    membership: Option<&str>,
) -> Result<Map<String, Value>, CliError> {
    if !card_ids.is_empty() && (deck_name.is_some() || deck_id.is_some()) {
        return Err(CliError::InvalidInput(format!(
            "{method} cannot filter by both --card-id and --deck-name/--deck-id"
        )));
    }

    let mut params = Map::new();
    if !card_ids.is_empty() {
        params.insert("cardIds".into(), json!(card_ids));
    }
    if deck_name.is_some() || deck_id.is_some() {
        let deck_ref = make_ref(deck_name, deck_id, "deck")?;
        params.insert("deck".into(), deck_ref);
    }
    if let Some(m) = membership {
        if !VALID_MEMBERSHIPS.contains(&m) {
            return Err(CliError::InvalidInput(format!(
                "Invalid membership '{m}': expected one of {}",
                VALID_MEMBERSHIPS.join(", ")
            )));
        }
        params.insert("membership".into(), json!(m));
    }

    Ok(params)
}

/// Translate a `reviews` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &ReviewsCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        ReviewsCmd::List {
            card_ids,
            deck_name,
            deck_id,
            include_children,
            membership,
            after_id,
            no_manual,
            limit,
            cursor,
            all,
        } => {
            if !(1..=1000).contains(limit) {
                return Err(CliError::InvalidInput(format!(
                    "Invalid limit {limit}: must be between 1 and 1000"
                )));
            }

            let mut params = build_filter_params(
                "reviews.list",
                card_ids,
                deck_name.as_deref(),
                deck_id.as_deref(),
                membership.as_deref(),
            )?;

            params.insert("limit".into(), json!(limit));
            params.insert("includeChildren".into(), json!(include_children));
            params.insert("includeManual".into(), json!(!no_manual));

            if let Some(id) = after_id {
                if id.trim().is_empty() {
                    return Err(CliError::InvalidInput(
                        "--after-id must not be empty".into(),
                    ));
                }
                params.insert("afterId".into(), json!(id));
            }
            if let Some(c) = cursor {
                if c.trim().is_empty() {
                    return Err(CliError::InvalidInput("--cursor must not be empty".into()));
                }
                params.insert("cursor".into(), json!(c));
            }

            if *all {
                Ok(RpcPlan::Paginate {
                    method: "reviews.list".into(),
                    params,
                    array_key: "items",
                })
            } else {
                Ok(call_plan("reviews.list", Value::Object(params)))
            }
        }
        ReviewsCmd::LatestId {
            card_ids,
            deck_name,
            deck_id,
            membership,
        } => {
            let params = build_filter_params(
                "reviews.getLatestId",
                card_ids,
                deck_name.as_deref(),
                deck_id.as_deref(),
                membership.as_deref(),
            )?;
            Ok(call_plan("reviews.getLatestId", Value::Object(params)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    #[test]
    fn list_unfiltered_validates_against_schema() {
        let plan = to_rpc_plan(&ReviewsCmd::List {
            card_ids: vec![],
            deck_name: None,
            deck_id: None,
            include_children: false,
            membership: None,
            after_id: None,
            no_manual: false,
            limit: 100,
            cursor: None,
            all: false,
        })
        .unwrap();

        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                ..
            } => {
                assert_eq!(method, "reviews.list");
                assert!(!wait);
                assert_eq!(
                    params,
                    json!({
                        "limit": 100,
                        "includeChildren": false,
                        "includeManual": true
                    })
                );
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("Expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn list_by_card_ids_and_optional_flags_validates_against_schema() {
        let plan = to_rpc_plan(&ReviewsCmd::List {
            card_ids: vec!["101".into(), "102".into()],
            deck_name: None,
            deck_id: None,
            include_children: false,
            membership: Some("home".into()),
            after_id: Some("1700000000000".into()),
            no_manual: true,
            limit: 25,
            cursor: Some("cur-1".into()),
            all: false,
        })
        .unwrap();

        match plan {
            RpcPlan::Call { method, params, .. } => {
                assert_eq!(method, "reviews.list");
                assert_eq!(
                    params,
                    json!({
                        "cardIds": ["101", "102"],
                        "membership": "home",
                        "limit": 25,
                        "includeChildren": false,
                        "includeManual": false,
                        "afterId": "1700000000000",
                        "cursor": "cur-1"
                    })
                );
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("Expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn list_by_deck_name_with_all_returns_paginate_plan() {
        let plan = to_rpc_plan(&ReviewsCmd::List {
            card_ids: vec![],
            deck_name: Some("Default".into()),
            deck_id: None,
            include_children: true,
            membership: Some("current".into()),
            after_id: None,
            no_manual: false,
            limit: 500,
            cursor: None,
            all: true,
        })
        .unwrap();

        match plan {
            RpcPlan::Paginate {
                method,
                params,
                array_key,
            } => {
                assert_eq!(method, "reviews.list");
                assert_eq!(array_key, "items");
                let value = Value::Object(params);
                assert_eq!(
                    value,
                    json!({
                        "deck": { "name": "Default" },
                        "membership": "current",
                        "limit": 500,
                        "includeChildren": true,
                        "includeManual": true
                    })
                );
                schema::validate_method_params(&method, &value).unwrap();
            }
            other => panic!("Expected RpcPlan::Paginate, got {other:?}"),
        }
    }

    #[test]
    fn list_rejects_simultaneous_card_ids_and_deck() {
        let err = to_rpc_plan(&ReviewsCmd::List {
            card_ids: vec!["101".into()],
            deck_name: Some("Default".into()),
            deck_id: None,
            include_children: false,
            membership: None,
            after_id: None,
            no_manual: false,
            limit: 100,
            cursor: None,
            all: false,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn list_rejects_out_of_range_limit_and_invalid_membership() {
        let err_zero = to_rpc_plan(&ReviewsCmd::List {
            card_ids: vec![],
            deck_name: None,
            deck_id: None,
            include_children: false,
            membership: None,
            after_id: None,
            no_manual: false,
            limit: 0,
            cursor: None,
            all: false,
        })
        .unwrap_err();
        assert!(matches!(err_zero, CliError::InvalidInput(_)));

        let err_high = to_rpc_plan(&ReviewsCmd::List {
            card_ids: vec![],
            deck_name: None,
            deck_id: None,
            include_children: false,
            membership: None,
            after_id: None,
            no_manual: false,
            limit: 1001,
            cursor: None,
            all: false,
        })
        .unwrap_err();
        assert!(matches!(err_high, CliError::InvalidInput(_)));

        let err_membership = to_rpc_plan(&ReviewsCmd::List {
            card_ids: vec![],
            deck_name: None,
            deck_id: None,
            include_children: false,
            membership: Some("all".into()),
            after_id: None,
            no_manual: false,
            limit: 100,
            cursor: None,
            all: false,
        })
        .unwrap_err();
        assert!(matches!(err_membership, CliError::InvalidInput(_)));
    }

    #[test]
    fn latest_id_validates_against_schema_for_all_filter_modes() {
        let unfiltered = to_rpc_plan(&ReviewsCmd::LatestId {
            card_ids: vec![],
            deck_name: None,
            deck_id: None,
            membership: None,
        })
        .unwrap();
        match unfiltered {
            RpcPlan::Call { method, params, .. } => {
                assert_eq!(method, "reviews.getLatestId");
                assert_eq!(params, json!({}));
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("Expected RpcPlan::Call, got {other:?}"),
        }

        let by_deck = to_rpc_plan(&ReviewsCmd::LatestId {
            card_ids: vec![],
            deck_name: None,
            deck_id: Some("42".into()),
            membership: Some("home".into()),
        })
        .unwrap();
        match by_deck {
            RpcPlan::Call { method, params, .. } => {
                assert_eq!(method, "reviews.getLatestId");
                assert_eq!(
                    params,
                    json!({
                        "deck": { "id": "42" },
                        "membership": "home"
                    })
                );
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("Expected RpcPlan::Call, got {other:?}"),
        }

        let by_cards = to_rpc_plan(&ReviewsCmd::LatestId {
            card_ids: vec!["99".into()],
            deck_name: None,
            deck_id: None,
            membership: None,
        })
        .unwrap();
        match by_cards {
            RpcPlan::Call { method, params, .. } => {
                assert_eq!(method, "reviews.getLatestId");
                assert_eq!(params, json!({ "cardIds": ["99"] }));
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("Expected RpcPlan::Call, got {other:?}"),
        }

        let conflict = to_rpc_plan(&ReviewsCmd::LatestId {
            card_ids: vec!["99".into()],
            deck_name: Some("Default".into()),
            deck_id: None,
            membership: None,
        })
        .unwrap_err();
        assert!(matches!(conflict, CliError::InvalidInput(_)));
    }
}

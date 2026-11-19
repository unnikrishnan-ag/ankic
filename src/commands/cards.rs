use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{call_plan, make_ref, make_sort};
use super::RpcPlan;

#[derive(Subcommand)]
pub enum CardsCmd {
    /// Get a card by ID
    Get { id: String },
    /// Get multiple cards by ID
    #[command(name = "get-many")]
    GetMany {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Search cards using Anki query syntax
    Search {
        /// Anki search query (empty string matches all)
        #[arg(default_value = "")]
        query: String,
        #[arg(long, default_value_t = 100)]
        limit: u64,
        #[arg(long)]
        cursor: Option<String>,
        /// Sort column name
        #[arg(long)]
        sort_by: Option<String>,
        /// Sort direction (ascending or descending)
        #[arg(long)]
        sort_dir: Option<String>,
        /// Auto-paginate through all matching IDs
        #[arg(long)]
        all: bool,
    },
    /// Render a card's question, answer, and CSS
    Render { id: String },
    /// Move cards to a target deck
    Move {
        /// Target deck name
        #[arg(long, conflicts_with = "deck_id")]
        deck: Option<String>,
        /// Target deck ID
        #[arg(long, conflicts_with = "deck")]
        deck_id: Option<String>,
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Suspend cards
    Suspend {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Unsuspend cards
    Unsuspend {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Set user flag (0 = none, 1..7 = flag colors)
    Flags {
        #[arg(long)]
        flag: u8,
        #[arg(required = true)]
        ids: Vec<String>,
    },
}

/// Translate a `cards` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &CardsCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        CardsCmd::Get { id } => Ok(call_plan("cards.get", json!({ "id": id }))),
        CardsCmd::GetMany { ids } => Ok(call_plan("cards.getMany", json!({ "ids": ids }))),
        CardsCmd::Search {
            query,
            limit,
            cursor,
            sort_by,
            sort_dir,
            all,
        } => {
            let mut params = Map::new();
            params.insert("query".into(), json!(query));
            params.insert("limit".into(), json!(limit));
            if let Some(c) = cursor {
                params.insert("cursor".into(), json!(c));
            }
            if let Some(sort) = make_sort(sort_by.as_deref(), sort_dir.as_deref())? {
                params.insert("sort".into(), sort);
            }
            if *all {
                Ok(RpcPlan::Paginate {
                    method: "cards.search".into(),
                    params,
                    array_key: "ids",
                })
            } else {
                Ok(call_plan("cards.search", Value::Object(params)))
            }
        }
        CardsCmd::Render { id } => Ok(call_plan("cards.render", json!({ "id": id }))),
        CardsCmd::Move { deck, deck_id, ids } => {
            let deck_ref = make_ref(deck.as_deref(), deck_id.as_deref(), "deck")?;
            Ok(call_plan(
                "cards.move",
                json!({ "ids": ids, "deck": deck_ref }),
            ))
        }
        CardsCmd::Suspend { ids } => Ok(call_plan(
            "cards.setSuspended",
            json!({ "ids": ids, "suspended": true }),
        )),
        CardsCmd::Unsuspend { ids } => Ok(call_plan(
            "cards.setSuspended",
            json!({ "ids": ids, "suspended": false }),
        )),
        CardsCmd::Flags { flag, ids } => {
            if *flag > 7 {
                return Err(CliError::InvalidInput(format!(
                    "Invalid flag '{flag}': expected an integer from 0 to 7"
                )));
            }
            Ok(call_plan(
                "cards.setFlags",
                json!({ "ids": ids, "flag": flag }),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_call_valid(cmd: CardsCmd, expected_method: &str, expected_params: Value) {
        let plan = to_rpc_plan(&cmd).unwrap();
        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                timeout_secs,
            } => {
                assert_eq!(method, expected_method);
                assert_eq!(params, expected_params);
                assert!(!wait);
                assert_eq!(timeout_secs, 0);
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("Expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn get_and_get_many_and_render_validate_against_schema() {
        assert_call_valid(
            CardsCmd::Get { id: "101".into() },
            "cards.get",
            json!({ "id": "101" }),
        );
        assert_call_valid(
            CardsCmd::GetMany {
                ids: vec!["101".into(), "102".into()],
            },
            "cards.getMany",
            json!({ "ids": ["101", "102"] }),
        );
        assert_call_valid(
            CardsCmd::Render { id: "101".into() },
            "cards.render",
            json!({ "id": "101" }),
        );
    }

    #[test]
    fn search_builds_call_or_paginate_and_validates_sort() {
        assert_call_valid(
            CardsCmd::Search {
                query: "deck:Default".into(),
                limit: 50,
                cursor: Some("cur-1".into()),
                sort_by: Some("due".into()),
                sort_dir: Some("desc".into()),
                all: false,
            },
            "cards.search",
            json!({
                "query": "deck:Default",
                "limit": 50,
                "cursor": "cur-1",
                "sort": { "column": "due", "direction": "descending" },
            }),
        );

        let plan = to_rpc_plan(&CardsCmd::Search {
            query: "".into(),
            limit: 100,
            cursor: None,
            sort_by: None,
            sort_dir: None,
            all: true,
        })
        .unwrap();
        match plan {
            RpcPlan::Paginate {
                method,
                params,
                array_key,
            } => {
                assert_eq!(method, "cards.search");
                assert_eq!(array_key, "ids");
                let val = Value::Object(params);
                assert_eq!(val, json!({ "query": "", "limit": 100 }));
                schema::validate_method_params(&method, &val).unwrap();
            }
            other => panic!("Expected RpcPlan::Paginate, got {other:?}"),
        }
    }

    #[test]
    fn move_supports_deck_name_and_deck_id() {
        assert_call_valid(
            CardsCmd::Move {
                deck: Some("Japanese".into()),
                deck_id: None,
                ids: vec!["101".into(), "102".into()],
            },
            "cards.move",
            json!({
                "ids": ["101", "102"],
                "deck": { "name": "Japanese" },
            }),
        );

        assert_call_valid(
            CardsCmd::Move {
                deck: None,
                deck_id: Some("12345".into()),
                ids: vec!["101".into()],
            },
            "cards.move",
            json!({
                "ids": ["101"],
                "deck": { "id": "12345" },
            }),
        );

        let err = to_rpc_plan(&CardsCmd::Move {
            deck: None,
            deck_id: None,
            ids: vec!["101".into()],
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn suspend_unsuspend_and_flags_validate_against_schema() {
        assert_call_valid(
            CardsCmd::Suspend {
                ids: vec!["101".into()],
            },
            "cards.setSuspended",
            json!({ "ids": ["101"], "suspended": true }),
        );
        assert_call_valid(
            CardsCmd::Unsuspend {
                ids: vec!["101".into()],
            },
            "cards.setSuspended",
            json!({ "ids": ["101"], "suspended": false }),
        );
        assert_call_valid(
            CardsCmd::Flags {
                flag: 3,
                ids: vec!["101".into(), "102".into()],
            },
            "cards.setFlags",
            json!({ "ids": ["101", "102"], "flag": 3 }),
        );

        let err = to_rpc_plan(&CardsCmd::Flags {
            flag: 8,
            ids: vec!["101".into()],
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("0 to 7"));
    }
}

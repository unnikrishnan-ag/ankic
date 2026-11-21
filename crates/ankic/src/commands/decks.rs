use clap::Subcommand;
use serde_json::json;

use crate::error::CliError;

use super::helpers::{call_plan, make_ref, make_refs};
use super::RpcPlan;

#[derive(Subcommand)]
pub enum DecksCmd {
    /// List all decks
    List,
    /// Get a deck by name or ID
    Get {
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
    },
    /// Create a normal deck
    Create { name: String },
    /// Rename a deck
    Update {
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
        /// New deck name
        #[arg(long)]
        new_name: String,
    },
    /// Delete one or more decks and their cards (requires --delete-cards)
    Delete {
        #[arg(long, conflicts_with_all = ["id", "names", "ids"])]
        name: Option<String>,
        #[arg(long, conflicts_with_all = ["name", "names", "ids"])]
        id: Option<String>,
        #[arg(long = "names", value_name = "NAME")]
        names: Vec<String>,
        #[arg(long = "ids", value_name = "ID")]
        ids: Vec<String>,
        #[arg(long)]
        delete_cards: bool,
    },
    /// Get due-tree counts for a deck
    Counts {
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
        #[arg(long, conflicts_with = "no_children")]
        include_children: bool,
        #[arg(long, conflicts_with = "include_children")]
        no_children: bool,
    },
}

/// Translate a `decks` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &DecksCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        DecksCmd::List => Ok(call_plan("decks.list", json!({}))),
        DecksCmd::Get { name, id } => Ok(call_plan(
            "decks.get",
            json!({ "deck": make_ref(name.as_deref(), id.as_deref(), "deck")? }),
        )),
        DecksCmd::Create { name } => Ok(call_plan("decks.create", json!({ "name": name }))),
        DecksCmd::Update { name, id, new_name } => Ok(call_plan(
            "decks.update",
            json!({
                "deck": make_ref(name.as_deref(), id.as_deref(), "deck")?,
                "name": new_name,
            }),
        )),
        DecksCmd::Delete {
            name,
            id,
            names,
            ids,
            delete_cards,
        } => {
            if !*delete_cards {
                return Err(CliError::InvalidInput(
                    "decks.delete requires --delete-cards to confirm card deletion".into(),
                ));
            }
            let decks = if name.is_some() || id.is_some() {
                if !names.is_empty() || !ids.is_empty() {
                    return Err(CliError::InvalidInput(
                        "Use either --name/--id or --names/--ids, not both".into(),
                    ));
                }
                vec![make_ref(name.as_deref(), id.as_deref(), "deck")?]
            } else {
                make_refs(names, ids, "deck")?
            };
            Ok(call_plan(
                "decks.delete",
                json!({
                    "decks": decks,
                    "deleteCards": true,
                }),
            ))
        }
        DecksCmd::Counts {
            name,
            id,
            include_children: _,
            no_children,
        } => Ok(call_plan(
            "decks.getCounts",
            json!({
                "deck": make_ref(name.as_deref(), id.as_deref(), "deck")?,
                "includeChildren": !*no_children,
            }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::schema;

    fn assert_valid_call(cmd: DecksCmd, expected_method: &str) -> Value {
        let plan = to_rpc_plan(&cmd).expect("build decks rpc plan");
        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                ..
            } => {
                assert_eq!(method, expected_method);
                assert!(!wait);
                schema::validate_method_params(&method, &params).unwrap_or_else(|e| {
                    panic!("schema validation failed for {method}: {e}");
                });
                params
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn decks_list_get_create_update_produce_valid_plans() {
        assert_eq!(assert_valid_call(DecksCmd::List, "decks.list"), json!({}));

        assert_eq!(
            assert_valid_call(
                DecksCmd::Get {
                    name: Some("Default".into()),
                    id: None,
                },
                "decks.get",
            ),
            json!({ "deck": { "name": "Default" } })
        );
        assert_eq!(
            assert_valid_call(
                DecksCmd::Get {
                    name: None,
                    id: Some("1".into()),
                },
                "decks.get",
            ),
            json!({ "deck": { "id": "1" } })
        );

        assert_eq!(
            assert_valid_call(
                DecksCmd::Create {
                    name: "Japanese::Vocab".into(),
                },
                "decks.create",
            ),
            json!({ "name": "Japanese::Vocab" })
        );

        assert_eq!(
            assert_valid_call(
                DecksCmd::Update {
                    name: None,
                    id: Some("42".into()),
                    new_name: "Japanese::Core".into(),
                },
                "decks.update",
            ),
            json!({
                "deck": { "id": "42" },
                "name": "Japanese::Core",
            })
        );
    }

    #[test]
    fn decks_delete_supports_single_and_multi_deck_and_requires_confirmation() {
        assert_eq!(
            assert_valid_call(
                DecksCmd::Delete {
                    name: Some("Temp".into()),
                    id: None,
                    names: vec![],
                    ids: vec![],
                    delete_cards: true,
                },
                "decks.delete",
            ),
            json!({
                "decks": [{ "name": "Temp" }],
                "deleteCards": true,
            })
        );

        assert_eq!(
            assert_valid_call(
                DecksCmd::Delete {
                    name: None,
                    id: None,
                    names: vec!["Temp1".into(), "Temp2".into()],
                    ids: vec!["99".into()],
                    delete_cards: true,
                },
                "decks.delete",
            ),
            json!({
                "decks": [
                    { "name": "Temp1" },
                    { "name": "Temp2" },
                    { "id": "99" },
                ],
                "deleteCards": true,
            })
        );

        let err = to_rpc_plan(&DecksCmd::Delete {
            name: Some("Temp".into()),
            id: None,
            names: vec![],
            ids: vec![],
            delete_cards: false,
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("--delete-cards"));

        let empty_err = to_rpc_plan(&DecksCmd::Delete {
            name: None,
            id: None,
            names: vec![],
            ids: vec![],
            delete_cards: true,
        })
        .unwrap_err();
        assert_eq!(empty_err.exit_code(), 2);
    }

    #[test]
    fn decks_counts_defaults_include_children_true_and_supports_no_children() {
        assert_eq!(
            assert_valid_call(
                DecksCmd::Counts {
                    name: Some("Default".into()),
                    id: None,
                    include_children: false,
                    no_children: false,
                },
                "decks.getCounts",
            ),
            json!({
                "deck": { "name": "Default" },
                "includeChildren": true,
            })
        );

        assert_eq!(
            assert_valid_call(
                DecksCmd::Counts {
                    name: None,
                    id: Some("1".into()),
                    include_children: true,
                    no_children: false,
                },
                "decks.getCounts",
            ),
            json!({
                "deck": { "id": "1" },
                "includeChildren": true,
            })
        );

        assert_eq!(
            assert_valid_call(
                DecksCmd::Counts {
                    name: Some("Default".into()),
                    id: None,
                    include_children: false,
                    no_children: true,
                },
                "decks.getCounts",
            ),
            json!({
                "deck": { "name": "Default" },
                "includeChildren": false,
            })
        );
    }
}

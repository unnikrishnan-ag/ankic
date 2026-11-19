use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{call_plan, make_ref, make_refs, parse_yaml_or_json_object};
use super::RpcPlan;

#[derive(Subcommand)]
pub enum DeckPresetsCmd {
    /// List all deck presets
    List,
    /// Get a deck preset by name or ID
    Get {
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
    },
    /// Create a deck preset (optionally cloning an existing preset)
    Create {
        name: String,
        #[arg(long, conflicts_with = "clone_from_id")]
        clone_from_name: Option<String>,
        #[arg(long, conflicts_with = "clone_from_name")]
        clone_from_id: Option<String>,
    },
    /// Update a deck preset's name and/or settings (waits for completion unless --no-wait)
    Update {
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
        /// New preset name
        #[arg(long)]
        new_name: Option<String>,
        /// YAML or JSON PresetPatch object
        #[arg(long)]
        settings: Option<String>,
        /// Reschedule cards using FSRS after updating settings
        #[arg(long)]
        reschedule_fsrs: bool,
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Assign a deck preset to one or more decks (waits for completion unless --no-wait)
    Assign {
        #[arg(long, conflicts_with = "preset_id")]
        preset_name: Option<String>,
        #[arg(long, conflicts_with = "preset_name")]
        preset_id: Option<String>,
        #[arg(long = "deck-name", value_name = "NAME")]
        deck_names: Vec<String>,
        #[arg(long = "deck-id", value_name = "ID")]
        deck_ids: Vec<String>,
        /// Reschedule cards using FSRS after assigning the preset
        #[arg(long)]
        reschedule_fsrs: bool,
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Delete a deck preset (optionally specifying a replacement preset)
    Delete {
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
        #[arg(long, conflicts_with = "replacement_id")]
        replacement_name: Option<String>,
        #[arg(long, conflicts_with = "replacement_name")]
        replacement_id: Option<String>,
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
}

/// Translate a `deck-presets` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &DeckPresetsCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        DeckPresetsCmd::List => Ok(call_plan("deckPresets.list", json!({}))),
        DeckPresetsCmd::Get { name, id } => Ok(call_plan(
            "deckPresets.get",
            json!({ "preset": make_ref(name.as_deref(), id.as_deref(), "preset")? }),
        )),
        DeckPresetsCmd::Create {
            name,
            clone_from_name,
            clone_from_id,
        } => {
            let mut params = Map::new();
            params.insert("name".into(), json!(name));
            if clone_from_name.is_some() || clone_from_id.is_some() {
                params.insert(
                    "cloneFrom".into(),
                    make_ref(
                        clone_from_name.as_deref(),
                        clone_from_id.as_deref(),
                        "cloneFrom",
                    )?,
                );
            }
            Ok(call_plan("deckPresets.create", Value::Object(params)))
        }
        DeckPresetsCmd::Update {
            name,
            id,
            new_name,
            settings,
            reschedule_fsrs,
            no_wait,
            timeout,
        } => {
            if new_name.is_none() && settings.is_none() {
                return Err(CliError::InvalidInput(
                    "deckPresets.update requires at least one of --new-name or --settings".into(),
                ));
            }
            let mut params = Map::new();
            params.insert(
                "preset".into(),
                make_ref(name.as_deref(), id.as_deref(), "preset")?,
            );
            if let Some(new_name) = new_name {
                params.insert("name".into(), json!(new_name));
            }
            if let Some(raw_settings) = settings {
                let patch = parse_yaml_or_json_object(raw_settings, "--settings")?;
                params.insert("settings".into(), Value::Object(patch));
            }
            if *reschedule_fsrs {
                params.insert("rescheduleFsrs".into(), json!(true));
            }
            Ok(RpcPlan::Call {
                method: "deckPresets.update".into(),
                params: Value::Object(params),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        DeckPresetsCmd::Assign {
            preset_name,
            preset_id,
            deck_names,
            deck_ids,
            reschedule_fsrs,
            no_wait,
            timeout,
        } => {
            let mut params = Map::new();
            params.insert(
                "preset".into(),
                make_ref(preset_name.as_deref(), preset_id.as_deref(), "preset")?,
            );
            params.insert(
                "decks".into(),
                Value::Array(make_refs(deck_names, deck_ids, "deck")?),
            );
            if *reschedule_fsrs {
                params.insert("rescheduleFsrs".into(), json!(true));
            }
            Ok(RpcPlan::Call {
                method: "deckPresets.assign".into(),
                params: Value::Object(params),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        DeckPresetsCmd::Delete {
            name,
            id,
            replacement_name,
            replacement_id,
            no_wait,
            timeout,
        } => {
            let mut params = Map::new();
            params.insert(
                "preset".into(),
                make_ref(name.as_deref(), id.as_deref(), "preset")?,
            );
            if replacement_name.is_some() || replacement_id.is_some() {
                params.insert(
                    "replacement".into(),
                    make_ref(
                        replacement_name.as_deref(),
                        replacement_id.as_deref(),
                        "replacement",
                    )?,
                );
            }
            Ok(RpcPlan::Call {
                method: "deckPresets.delete".into(),
                params: Value::Object(params),
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
        cmd: DeckPresetsCmd,
        expected_method: &str,
        expected_wait: bool,
        expected_timeout: u64,
    ) -> Value {
        let plan = to_rpc_plan(&cmd).expect("build deck-presets rpc plan");
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
                schema::validate_method_params(&method, &params).unwrap_or_else(|e| {
                    panic!("schema validation failed for {method}: {e}");
                });
                params
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn deck_presets_list_get_create_produce_valid_plans() {
        assert_eq!(
            assert_valid_call(DeckPresetsCmd::List, "deckPresets.list", false, 0),
            json!({})
        );

        assert_eq!(
            assert_valid_call(
                DeckPresetsCmd::Get {
                    name: Some("Default".into()),
                    id: None,
                },
                "deckPresets.get",
                false,
                0,
            ),
            json!({ "preset": { "name": "Default" } })
        );

        assert_eq!(
            assert_valid_call(
                DeckPresetsCmd::Create {
                    name: "FSRS Intensive".into(),
                    clone_from_name: None,
                    clone_from_id: None,
                },
                "deckPresets.create",
                false,
                0,
            ),
            json!({ "name": "FSRS Intensive" })
        );

        assert_eq!(
            assert_valid_call(
                DeckPresetsCmd::Create {
                    name: "FSRS Clone".into(),
                    clone_from_name: None,
                    clone_from_id: Some("1".into()),
                },
                "deckPresets.create",
                false,
                0,
            ),
            json!({
                "name": "FSRS Clone",
                "cloneFrom": { "id": "1" },
            })
        );
    }

    #[test]
    fn deck_presets_update_validates_and_parses_yaml_or_json() {
        let params = assert_valid_call(
            DeckPresetsCmd::Update {
                name: Some("Default".into()),
                id: None,
                new_name: Some("Renamed".into()),
                settings: Some("newPerDay: 30\nreviewsPerDay: 200".into()),
                reschedule_fsrs: true,
                no_wait: false,
                timeout: 300,
            },
            "deckPresets.update",
            true,
            300,
        );
        assert_eq!(
            params,
            json!({
                "preset": { "name": "Default" },
                "name": "Renamed",
                "settings": { "newPerDay": 30, "reviewsPerDay": 200 },
                "rescheduleFsrs": true,
            })
        );

        let no_wait_params = assert_valid_call(
            DeckPresetsCmd::Update {
                name: None,
                id: Some("42".into()),
                new_name: None,
                settings: Some(r#"{"newPerDay": 15}"#.into()),
                reschedule_fsrs: false,
                no_wait: true,
                timeout: 120,
            },
            "deckPresets.update",
            false,
            120,
        );
        assert_eq!(
            no_wait_params,
            json!({
                "preset": { "id": "42" },
                "settings": { "newPerDay": 15 },
            })
        );

        let err = to_rpc_plan(&DeckPresetsCmd::Update {
            name: Some("Default".into()),
            id: None,
            new_name: None,
            settings: None,
            reschedule_fsrs: false,
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn deck_presets_assign_and_delete_produce_valid_plans() {
        let assign_params = assert_valid_call(
            DeckPresetsCmd::Assign {
                preset_name: Some("FSRS".into()),
                preset_id: None,
                deck_names: vec!["Japanese".into()],
                deck_ids: vec!["99".into()],
                reschedule_fsrs: true,
                no_wait: false,
                timeout: 300,
            },
            "deckPresets.assign",
            true,
            300,
        );
        assert_eq!(
            assign_params,
            json!({
                "preset": { "name": "FSRS" },
                "decks": [
                    { "name": "Japanese" },
                    { "id": "99" },
                ],
                "rescheduleFsrs": true,
            })
        );

        let delete_params = assert_valid_call(
            DeckPresetsCmd::Delete {
                name: None,
                id: Some("10".into()),
                replacement_name: Some("Default".into()),
                replacement_id: None,
                no_wait: true,
                timeout: 60,
            },
            "deckPresets.delete",
            false,
            60,
        );
        assert_eq!(
            delete_params,
            json!({
                "preset": { "id": "10" },
                "replacement": { "name": "Default" },
            })
        );

        let delete_minimal = assert_valid_call(
            DeckPresetsCmd::Delete {
                name: Some("Unused".into()),
                id: None,
                replacement_name: None,
                replacement_id: None,
                no_wait: false,
                timeout: 300,
            },
            "deckPresets.delete",
            true,
            300,
        );
        assert_eq!(
            delete_minimal,
            json!({
                "preset": { "name": "Unused" },
            })
        );
    }
}

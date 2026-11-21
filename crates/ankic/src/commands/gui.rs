use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{call_plan, make_ref, make_sort, parse_yaml_or_json_object};
use super::RpcPlan;

const VALID_NAVIGATE_VIEWS: [&str; 3] = ["deckBrowser", "deckOverview", "review"];
const VALID_DRAFT_MODES: [&str; 2] = ["replace", "append"];
const VALID_RATINGS: [&str; 4] = ["again", "hard", "good", "easy"];

#[derive(Subcommand)]
pub enum GuiCmd {
    /// Navigate the Anki GUI to deckBrowser, deckOverview, or review
    Navigate {
        /// Target view (deckBrowser, deckOverview, review)
        view: String,
        /// Target deck name (required for deckOverview and review; forbidden for deckBrowser)
        #[arg(long, visible_aliases = ["deck", "name"], conflicts_with = "deck_id")]
        deck_name: Option<String>,
        /// Target deck ID (required for deckOverview and review; forbidden for deckBrowser)
        #[arg(long, visible_alias = "id", conflicts_with = "deck_name")]
        deck_id: Option<String>,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Open or trigger file import in the Anki GUI
    #[command(name = "import-file")]
    ImportFile {
        /// Optional local filesystem path to import
        #[arg(long)]
        path: Option<String>,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Request Anki to exit gracefully
    Exit,
    /// Open the card browser window with an optional search query and sort order
    #[command(name = "browser-open")]
    BrowserOpen {
        /// Optional Anki search query
        #[arg(long)]
        query: Option<String>,
        /// Optional sort column name
        #[arg(long)]
        sort_by: Option<String>,
        /// Optional sort direction (ascending or descending)
        #[arg(long)]
        sort_dir: Option<String>,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Get the currently selected cards and notes in the card browser
    #[command(name = "browser-selection")]
    BrowserSelection,
    /// Select specific cards or notes in the card browser
    #[command(name = "browser-select")]
    BrowserSelect {
        /// Card IDs to select (mutually exclusive with --note-id)
        #[arg(long = "card-id")]
        card_ids: Vec<String>,
        /// Note IDs to select (mutually exclusive with --card-id)
        #[arg(long = "note-id")]
        note_ids: Vec<String>,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Open an existing note in the note editor window
    #[command(name = "editor-open-note")]
    EditorOpenNote {
        /// Note ID to open
        note_id: String,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Open the Add Cards window, optionally pre-populated with a draft note
    #[command(name = "editor-open-add")]
    EditorOpenAdd {
        /// YAML or JSON DraftInput object
        #[arg(long)]
        note: Option<String>,
        /// Target deck name
        #[arg(long, conflicts_with = "deck_id")]
        deck_name: Option<String>,
        /// Target deck ID
        #[arg(long, conflicts_with = "deck_name")]
        deck_id: Option<String>,
        /// Target note type name
        #[arg(long, conflicts_with = "note_type_id")]
        note_type_name: Option<String>,
        /// Target note type ID
        #[arg(long, conflicts_with = "note_type_name")]
        note_type_id: Option<String>,
        /// Field values as a YAML or JSON object
        #[arg(long)]
        fields: Option<String>,
        /// Tag to attach to the draft note (repeatable)
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Update an open Add/Edit note draft in the GUI
    #[command(name = "editor-update-draft")]
    EditorUpdateDraft {
        /// Editor instance ID
        editor_id: String,
        /// YAML or JSON EditorPatch object
        #[arg(long)]
        patch: Option<String>,
        /// Updated target deck name
        #[arg(long, conflicts_with = "deck_id")]
        deck_name: Option<String>,
        /// Updated target deck ID
        #[arg(long, conflicts_with = "deck_name")]
        deck_id: Option<String>,
        /// Updated note type name
        #[arg(long, conflicts_with = "note_type_id")]
        note_type_name: Option<String>,
        /// Updated note type ID
        #[arg(long, conflicts_with = "note_type_name")]
        note_type_id: Option<String>,
        /// Updated field values as a YAML or JSON object
        #[arg(long)]
        fields: Option<String>,
        /// Updated tags (repeatable)
        #[arg(long = "tag")]
        tags: Option<Vec<String>>,
        /// Patch application mode (replace or append)
        #[arg(long)]
        mode: Option<String>,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Get the current reviewer state (active card, phase, stateToken)
    #[command(name = "reviewer-get")]
    ReviewerGet,
    /// Start the answer timer for the current reviewer card
    #[command(name = "reviewer-start-timer")]
    ReviewerStartTimer {
        /// Reviewer state token from reviewer.get
        #[arg(long)]
        state_token: String,
    },
    /// Show the question side of the current reviewer card
    #[command(name = "reviewer-show-question")]
    ReviewerShowQuestion {
        /// Reviewer state token from reviewer.get
        #[arg(long)]
        state_token: String,
    },
    /// Show the answer side of the current reviewer card
    #[command(name = "reviewer-show-answer")]
    ReviewerShowAnswer {
        /// Reviewer state token from reviewer.get
        #[arg(long)]
        state_token: String,
    },
    /// Submit an answer rating for the current reviewer card
    #[command(name = "reviewer-answer")]
    ReviewerAnswer {
        /// Reviewer state token from reviewer.get
        #[arg(long)]
        state_token: String,
        /// Answer rating (again, hard, good, easy)
        #[arg(long)]
        rating: String,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    /// Play audio for the current reviewer card
    #[command(name = "reviewer-play-audio")]
    ReviewerPlayAudio {
        /// Reviewer state token from reviewer.get
        #[arg(long)]
        state_token: String,
    },
}

fn validate_non_empty_ids(ids: &[String], flag_name: &str) -> Result<(), CliError> {
    if ids.iter().any(|id| id.trim().is_empty()) {
        return Err(CliError::InvalidInput(format!(
            "{flag_name} values must not be empty"
        )));
    }
    Ok(())
}

fn validate_non_empty_tags(tags: &[String]) -> Result<(), CliError> {
    if tags.iter().any(|t| t.trim().is_empty()) {
        return Err(CliError::InvalidInput(
            "Tags must not be empty strings".into(),
        ));
    }
    Ok(())
}

fn validate_state_token(state_token: &str) -> Result<(), CliError> {
    if state_token.trim().is_empty() {
        return Err(CliError::InvalidInput(
            "State token must not be empty".into(),
        ));
    }
    Ok(())
}

fn reviewer_state_plan(method: &str, state_token: &str) -> Result<RpcPlan, CliError> {
    validate_state_token(state_token)?;
    Ok(call_plan(method, json!({ "stateToken": state_token })))
}

/// Translate a `gui` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &GuiCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        GuiCmd::Navigate {
            view,
            deck_name,
            deck_id,
            no_wait,
            timeout,
        } => {
            if !VALID_NAVIGATE_VIEWS.contains(&view.as_str()) {
                return Err(CliError::InvalidInput(format!(
                    "Invalid view '{view}': expected one of {}",
                    VALID_NAVIGATE_VIEWS.join(", ")
                )));
            }

            let params = if view == "deckBrowser" {
                if deck_name.is_some() || deck_id.is_some() {
                    return Err(CliError::InvalidInput(
                        "application.navigate with view 'deckBrowser' must not specify --deck-name or --deck-id".into(),
                    ));
                }
                json!({ "view": "deckBrowser" })
            } else {
                let deck = make_ref(deck_name.as_deref(), deck_id.as_deref(), "deck")?;
                json!({ "view": view, "deck": deck })
            };

            Ok(RpcPlan::Call {
                method: "application.navigate".into(),
                params,
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::ImportFile {
            path,
            no_wait,
            timeout,
        } => {
            let mut params = Map::new();
            if let Some(p) = path {
                if p.trim().is_empty() {
                    return Err(CliError::InvalidInput(
                        "Import path must not be empty".into(),
                    ));
                }
                params.insert("path".into(), json!(p));
            }
            Ok(RpcPlan::Call {
                method: "application.importFile".into(),
                params: Value::Object(params),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::Exit => Ok(call_plan("application.exit", json!({}))),
        GuiCmd::BrowserOpen {
            query,
            sort_by,
            sort_dir,
            no_wait,
            timeout,
        } => {
            let mut params = Map::new();
            if let Some(q) = query {
                params.insert("query".into(), json!(q));
            }
            if let Some(sort) = make_sort(sort_by.as_deref(), sort_dir.as_deref())? {
                params.insert("sort".into(), sort);
            }
            Ok(RpcPlan::Call {
                method: "browser.open".into(),
                params: Value::Object(params),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::BrowserSelection => Ok(call_plan("browser.getSelection", json!({}))),
        GuiCmd::BrowserSelect {
            card_ids,
            note_ids,
            no_wait,
            timeout,
        } => {
            let params = match (card_ids.is_empty(), note_ids.is_empty()) {
                (false, true) => {
                    validate_non_empty_ids(card_ids, "--card-id")?;
                    json!({ "cardIds": card_ids })
                }
                (true, false) => {
                    validate_non_empty_ids(note_ids, "--note-id")?;
                    json!({ "noteIds": note_ids })
                }
                (false, false) => {
                    return Err(CliError::InvalidInput(
                        "browser.select accepts either --card-id or --note-id, not both".into(),
                    ));
                }
                (true, true) => {
                    return Err(CliError::InvalidInput(
                        "browser.select requires either --card-id or --note-id".into(),
                    ));
                }
            };
            Ok(RpcPlan::Call {
                method: "browser.select".into(),
                params,
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::EditorOpenNote {
            note_id,
            no_wait,
            timeout,
        } => {
            if note_id.trim().is_empty() {
                return Err(CliError::InvalidInput(
                    "Note ID must not be empty".into(),
                ));
            }
            Ok(RpcPlan::Call {
                method: "editors.openNote".into(),
                params: json!({ "noteId": note_id }),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::EditorOpenAdd {
            note,
            deck_name,
            deck_id,
            note_type_name,
            note_type_id,
            fields,
            tags,
            no_wait,
            timeout,
        } => {
            let has_draft_flags = deck_name.is_some()
                || deck_id.is_some()
                || note_type_name.is_some()
                || note_type_id.is_some()
                || fields.is_some()
                || !tags.is_empty();

            let params = if let Some(raw_note) = note {
                if has_draft_flags {
                    return Err(CliError::InvalidInput(
                        "Provide either --note or individual draft flags, not both".into(),
                    ));
                }
                let note_obj = parse_yaml_or_json_object(raw_note, "--note")?;
                json!({ "note": note_obj })
            } else if !has_draft_flags {
                json!({})
            } else {
                let deck = make_ref(deck_name.as_deref(), deck_id.as_deref(), "deck")?;
                let note_type = make_ref(
                    note_type_name.as_deref(),
                    note_type_id.as_deref(),
                    "note-type",
                )?;
                let mut note_map = Map::new();
                note_map.insert("deck".into(), deck);
                note_map.insert("noteType".into(), note_type);
                if let Some(raw_fields) = fields {
                    let fields_map = parse_yaml_or_json_object(raw_fields, "--fields")?;
                    note_map.insert("fields".into(), Value::Object(fields_map));
                }
                if !tags.is_empty() {
                    validate_non_empty_tags(tags)?;
                    note_map.insert("tags".into(), json!(tags));
                }
                json!({ "note": note_map })
            };

            Ok(RpcPlan::Call {
                method: "editors.openAdd".into(),
                params,
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::EditorUpdateDraft {
            editor_id,
            patch,
            deck_name,
            deck_id,
            note_type_name,
            note_type_id,
            fields,
            tags,
            mode,
            no_wait,
            timeout,
        } => {
            if editor_id.trim().is_empty() {
                return Err(CliError::InvalidInput(
                    "Editor ID must not be empty".into(),
                ));
            }

            let mut patch_map = match patch {
                Some(raw) => parse_yaml_or_json_object(raw, "--patch")?,
                None => Map::new(),
            };

            if deck_name.is_some() || deck_id.is_some() {
                let deck = make_ref(deck_name.as_deref(), deck_id.as_deref(), "deck")?;
                patch_map.insert("deck".into(), deck);
            }
            if note_type_name.is_some() || note_type_id.is_some() {
                let note_type = make_ref(
                    note_type_name.as_deref(),
                    note_type_id.as_deref(),
                    "note-type",
                )?;
                patch_map.insert("noteType".into(), note_type);
            }
            if let Some(raw_fields) = fields {
                let fields_map = parse_yaml_or_json_object(raw_fields, "--fields")?;
                patch_map.insert("fields".into(), Value::Object(fields_map));
            }
            if let Some(tag_list) = tags {
                validate_non_empty_tags(tag_list)?;
                patch_map.insert("tags".into(), json!(tag_list));
            }

            if patch_map.is_empty() {
                return Err(CliError::InvalidInput(
                    "editors.updateDraft requires at least one field in --patch or convenience flags".into(),
                ));
            }

            let mut params = Map::new();
            params.insert("editorId".into(), json!(editor_id));
            params.insert("patch".into(), Value::Object(patch_map));

            if let Some(m) = mode {
                if !VALID_DRAFT_MODES.contains(&m.as_str()) {
                    return Err(CliError::InvalidInput(format!(
                        "Invalid mode '{m}': expected one of {}",
                        VALID_DRAFT_MODES.join(", ")
                    )));
                }
                params.insert("mode".into(), json!(m));
            }

            Ok(RpcPlan::Call {
                method: "editors.updateDraft".into(),
                params: Value::Object(params),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::ReviewerGet => Ok(call_plan("reviewer.get", json!({}))),
        GuiCmd::ReviewerStartTimer { state_token } => {
            reviewer_state_plan("reviewer.startTimer", state_token)
        }
        GuiCmd::ReviewerShowQuestion { state_token } => {
            reviewer_state_plan("reviewer.showQuestion", state_token)
        }
        GuiCmd::ReviewerShowAnswer { state_token } => {
            reviewer_state_plan("reviewer.showAnswer", state_token)
        }
        GuiCmd::ReviewerAnswer {
            state_token,
            rating,
            no_wait,
            timeout,
        } => {
            validate_state_token(state_token)?;
            if !VALID_RATINGS.contains(&rating.as_str()) {
                return Err(CliError::InvalidInput(format!(
                    "Invalid rating '{rating}': expected one of {}",
                    VALID_RATINGS.join(", ")
                )));
            }
            Ok(RpcPlan::Call {
                method: "reviewer.answer".into(),
                params: json!({
                    "stateToken": state_token,
                    "rating": rating,
                }),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        GuiCmd::ReviewerPlayAudio { state_token } => {
            reviewer_state_plan("reviewer.playAudio", state_token)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_call_schema(cmd: &GuiCmd, expected_method: &str, expected_wait: bool) -> Value {
        let plan = to_rpc_plan(cmd).unwrap();
        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                ..
            } => {
                assert_eq!(method, expected_method);
                assert_eq!(wait, expected_wait);
                schema::validate_method_params(&method, &params).unwrap();
                params
            }
            other => panic!("Expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn navigate_deck_browser_overview_and_review() {
        let params = assert_call_schema(
            &GuiCmd::Navigate {
                view: "deckBrowser".into(),
                deck_name: None,
                deck_id: None,
                no_wait: false,
                timeout: 60,
            },
            "application.navigate",
            true,
        );
        assert_eq!(params, json!({ "view": "deckBrowser" }));

        let params = assert_call_schema(
            &GuiCmd::Navigate {
                view: "deckOverview".into(),
                deck_name: Some("Default".into()),
                deck_id: None,
                no_wait: true,
                timeout: 60,
            },
            "application.navigate",
            false,
        );
        assert_eq!(
            params,
            json!({ "view": "deckOverview", "deck": { "name": "Default" } })
        );

        let params = assert_call_schema(
            &GuiCmd::Navigate {
                view: "review".into(),
                deck_name: None,
                deck_id: Some("1".into()),
                no_wait: false,
                timeout: 60,
            },
            "application.navigate",
            true,
        );
        assert_eq!(params, json!({ "view": "review", "deck": { "id": "1" } }));
    }

    #[test]
    fn navigate_rejects_invalid_deck_combinations() {
        let err = to_rpc_plan(&GuiCmd::Navigate {
            view: "deckBrowser".into(),
            deck_name: Some("Default".into()),
            deck_id: None,
            no_wait: false,
            timeout: 60,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));

        let err = to_rpc_plan(&GuiCmd::Navigate {
            view: "review".into(),
            deck_name: None,
            deck_id: None,
            no_wait: false,
            timeout: 60,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn import_file_and_exit() {
        let params = assert_call_schema(
            &GuiCmd::ImportFile {
                path: None,
                no_wait: false,
                timeout: 300,
            },
            "application.importFile",
            true,
        );
        assert_eq!(params, json!({}));

        let params = assert_call_schema(
            &GuiCmd::ImportFile {
                path: Some("/tmp/deck.apkg".into()),
                no_wait: true,
                timeout: 300,
            },
            "application.importFile",
            false,
        );
        assert_eq!(params, json!({ "path": "/tmp/deck.apkg" }));

        let params = assert_call_schema(&GuiCmd::Exit, "application.exit", false);
        assert_eq!(params, json!({}));
    }

    #[test]
    fn browser_open_selection_and_select() {
        let params = assert_call_schema(
            &GuiCmd::BrowserOpen {
                query: Some("deck:Default".into()),
                sort_by: Some("cardDue".into()),
                sort_dir: Some("ascending".into()),
                no_wait: false,
                timeout: 60,
            },
            "browser.open",
            true,
        );
        assert_eq!(
            params,
            json!({
                "query": "deck:Default",
                "sort": { "column": "cardDue", "direction": "ascending" }
            })
        );

        let params = assert_call_schema(&GuiCmd::BrowserSelection, "browser.getSelection", false);
        assert_eq!(params, json!({}));

        let params = assert_call_schema(
            &GuiCmd::BrowserSelect {
                card_ids: vec!["101".into(), "102".into()],
                note_ids: vec![],
                no_wait: false,
                timeout: 60,
            },
            "browser.select",
            true,
        );
        assert_eq!(params, json!({ "cardIds": ["101", "102"] }));

        let params = assert_call_schema(
            &GuiCmd::BrowserSelect {
                card_ids: vec![],
                note_ids: vec!["201".into()],
                no_wait: true,
                timeout: 60,
            },
            "browser.select",
            false,
        );
        assert_eq!(params, json!({ "noteIds": ["201"] }));
    }

    #[test]
    fn browser_select_rejects_conflict_and_empty() {
        let err = to_rpc_plan(&GuiCmd::BrowserSelect {
            card_ids: vec!["101".into()],
            note_ids: vec!["201".into()],
            no_wait: false,
            timeout: 60,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));

        let err = to_rpc_plan(&GuiCmd::BrowserSelect {
            card_ids: vec![],
            note_ids: vec![],
            no_wait: false,
            timeout: 60,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn editors_open_note_open_add_and_update_draft() {
        let params = assert_call_schema(
            &GuiCmd::EditorOpenNote {
                note_id: "12345".into(),
                no_wait: false,
                timeout: 60,
            },
            "editors.openNote",
            true,
        );
        assert_eq!(params, json!({ "noteId": "12345" }));

        let params = assert_call_schema(
            &GuiCmd::EditorOpenAdd {
                note: None,
                deck_name: None,
                deck_id: None,
                note_type_name: None,
                note_type_id: None,
                fields: None,
                tags: vec![],
                no_wait: false,
                timeout: 60,
            },
            "editors.openAdd",
            true,
        );
        assert_eq!(params, json!({}));

        let params = assert_call_schema(
            &GuiCmd::EditorOpenAdd {
                note: None,
                deck_name: Some("Default".into()),
                deck_id: None,
                note_type_name: Some("Basic".into()),
                note_type_id: None,
                fields: Some("Front: Hello\nBack: World".into()),
                tags: vec!["vocab".into()],
                no_wait: true,
                timeout: 60,
            },
            "editors.openAdd",
            false,
        );
        assert_eq!(
            params,
            json!({
                "note": {
                    "deck": { "name": "Default" },
                    "noteType": { "name": "Basic" },
                    "fields": { "Front": "Hello", "Back": "World" },
                    "tags": ["vocab"]
                }
            })
        );

        let params = assert_call_schema(
            &GuiCmd::EditorUpdateDraft {
                editor_id: "ed-1".into(),
                patch: Some("{fields: {Front: Updated}}".into()),
                deck_name: Some("Japanese".into()),
                deck_id: None,
                note_type_name: None,
                note_type_id: None,
                fields: None,
                tags: Some(vec!["tag1".into()]),
                mode: Some("append".into()),
                no_wait: false,
                timeout: 60,
            },
            "editors.updateDraft",
            true,
        );
        assert_eq!(
            params,
            json!({
                "editorId": "ed-1",
                "patch": {
                    "fields": { "Front": "Updated" },
                    "deck": { "name": "Japanese" },
                    "tags": ["tag1"]
                },
                "mode": "append"
            })
        );
    }

    #[test]
    fn editor_update_draft_rejects_empty_patch() {
        let err = to_rpc_plan(&GuiCmd::EditorUpdateDraft {
            editor_id: "ed-1".into(),
            patch: None,
            deck_name: None,
            deck_id: None,
            note_type_name: None,
            note_type_id: None,
            fields: None,
            tags: None,
            mode: None,
            no_wait: false,
            timeout: 60,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn reviewer_subcommands_validate_against_schema() {
        let params = assert_call_schema(&GuiCmd::ReviewerGet, "reviewer.get", false);
        assert_eq!(params, json!({}));

        let params = assert_call_schema(
            &GuiCmd::ReviewerStartTimer {
                state_token: "tok-1".into(),
            },
            "reviewer.startTimer",
            false,
        );
        assert_eq!(params, json!({ "stateToken": "tok-1" }));

        let params = assert_call_schema(
            &GuiCmd::ReviewerShowQuestion {
                state_token: "tok-1".into(),
            },
            "reviewer.showQuestion",
            false,
        );
        assert_eq!(params, json!({ "stateToken": "tok-1" }));

        let params = assert_call_schema(
            &GuiCmd::ReviewerShowAnswer {
                state_token: "tok-1".into(),
            },
            "reviewer.showAnswer",
            false,
        );
        assert_eq!(params, json!({ "stateToken": "tok-1" }));

        let params = assert_call_schema(
            &GuiCmd::ReviewerAnswer {
                state_token: "tok-1".into(),
                rating: "good".into(),
                no_wait: false,
                timeout: 60,
            },
            "reviewer.answer",
            true,
        );
        assert_eq!(params, json!({ "stateToken": "tok-1", "rating": "good" }));

        let params = assert_call_schema(
            &GuiCmd::ReviewerPlayAudio {
                state_token: "tok-1".into(),
            },
            "reviewer.playAudio",
            false,
        );
        assert_eq!(params, json!({ "stateToken": "tok-1" }));
    }

    #[test]
    fn reviewer_rejects_invalid_rating_and_empty_state_token() {
        let err = to_rpc_plan(&GuiCmd::ReviewerAnswer {
            state_token: "tok-1".into(),
            rating: "skip".into(),
            no_wait: false,
            timeout: 60,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));

        let err = to_rpc_plan(&GuiCmd::ReviewerShowAnswer {
            state_token: "   ".into(),
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }
}

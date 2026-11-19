use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{call_plan, make_ref, make_sort, parse_yaml_or_json_object};
use super::RpcPlan;

const VALID_NAVIGATE_VIEWS: [&str; 3] = ["deckBrowser", "deckOverview", "review"];
const VALID_DRAFT_MODES: [&str; 2] = ["replace", "append"];

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
    }
}

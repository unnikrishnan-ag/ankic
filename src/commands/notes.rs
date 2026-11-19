use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{
    call_plan, make_ref, make_sort, parse_yaml_or_json_array, parse_yaml_or_json_object,
    read_input_or_stdin,
};
use super::RpcPlan;

#[derive(Subcommand)]
pub enum NotesCmd {
    /// Get a note by ID
    Get { id: String },
    /// Get multiple notes by ID
    #[command(name = "get-many")]
    GetMany {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Search notes using Anki query syntax
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
    /// Find empty notes by criterion (allFieldsEmpty, noCards, allQuestionsEmpty)
    #[command(name = "find-empty")]
    FindEmpty {
        #[arg(long, default_value = "allFieldsEmpty")]
        criterion: String,
        /// Optional Anki search query to filter candidate notes
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: u64,
        #[arg(long)]
        cursor: Option<String>,
        /// Auto-paginate through all matching IDs
        #[arg(long)]
        all: bool,
    },
    /// Create a note
    Create {
        #[arg(long)]
        deck: String,
        #[arg(long)]
        note_type: String,
        /// YAML or JSON object of field name -> HTML value
        #[arg(long)]
        fields: String,
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Validate a note without creating it
    Validate {
        #[arg(long, conflicts_with = "deck_id")]
        deck: Option<String>,
        #[arg(long, conflicts_with = "deck")]
        deck_id: Option<String>,
        #[arg(long, conflicts_with = "note_type_id")]
        note_type: Option<String>,
        #[arg(long, conflicts_with = "note_type")]
        note_type_id: Option<String>,
        /// YAML or JSON object of field name -> HTML value
        #[arg(long)]
        fields: String,
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Allow duplicate notes
        #[arg(long)]
        allow_duplicates: bool,
        /// Reject notes whose generated questions are empty (true/false)
        #[arg(long)]
        reject_empty_questions: Option<bool>,
    },
    /// Validate multiple notes without creating them
    #[command(name = "validate-many")]
    ValidateMany {
        /// YAML or JSON array of NoteInput objects
        #[arg(long, alias = "items", conflicts_with = "stdin")]
        notes: Option<String>,
        /// Read YAML or JSON array of NoteInput objects from stdin
        #[arg(long, conflicts_with = "notes")]
        stdin: bool,
    },
    /// Update note fields and/or tags
    Update {
        id: String,
        /// YAML or JSON object of fields to merge
        #[arg(long)]
        fields: Option<String>,
        /// Replace all tags with these values (repeatable)
        #[arg(long = "tag")]
        tags: Option<Vec<String>>,
    },
    /// Delete one or more notes by ID
    Delete {
        #[arg(required = true)]
        ids: Vec<String>,
    },
}

/// Translate a `notes` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &NotesCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        NotesCmd::Get { id } => Ok(call_plan("notes.get", json!({ "id": id }))),
        NotesCmd::GetMany { ids } => Ok(call_plan("notes.getMany", json!({ "ids": ids }))),
        NotesCmd::Search {
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
                    method: "notes.search".into(),
                    params,
                    array_key: "ids",
                })
            } else {
                Ok(call_plan("notes.search", Value::Object(params)))
            }
        }
        NotesCmd::FindEmpty {
            criterion,
            query,
            limit,
            cursor,
            all,
        } => {
            if !matches!(
                criterion.as_str(),
                "allFieldsEmpty" | "noCards" | "allQuestionsEmpty"
            ) {
                return Err(CliError::InvalidInput(format!(
                    "Invalid criterion '{criterion}': expected 'allFieldsEmpty', 'noCards', or 'allQuestionsEmpty'"
                )));
            }
            let mut params = Map::new();
            params.insert("criterion".into(), json!(criterion));
            if let Some(q) = query {
                params.insert("query".into(), json!(q));
            }
            params.insert("limit".into(), json!(limit));
            if let Some(c) = cursor {
                params.insert("cursor".into(), json!(c));
            }
            if *all {
                Ok(RpcPlan::Paginate {
                    method: "notes.findEmpty".into(),
                    params,
                    array_key: "ids",
                })
            } else {
                Ok(call_plan("notes.findEmpty", Value::Object(params)))
            }
        }
        NotesCmd::Create {
            deck,
            note_type,
            fields,
            tags,
        } => {
            let fields_obj = parse_yaml_or_json_object(fields, "--fields")?;
            Ok(call_plan(
                "notes.create",
                json!({
                    "deck": { "name": deck },
                    "noteType": { "name": note_type },
                    "fields": Value::Object(fields_obj),
                    "tags": tags,
                }),
            ))
        }
        NotesCmd::Validate {
            deck,
            deck_id,
            note_type,
            note_type_id,
            fields,
            tags,
            allow_duplicates,
            reject_empty_questions,
        } => {
            let deck_ref = make_ref(deck.as_deref(), deck_id.as_deref(), "deck")?;
            let note_type_ref =
                make_ref(note_type.as_deref(), note_type_id.as_deref(), "noteType")?;
            let fields_obj = parse_yaml_or_json_object(fields, "--fields")?;

            let mut note = Map::new();
            note.insert("deck".into(), deck_ref);
            note.insert("noteType".into(), note_type_ref);
            note.insert("fields".into(), Value::Object(fields_obj));
            if !tags.is_empty() {
                note.insert("tags".into(), json!(tags));
            }
            if *allow_duplicates {
                note.insert("duplicates".into(), json!({ "allow": true }));
            }
            if let Some(reject) = reject_empty_questions {
                note.insert("rejectEmptyQuestions".into(), json!(reject));
            }

            Ok(call_plan("notes.validate", json!({ "note": note })))
        }
        NotesCmd::ValidateMany { notes, stdin } => {
            let raw = read_input_or_stdin(notes.as_deref(), *stdin, "--notes")?;
            let notes_arr = parse_yaml_or_json_array(&raw, "--notes")?;
            Ok(call_plan(
                "notes.validateMany",
                json!({ "notes": notes_arr }),
            ))
        }
        NotesCmd::Update { id, fields, tags } => {
            let mut patch = Map::new();
            if let Some(f) = fields {
                let fields_obj = parse_yaml_or_json_object(f, "--fields")?;
                patch.insert("fields".into(), Value::Object(fields_obj));
            }
            if let Some(t) = tags {
                patch.insert("tags".into(), json!(t));
            }
            if patch.is_empty() {
                return Err(CliError::InvalidInput(
                    "Provide at least one of --fields or --tag".into(),
                ));
            }
            Ok(call_plan(
                "notes.update",
                json!({ "id": id, "patch": patch }),
            ))
        }
        NotesCmd::Delete { ids } => Ok(call_plan("notes.delete", json!({ "ids": ids }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_valid_call(cmd: NotesCmd, expected_method: &str) -> Value {
        match to_rpc_plan(&cmd).expect("build notes rpc plan") {
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
                    .unwrap_or_else(|e| panic!("schema validation failed for {method}: {e}"));
                params
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn notes_get_and_get_many_produce_valid_plans() {
        let p = assert_valid_call(NotesCmd::Get { id: "1001".into() }, "notes.get");
        assert_eq!(p, json!({ "id": "1001" }));

        let p_many = assert_valid_call(
            NotesCmd::GetMany {
                ids: vec!["1001".into(), "1002".into()],
            },
            "notes.getMany",
        );
        assert_eq!(p_many, json!({ "ids": ["1001", "1002"] }));
    }

    #[test]
    fn notes_search_supports_sort_cursor_and_pagination() {
        let p = assert_valid_call(
            NotesCmd::Search {
                query: "deck:Default".into(),
                limit: 50,
                cursor: Some("cur-1".into()),
                sort_by: Some("noteFld".into()),
                sort_dir: Some("descending".into()),
                all: false,
            },
            "notes.search",
        );
        assert_eq!(
            p,
            json!({
                "query": "deck:Default",
                "limit": 50,
                "cursor": "cur-1",
                "sort": { "column": "noteFld", "direction": "descending" }
            })
        );

        let plan = to_rpc_plan(&NotesCmd::Search {
            query: "".into(),
            limit: 100,
            cursor: None,
            sort_by: None,
            sort_dir: None,
            all: true,
        })
        .expect("paginate search");
        match plan {
            RpcPlan::Paginate {
                method,
                params,
                array_key,
            } => {
                assert_eq!(method, "notes.search");
                assert_eq!(array_key, "ids");
                schema::validate_method_params(&method, &Value::Object(params))
                    .expect("valid notes.search paginate params");
            }
            other => panic!("expected RpcPlan::Paginate, got {other:?}"),
        }
    }

    #[test]
    fn notes_find_empty_supports_all_criteria_and_pagination() {
        for criterion in ["allFieldsEmpty", "noCards", "allQuestionsEmpty"] {
            let p = assert_valid_call(
                NotesCmd::FindEmpty {
                    criterion: criterion.into(),
                    query: Some("deck:Default".into()),
                    limit: 25,
                    cursor: None,
                    all: false,
                },
                "notes.findEmpty",
            );
            assert_eq!(
                p,
                json!({
                    "criterion": criterion,
                    "query": "deck:Default",
                    "limit": 25
                })
            );
        }

        let plan = to_rpc_plan(&NotesCmd::FindEmpty {
            criterion: "noCards".into(),
            query: None,
            limit: 100,
            cursor: None,
            all: true,
        })
        .expect("paginate findEmpty");
        match plan {
            RpcPlan::Paginate {
                method,
                params,
                array_key,
            } => {
                assert_eq!(method, "notes.findEmpty");
                assert_eq!(array_key, "ids");
                schema::validate_method_params(&method, &Value::Object(params))
                    .expect("valid notes.findEmpty paginate params");
            }
            other => panic!("expected RpcPlan::Paginate, got {other:?}"),
        }

        let err = to_rpc_plan(&NotesCmd::FindEmpty {
            criterion: "bogus".into(),
            query: None,
            limit: 100,
            cursor: None,
            all: false,
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn notes_validate_wraps_note_and_supports_refs_and_flags() {
        let p = assert_valid_call(
            NotesCmd::Validate {
                deck: Some("Default".into()),
                deck_id: None,
                note_type: None,
                note_type_id: Some("12345".into()),
                fields: "Front: Q\nBack: A".into(),
                tags: vec!["vocab".into()],
                allow_duplicates: true,
                reject_empty_questions: Some(false),
            },
            "notes.validate",
        );
        assert_eq!(
            p,
            json!({
                "note": {
                    "deck": { "name": "Default" },
                    "noteType": { "id": "12345" },
                    "fields": { "Front": "Q", "Back": "A" },
                    "tags": ["vocab"],
                    "duplicates": { "allow": true },
                    "rejectEmptyQuestions": false
                }
            })
        );
    }

    #[test]
    fn notes_validate_many_parses_yaml_and_json_arrays() {
        let yaml_notes = r#"
- deck: { name: Default }
  noteType: { name: Basic }
  fields: { Front: Q1, Back: A1 }
- deck: { id: "1" }
  noteType: { id: "2" }
  fields: { Front: Q2, Back: A2 }
  tags: [tag1]
"#;
        let p = assert_valid_call(
            NotesCmd::ValidateMany {
                notes: Some(yaml_notes.into()),
                stdin: false,
            },
            "notes.validateMany",
        );
        assert_eq!(p["notes"].as_array().unwrap().len(), 2);

        let err = to_rpc_plan(&NotesCmd::ValidateMany {
            notes: Some("deck: Default".into()),
            stdin: false,
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}

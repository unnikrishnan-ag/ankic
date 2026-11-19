use clap::Subcommand;
use serde_json::json;

use crate::error::CliError;

use super::helpers::call_plan;
use super::RpcPlan;

#[derive(Subcommand)]
pub enum TagsCmd {
    /// List all tags in the collection
    List,
    /// Add one or more tags to one or more notes
    Add {
        /// Target note ID (repeatable)
        #[arg(long = "note-id", alias = "note-ids", value_name = "ID")]
        note_ids: Vec<String>,
        /// Tag to add (repeatable)
        #[arg(long = "tag", alias = "tags", value_name = "TAG")]
        tags: Vec<String>,
    },
    /// Remove one or more tags from one or more notes
    Remove {
        /// Target note ID (repeatable)
        #[arg(long = "note-id", alias = "note-ids", value_name = "ID")]
        note_ids: Vec<String>,
        /// Tag to remove (repeatable)
        #[arg(long = "tag", alias = "tags", value_name = "TAG")]
        tags: Vec<String>,
    },
    /// Replace a tag across specific notes or all notes
    Replace {
        /// Existing tag to replace
        #[arg(long)]
        from: String,
        /// Replacement tag
        #[arg(long)]
        to: String,
        /// Target note ID (repeatable)
        #[arg(long = "note-id", alias = "note-ids", value_name = "ID")]
        note_ids: Vec<String>,
        /// Replace across all notes in the collection
        #[arg(long, conflicts_with = "note_ids")]
        all_notes: bool,
    },
    /// Delete unused tags from the collection
    #[command(name = "delete-unused")]
    DeleteUnused,
}

fn validate_note_ids_and_tags(note_ids: &[String], tags: &[String]) -> Result<(), CliError> {
    if note_ids.is_empty() {
        return Err(CliError::InvalidInput(
            "Specify at least one --note-id".into(),
        ));
    }
    if tags.is_empty() {
        return Err(CliError::InvalidInput("Specify at least one --tag".into()));
    }
    Ok(())
}

/// Translate a `tags` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &TagsCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        TagsCmd::List => Ok(call_plan("tags.list", json!({}))),
        TagsCmd::Add { note_ids, tags } => {
            validate_note_ids_and_tags(note_ids, tags)?;
            Ok(call_plan(
                "tags.add",
                json!({ "noteIds": note_ids, "tags": tags }),
            ))
        }
        TagsCmd::Remove { note_ids, tags } => {
            validate_note_ids_and_tags(note_ids, tags)?;
            Ok(call_plan(
                "tags.remove",
                json!({ "noteIds": note_ids, "tags": tags }),
            ))
        }
        TagsCmd::Replace {
            from,
            to,
            note_ids,
            all_notes,
        } => match (*all_notes, note_ids.is_empty()) {
            (true, true) => Ok(call_plan(
                "tags.replace",
                json!({ "from": from, "to": to, "allNotes": true }),
            )),
            (false, false) => Ok(call_plan(
                "tags.replace",
                json!({ "from": from, "to": to, "noteIds": note_ids }),
            )),
            _ => Err(CliError::InvalidInput(
                "Specify either --note-id(s) or --all-notes (mutually exclusive)".into(),
            )),
        },
        TagsCmd::DeleteUnused => Ok(call_plan("tags.deleteUnused", json!({}))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use serde_json::Value;

    fn assert_call_valid(cmd: TagsCmd, expected_method: &str, expected_params: Value) {
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
    fn list_and_delete_unused_validate_against_schema() {
        assert_call_valid(TagsCmd::List, "tags.list", json!({}));
        assert_call_valid(TagsCmd::DeleteUnused, "tags.deleteUnused", json!({}));
    }

    #[test]
    fn add_and_remove_validate_against_schema_and_require_non_empty() {
        assert_call_valid(
            TagsCmd::Add {
                note_ids: vec!["101".into(), "102".into()],
                tags: vec!["vocab".into(), "jlpt-n3".into()],
            },
            "tags.add",
            json!({
                "noteIds": ["101", "102"],
                "tags": ["vocab", "jlpt-n3"]
            }),
        );

        assert_call_valid(
            TagsCmd::Remove {
                note_ids: vec!["101".into()],
                tags: vec!["old-tag".into()],
            },
            "tags.remove",
            json!({
                "noteIds": ["101"],
                "tags": ["old-tag"]
            }),
        );

        let err_no_notes = to_rpc_plan(&TagsCmd::Add {
            note_ids: vec![],
            tags: vec!["vocab".into()],
        })
        .unwrap_err();
        assert_eq!(err_no_notes.exit_code(), 2);

        let err_no_tags = to_rpc_plan(&TagsCmd::Remove {
            note_ids: vec!["101".into()],
            tags: vec![],
        })
        .unwrap_err();
        assert_eq!(err_no_tags.exit_code(), 2);
    }

    #[test]
    fn replace_supports_note_ids_and_all_notes_and_enforces_one_of() {
        assert_call_valid(
            TagsCmd::Replace {
                from: "old".into(),
                to: "new".into(),
                note_ids: vec!["101".into(), "102".into()],
                all_notes: false,
            },
            "tags.replace",
            json!({
                "from": "old",
                "to": "new",
                "noteIds": ["101", "102"]
            }),
        );

        assert_call_valid(
            TagsCmd::Replace {
                from: "old".into(),
                to: "new".into(),
                note_ids: vec![],
                all_notes: true,
            },
            "tags.replace",
            json!({
                "from": "old",
                "to": "new",
                "allNotes": true
            }),
        );

        let err_neither = to_rpc_plan(&TagsCmd::Replace {
            from: "old".into(),
            to: "new".into(),
            note_ids: vec![],
            all_notes: false,
        })
        .unwrap_err();
        assert_eq!(err_neither.exit_code(), 2);

        let err_both = to_rpc_plan(&TagsCmd::Replace {
            from: "old".into(),
            to: "new".into(),
            note_ids: vec!["101".into()],
            all_notes: true,
        })
        .unwrap_err();
        assert_eq!(err_both.exit_code(), 2);
    }
}

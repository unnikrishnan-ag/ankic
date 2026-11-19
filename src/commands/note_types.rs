use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::{
    call_plan, make_ref, make_refs, parse_yaml_or_json_array, parse_yaml_or_json_object,
    read_input_or_stdin,
};
use super::RpcPlan;

#[derive(Subcommand)]
pub enum NoteTypesCmd {
    /// List all note types in the collection
    List,
    /// Get full configuration of a note type by name or ID
    Get {
        /// Note type name
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        /// Note type ID
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
    },
    /// Create a new note type from flags or a YAML/JSON spec
    Create {
        /// Note type name
        #[arg(long)]
        name: Option<String>,
        /// Note type kind (normal or cloze; defaults to normal)
        #[arg(long)]
        kind: Option<String>,
        /// YAML or JSON array of FieldConfig objects or field name strings
        #[arg(long)]
        fields: Option<String>,
        /// YAML or JSON array of TemplateConfig objects
        #[arg(long)]
        templates: Option<String>,
        /// Shared card CSS
        #[arg(long)]
        css: Option<String>,
        /// Sort field name
        #[arg(long)]
        sort_field: Option<String>,
        /// LaTeX header
        #[arg(long)]
        latex_header: Option<String>,
        /// LaTeX footer
        #[arg(long)]
        latex_footer: Option<String>,
        /// Render LaTeX as SVG
        #[arg(long)]
        latex_svg: Option<bool>,
        /// Full YAML or JSON noteTypes.create parameter object
        #[arg(long, conflicts_with = "stdin")]
        spec: Option<String>,
        /// Read full YAML or JSON noteTypes.create parameter object from stdin
        #[arg(long, conflicts_with = "spec")]
        stdin: bool,
    },
    /// Update note type properties
    Update {
        /// Target note type name
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        /// Target note type ID
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
        /// New note type name
        #[arg(long)]
        new_name: Option<String>,
        /// Updated shared card CSS
        #[arg(long)]
        css: Option<String>,
        /// Updated sort field name
        #[arg(long)]
        sort_field: Option<String>,
        /// Updated LaTeX header
        #[arg(long)]
        latex_header: Option<String>,
        /// Updated LaTeX footer
        #[arg(long)]
        latex_footer: Option<String>,
        /// Updated LaTeX SVG flag
        #[arg(long)]
        latex_svg: Option<bool>,
        /// YAML or JSON NoteTypePatch object
        #[arg(long)]
        patch: Option<String>,
    },
    /// Inspect card template usage across notes/cards for a note type
    #[command(name = "template-usage")]
    TemplateUsage {
        /// Note type name
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        /// Note type ID
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
    },
    /// Apply structural edits (fields/templates) to a note type
    #[command(name = "edit-structure")]
    EditStructure {
        /// Note type name
        #[arg(long, conflicts_with = "id")]
        name: Option<String>,
        /// Note type ID
        #[arg(long, conflicts_with = "name")]
        id: Option<String>,
        /// YAML or JSON array of StructureEdit objects
        #[arg(long, conflicts_with = "stdin")]
        edits: Option<String>,
        /// Read YAML or JSON array of StructureEdit objects from stdin
        #[arg(long, conflicts_with = "edits")]
        stdin: bool,
    },
    /// Find and replace text across note type templates and CSS
    #[command(name = "replace-text")]
    ReplaceText {
        /// Text to find (non-empty)
        #[arg(long)]
        find: String,
        /// Replacement text
        #[arg(long)]
        replace: String,
        /// Target note type name (repeatable)
        #[arg(long = "name", alias = "names", value_name = "NAME", conflicts_with = "all_note_types")]
        names: Vec<String>,
        /// Target note type ID (repeatable)
        #[arg(long = "id", alias = "ids", value_name = "ID", conflicts_with = "all_note_types")]
        ids: Vec<String>,
        /// Replace across all note types in the collection
        #[arg(long, conflicts_with_all = ["names", "ids"])]
        all_note_types: bool,
        /// Target sections to update: question, answer, css (repeatable)
        #[arg(long = "target", alias = "targets", value_name = "TARGET")]
        targets: Vec<String>,
    },
    /// Delete unused note types by name or ID
    #[command(name = "delete-unused")]
    DeleteUnused {
        /// Note type name to delete if unused (repeatable)
        #[arg(long = "name", alias = "names", value_name = "NAME")]
        names: Vec<String>,
        /// Note type ID to delete if unused (repeatable)
        #[arg(long = "id", alias = "ids", value_name = "ID")]
        ids: Vec<String>,
    },
}

fn normalize_fields(raw: &str) -> Result<Vec<Value>, CliError> {
    let items = parse_yaml_or_json_array(raw, "--fields")?;
    if items.is_empty() {
        return Err(CliError::InvalidInput(
            "--fields must contain at least one field".into(),
        ));
    }
    items
        .into_iter()
        .map(|item| match item {
            Value::String(name) => {
                if name.trim().is_empty() {
                    return Err(CliError::InvalidInput(
                        "Field name in --fields must not be empty".into(),
                    ));
                }
                Ok(json!({ "name": name }))
            }
            obj @ Value::Object(_) => Ok(obj),
            _ => Err(CliError::InvalidInput(
                "Each item in --fields must be a field name string or FieldConfig object".into(),
            )),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_create_params(
    name: &Option<String>,
    kind: &Option<String>,
    fields: &Option<String>,
    templates: &Option<String>,
    css: &Option<String>,
    sort_field: &Option<String>,
    latex_header: &Option<String>,
    latex_footer: &Option<String>,
    latex_svg: Option<bool>,
    spec: &Option<String>,
    stdin: bool,
) -> Result<Value, CliError> {
    if spec.is_some() || stdin {
        let has_individual_flags = name.is_some()
            || kind.is_some()
            || fields.is_some()
            || templates.is_some()
            || css.is_some()
            || sort_field.is_some()
            || latex_header.is_some()
            || latex_footer.is_some()
            || latex_svg.is_some();
        if has_individual_flags {
            return Err(CliError::InvalidInput(
                "Cannot combine --spec/--stdin with individual note-type flags".into(),
            ));
        }
        let raw = read_input_or_stdin(spec.as_deref(), stdin, "--spec")?;
        return Ok(Value::Object(parse_yaml_or_json_object(&raw, "--spec")?));
    }

    let note_type_name = name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            CliError::InvalidInput("--name is required (or pass --spec/--stdin)".into())
        })?;
    let fields_raw = fields.as_deref().ok_or_else(|| {
        CliError::InvalidInput("--fields is required (or pass --spec/--stdin)".into())
    })?;
    let templates_raw = templates.as_deref().ok_or_else(|| {
        CliError::InvalidInput("--templates is required (or pass --spec/--stdin)".into())
    })?;

    let kind_val = match kind.as_deref().unwrap_or("normal") {
        "normal" => "normal",
        "cloze" => "cloze",
        other => {
            return Err(CliError::InvalidInput(format!(
                "Invalid --kind '{other}': expected 'normal' or 'cloze'"
            )));
        }
    };

    let parsed_fields = normalize_fields(fields_raw)?;
    let parsed_templates = parse_yaml_or_json_array(templates_raw, "--templates")?;
    if parsed_templates.is_empty() {
        return Err(CliError::InvalidInput(
            "--templates must contain at least one template".into(),
        ));
    }

    let mut params = Map::new();
    params.insert("name".into(), json!(note_type_name));
    params.insert("kind".into(), json!(kind_val));
    params.insert("fields".into(), Value::Array(parsed_fields));
    params.insert("templates".into(), Value::Array(parsed_templates));

    if let Some(css) = css {
        params.insert("css".into(), json!(css));
    }
    if let Some(sort_field) = sort_field {
        params.insert("sortField".into(), json!(sort_field));
    }
    if let Some(latex_header) = latex_header {
        params.insert("latexHeader".into(), json!(latex_header));
    }
    if let Some(latex_footer) = latex_footer {
        params.insert("latexFooter".into(), json!(latex_footer));
    }
    if let Some(latex_svg) = latex_svg {
        params.insert("latexSvg".into(), json!(latex_svg));
    }

    Ok(Value::Object(params))
}

#[allow(clippy::too_many_arguments)]
fn build_patch(
    new_name: &Option<String>,
    css: &Option<String>,
    sort_field: &Option<String>,
    latex_header: &Option<String>,
    latex_footer: &Option<String>,
    latex_svg: Option<bool>,
    patch: &Option<String>,
) -> Result<Value, CliError> {
    let mut map = match patch.as_deref() {
        Some(raw) => parse_yaml_or_json_object(raw, "--patch")?,
        None => Map::new(),
    };

    if let Some(new_name) = new_name {
        map.insert("name".into(), json!(new_name));
    }
    if let Some(css) = css {
        map.insert("css".into(), json!(css));
    }
    if let Some(sort_field) = sort_field {
        map.insert("sortField".into(), json!(sort_field));
    }
    if let Some(latex_header) = latex_header {
        map.insert("latexHeader".into(), json!(latex_header));
    }
    if let Some(latex_footer) = latex_footer {
        map.insert("latexFooter".into(), json!(latex_footer));
    }
    if let Some(latex_svg) = latex_svg {
        map.insert("latexSvg".into(), json!(latex_svg));
    }

    if map.is_empty() {
        return Err(CliError::InvalidInput(
            "Specify at least one field to update via --patch or update flags".into(),
        ));
    }

    Ok(Value::Object(map))
}

/// Translate a `note-types` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &NoteTypesCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        NoteTypesCmd::List => Ok(call_plan("noteTypes.list", json!({}))),
        NoteTypesCmd::Get { name, id } => Ok(call_plan(
            "noteTypes.get",
            json!({ "noteType": make_ref(name.as_deref(), id.as_deref(), "noteType")? }),
        )),
        NoteTypesCmd::Create {
            name,
            kind,
            fields,
            templates,
            css,
            sort_field,
            latex_header,
            latex_footer,
            latex_svg,
            spec,
            stdin,
        } => {
            let params = build_create_params(
                name,
                kind,
                fields,
                templates,
                css,
                sort_field,
                latex_header,
                latex_footer,
                *latex_svg,
                spec,
                *stdin,
            )?;
            Ok(call_plan("noteTypes.create", params))
        }
        NoteTypesCmd::Update {
            name,
            id,
            new_name,
            css,
            sort_field,
            latex_header,
            latex_footer,
            latex_svg,
            patch,
        } => {
            let note_type = make_ref(name.as_deref(), id.as_deref(), "noteType")?;
            let patch_obj = build_patch(
                new_name,
                css,
                sort_field,
                latex_header,
                latex_footer,
                *latex_svg,
                patch,
            )?;
            Ok(call_plan(
                "noteTypes.update",
                json!({
                    "noteType": note_type,
                    "patch": patch_obj,
                }),
            ))
        }
        NoteTypesCmd::TemplateUsage { name, id } => Ok(call_plan(
            "noteTypes.getTemplateUsage",
            json!({ "noteType": make_ref(name.as_deref(), id.as_deref(), "noteType")? }),
        )),
        NoteTypesCmd::EditStructure {
            name,
            id,
            edits,
            stdin,
        } => {
            let note_type = make_ref(name.as_deref(), id.as_deref(), "noteType")?;
            let raw = read_input_or_stdin(edits.as_deref(), *stdin, "--edits")?;
            let edits_array = parse_yaml_or_json_array(&raw, "--edits")?;
            if edits_array.is_empty() {
                return Err(CliError::InvalidInput(
                    "--edits must contain at least one edit".into(),
                ));
            }
            Ok(call_plan(
                "noteTypes.editStructure",
                json!({
                    "noteType": note_type,
                    "edits": edits_array,
                }),
            ))
        }
        NoteTypesCmd::ReplaceText {
            find,
            replace,
            names,
            ids,
            all_note_types,
            targets,
        } => {
            if find.is_empty() {
                return Err(CliError::InvalidInput(
                    "--find must not be empty".into(),
                ));
            }
            let mut params = Map::new();
            params.insert("find".into(), json!(find));
            params.insert("replace".into(), json!(replace));

            let has_refs = !names.is_empty() || !ids.is_empty();
            match (*all_note_types, has_refs) {
                (true, false) => {
                    params.insert("allNoteTypes".into(), json!(true));
                }
                (false, true) => {
                    params.insert(
                        "noteTypes".into(),
                        Value::Array(make_refs(names, ids, "noteType")?),
                    );
                }
                _ => {
                    return Err(CliError::InvalidInput(
                        "Specify either --name/--id or --all-note-types (mutually exclusive)"
                            .into(),
                    ));
                }
            }

            if !targets.is_empty() {
                let mut seen = Vec::with_capacity(targets.len());
                for target in targets {
                    match target.as_str() {
                        "question" | "answer" | "css" => {
                            if seen.contains(target) {
                                return Err(CliError::InvalidInput(format!(
                                    "Duplicate --target '{target}'"
                                )));
                            }
                            seen.push(target.clone());
                        }
                        other => {
                            return Err(CliError::InvalidInput(format!(
                                "Invalid --target '{other}': expected 'question', 'answer', or 'css'"
                            )));
                        }
                    }
                }
                params.insert("targets".into(), json!(targets));
            }

            Ok(call_plan("noteTypes.replaceText", Value::Object(params)))
        }
        NoteTypesCmd::DeleteUnused { names, ids } => Ok(call_plan(
            "noteTypes.deleteUnused",
            json!({ "noteTypes": make_refs(names, ids, "noteType")? }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_call_valid(cmd: NoteTypesCmd, expected_method: &str, expected_params: Value) {
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
    fn list_get_and_template_usage_validate_against_schema() {
        assert_call_valid(NoteTypesCmd::List, "noteTypes.list", json!({}));

        assert_call_valid(
            NoteTypesCmd::Get {
                name: Some("Basic".into()),
                id: None,
            },
            "noteTypes.get",
            json!({ "noteType": { "name": "Basic" } }),
        );

        assert_call_valid(
            NoteTypesCmd::TemplateUsage {
                name: None,
                id: Some("1700000000000".into()),
            },
            "noteTypes.getTemplateUsage",
            json!({ "noteType": { "id": "1700000000000" } }),
        );
    }

    #[test]
    fn create_supports_shorthand_fields_flags_and_spec() {
        assert_call_valid(
            NoteTypesCmd::Create {
                name: Some("Vocab".into()),
                kind: Some("normal".into()),
                fields: Some("[\"Word\", {\"name\": \"Meaning\", \"fontSize\": 24}]".into()),
                templates: Some(
                    "[{\"name\": \"Card 1\", \"questionHtml\": \"{{Word}}\", \"answerHtml\": \"{{FrontSide}}<hr id=answer>{{Meaning}}\"}]".into(),
                ),
                css: Some(".card { font-size: 20px; }".into()),
                sort_field: Some("Word".into()),
                latex_header: None,
                latex_footer: None,
                latex_svg: Some(true),
                spec: None,
                stdin: false,
            },
            "noteTypes.create",
            json!({
                "name": "Vocab",
                "kind": "normal",
                "fields": [
                    { "name": "Word" },
                    { "name": "Meaning", "fontSize": 24 }
                ],
                "templates": [
                    {
                        "name": "Card 1",
                        "questionHtml": "{{Word}}",
                        "answerHtml": "{{FrontSide}}<hr id=answer>{{Meaning}}"
                    }
                ],
                "css": ".card { font-size: 20px; }",
                "sortField": "Word",
                "latexSvg": true
            }),
        );

        assert_call_valid(
            NoteTypesCmd::Create {
                name: None,
                kind: None,
                fields: None,
                templates: None,
                css: None,
                sort_field: None,
                latex_header: None,
                latex_footer: None,
                latex_svg: None,
                spec: Some(
                    "name: ClozeCustom\nkind: cloze\nfields:\n  - name: Text\n  - name: Extra\ntemplates:\n  - name: Cloze\n    questionHtml: '{{cloze:Text}}'\n    answerHtml: '{{cloze:Text}}<br>{{Extra}}'\n".into(),
                ),
                stdin: false,
            },
            "noteTypes.create",
            json!({
                "name": "ClozeCustom",
                "kind": "cloze",
                "fields": [
                    { "name": "Text" },
                    { "name": "Extra" }
                ],
                "templates": [
                    {
                        "name": "Cloze",
                        "questionHtml": "{{cloze:Text}}",
                        "answerHtml": "{{cloze:Text}}<br>{{Extra}}"
                    }
                ]
            }),
        );

        let err = to_rpc_plan(&NoteTypesCmd::Create {
            name: Some("Conflict".into()),
            kind: None,
            fields: None,
            templates: None,
            css: None,
            sort_field: None,
            latex_header: None,
            latex_footer: None,
            latex_svg: None,
            spec: Some("{}".into()),
            stdin: false,
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn update_supports_patch_and_flags_and_rejects_empty_patch() {
        assert_call_valid(
            NoteTypesCmd::Update {
                name: Some("Basic".into()),
                id: None,
                new_name: Some("Basic Updated".into()),
                css: None,
                sort_field: Some("Back".into()),
                latex_header: None,
                latex_footer: None,
                latex_svg: Some(false),
                patch: Some("css: '.card { color: black; }'".into()),
            },
            "noteTypes.update",
            json!({
                "noteType": { "name": "Basic" },
                "patch": {
                    "name": "Basic Updated",
                    "css": ".card { color: black; }",
                    "sortField": "Back",
                    "latexSvg": false
                }
            }),
        );

        let err = to_rpc_plan(&NoteTypesCmd::Update {
            name: Some("Basic".into()),
            id: None,
            new_name: None,
            css: None,
            sort_field: None,
            latex_header: None,
            latex_footer: None,
            latex_svg: None,
            patch: None,
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn edit_structure_validates_against_schema_and_rejects_empty_edits() {
        assert_call_valid(
            NoteTypesCmd::EditStructure {
                name: Some("Basic".into()),
                id: None,
                edits: Some(
                    "- op: addField\n  field:\n    name: Hint\n- op: moveField\n  name: Hint\n  index: 1\n".into(),
                ),
                stdin: false,
            },
            "noteTypes.editStructure",
            json!({
                "noteType": { "name": "Basic" },
                "edits": [
                    { "op": "addField", "field": { "name": "Hint" } },
                    { "op": "moveField", "name": "Hint", "index": 1 }
                ]
            }),
        );

        let err = to_rpc_plan(&NoteTypesCmd::EditStructure {
            name: Some("Basic".into()),
            id: None,
            edits: Some("[]".into()),
            stdin: false,
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn replace_text_handles_both_modes_targets_and_validation() {
        assert_call_valid(
            NoteTypesCmd::ReplaceText {
                find: "old-class".into(),
                replace: "new-class".into(),
                names: vec!["Basic".into()],
                ids: vec!["1700000000000".into()],
                all_note_types: false,
                targets: vec!["question".into(), "css".into()],
            },
            "noteTypes.replaceText",
            json!({
                "find": "old-class",
                "replace": "new-class",
                "noteTypes": [
                    { "name": "Basic" },
                    { "id": "1700000000000" }
                ],
                "targets": ["question", "css"]
            }),
        );

        assert_call_valid(
            NoteTypesCmd::ReplaceText {
                find: "foo".into(),
                replace: "".into(),
                names: vec![],
                ids: vec![],
                all_note_types: true,
                targets: vec![],
            },
            "noteTypes.replaceText",
            json!({
                "find": "foo",
                "replace": "",
                "allNoteTypes": true
            }),
        );

        let missing_scope = to_rpc_plan(&NoteTypesCmd::ReplaceText {
            find: "foo".into(),
            replace: "bar".into(),
            names: vec![],
            ids: vec![],
            all_note_types: false,
            targets: vec![],
        })
        .unwrap_err();
        assert_eq!(missing_scope.exit_code(), 2);

        let both_scopes = to_rpc_plan(&NoteTypesCmd::ReplaceText {
            find: "foo".into(),
            replace: "bar".into(),
            names: vec!["Basic".into()],
            ids: vec![],
            all_note_types: true,
            targets: vec![],
        })
        .unwrap_err();
        assert_eq!(both_scopes.exit_code(), 2);
    }

    #[test]
    fn delete_unused_validates_against_schema() {
        assert_call_valid(
            NoteTypesCmd::DeleteUnused {
                names: vec!["Unused A".into()],
                ids: vec!["1700000000001".into()],
            },
            "noteTypes.deleteUnused",
            json!({
                "noteTypes": [
                    { "name": "Unused A" },
                    { "id": "1700000000001" }
                ]
            }),
        );
    }
}

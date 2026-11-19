use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{json, Map, Value};

use crate::client::AnkiClient;
use crate::error::CliError;
use crate::output::OutputFormat;
use crate::schema;

pub mod collection;
pub mod decks;
pub mod helpers;
pub mod operations;

pub use collection::CollectionCmd;
pub use decks::DecksCmd;
pub use operations::OperationsCmd;

#[derive(Parser)]
#[command(
    name = "anki-cli",
    version,
    about = "Anki-Connect API v1 CLI for complete API coverage and LLM agent workflows"
)]
pub struct Cli {
    /// Anki-Connect v1 JSON-RPC endpoint
    #[arg(
        long,
        default_value = "http://127.0.0.1:8765/api/v1",
        env = "ANKI_CONNECT_URL",
        global = true
    )]
    pub url: String,

    /// Optional API key (sent as Authorization: Bearer <key>)
    #[arg(long, env = "ANKI_CONNECT_API_KEY", global = true)]
    pub api_key: Option<String>,

    /// Optional collection session ID from collection.get
    #[arg(long, env = "ANKI_CONNECT_SESSION", global = true)]
    pub session: Option<String>,

    /// Output format (yaml, json, json-pretty)
    #[arg(long, short = 'f', value_enum, global = true)]
    pub format: Option<OutputFormat>,

    /// Convenience flag for --format json
    #[arg(long, global = true)]
    pub json: bool,

    /// Convenience flag for --format json-pretty
    #[arg(long, global = true)]
    pub pretty: bool,

    /// Bypass client-side schema validation
    #[arg(long, global = true)]
    pub no_validate: bool,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    /// Resolve the effective output format from `--format`, `--json`, and `--pretty`.
    pub fn output_format(&self) -> OutputFormat {
        OutputFormat::resolve(self.format, self.json, self.pretty)
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Call any Anki-Connect v1 JSON-RPC method directly
    Call(CallArgs),

    /// Inspect the embedded API v1 schema offline (methods, params, results, errors)
    Schema(SchemaArgs),

    /// Query live runtime capabilities via api.describe
    Describe,

    /// Collection lifecycle commands (get, sync, refresh, check-integrity, undo)
    Collection {
        #[command(subcommand)]
        command: CollectionCmd,
    },

    /// Deck commands (list, get, create, update, delete, counts)
    Decks {
        #[command(subcommand)]
        command: DecksCmd,
    },

    /// Note commands (get, search, create, update, delete, validate)
    Notes {
        #[command(subcommand)]
        command: NotesCmd,
    },

    /// Card commands (get, search, move, suspend, unsuspend, flags)
    Cards {
        #[command(subcommand)]
        command: CardsCmd,
    },

    /// Async operation tracking (get, wait, cancel)
    Operations {
        #[command(subcommand)]
        command: OperationsCmd,
    },

    /// Execute a batch of JSON-RPC requests via api.execute or transport batching
    Batch(BatchArgs),
}

#[derive(Args)]
pub struct CallArgs {
    /// Method name (e.g. "decks.list", "notes.create")
    pub method: String,

    /// YAML or JSON object of named parameters
    #[arg(long, short = 'p')]
    pub params: Option<String>,

    /// Read YAML or JSON parameters from stdin
    #[arg(long, conflicts_with = "params")]
    pub stdin: bool,

    /// Return the OperationHandle immediately without polling operations.get
    #[arg(long)]
    pub no_wait: bool,

    /// Operation polling timeout in seconds
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

#[derive(Args)]
pub struct SchemaArgs {
    /// Specific method name to inspect (e.g. "notes.create"). Omit to list all methods.
    pub method: Option<String>,

    /// Filter method list by substring
    #[arg(long)]
    pub filter: Option<String>,

    /// Show documented error codes and server limits instead of methods
    #[arg(long)]
    pub errors: bool,
}

#[derive(Subcommand)]
pub enum NotesCmd {
    /// Get a note by ID
    Get { id: String },
    /// Search notes using Anki query syntax
    Search {
        /// Anki search query (empty string matches all)
        #[arg(default_value = "")]
        query: String,
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

#[derive(Subcommand)]
pub enum CardsCmd {
    /// Get a card by ID
    Get { id: String },
    /// Search cards using Anki query syntax
    Search {
        #[arg(default_value = "")]
        query: String,
        #[arg(long, default_value_t = 100)]
        limit: u64,
        #[arg(long)]
        cursor: Option<String>,
        /// Auto-paginate through all matching IDs
        #[arg(long)]
        all: bool,
    },
    /// Move cards to a target deck
    Move {
        #[arg(long)]
        deck: String,
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

#[derive(Args)]
pub struct BatchArgs {
    /// Path to a YAML or JSON file containing an array of {method, params} objects
    #[arg(long, conflicts_with = "stdin")]
    pub file: Option<String>,

    /// Read YAML or JSON array of {method, params} objects from stdin
    #[arg(long, conflicts_with = "file")]
    pub stdin: bool,

    /// Inline YAML or JSON array of {method, params} objects
    #[arg(long, short = 'c', allow_hyphen_values = true, conflicts_with_all = ["file", "stdin"])]
    pub commands: Option<String>,

    /// Batch execution mode: `execute` (api.execute) or `transport` (JSON-RPC array)
    #[arg(long, value_enum, default_value_t = BatchMode::Execute)]
    pub mode: BatchMode,

    /// Return OperationHandle immediately for `execute` mode without polling
    #[arg(long)]
    pub no_wait: bool,

    /// Operation polling timeout in seconds (for `execute` mode)
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum BatchMode {
    /// Sequential execution via api.execute (polls OperationHandle unless --no-wait)
    #[default]
    Execute,
    /// Raw JSON-RPC 2.0 TransportBatch array POST
    Transport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RpcPlan {
    /// Offline/local result requiring no Anki-Connect server call (e.g., `schema`)
    Local(Value),
    /// Single JSON-RPC method call (optionally polling if result is an OperationHandle)
    Call {
        method: String,
        params: Value,
        wait: bool,
        timeout_secs: u64,
    },
    /// Cursor-paginated call that auto-fetches all pages and merges `array_key`
    Paginate {
        method: String,
        params: Map<String, Value>,
        array_key: &'static str,
    },
    /// Poll `operations.get` for an existing operation ID until completion
    WaitOperation {
        operation_id: String,
        timeout_secs: u64,
    },
    /// Batch execution via `api.execute` or JSON-RPC transport batch
    Batch {
        mode: BatchMode,
        requests: Vec<(String, Value)>,
        wait: bool,
        timeout_secs: u64,
    },
}

fn call_plan(method: impl Into<String>, params: Value) -> RpcPlan {
    RpcPlan::Call {
        method: method.into(),
        params,
        wait: false,
        timeout_secs: 0,
    }
}

fn search_plan(
    method: &str,
    query: &str,
    limit: u64,
    cursor: Option<&str>,
    all: bool,
) -> RpcPlan {
    let mut params = Map::new();
    params.insert("query".into(), json!(query));
    params.insert("limit".into(), json!(limit));
    if let Some(c) = cursor {
        params.insert("cursor".into(), json!(c));
    }
    if all {
        RpcPlan::Paginate {
            method: method.into(),
            params,
            array_key: "ids",
        }
    } else {
        call_plan(method, Value::Object(params))
    }
}

/// Translate a parsed CLI command into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &Commands) -> Result<RpcPlan, CliError> {
    match cmd {
        Commands::Call(args) => {
            let params = parse_params(args.params.as_deref(), args.stdin)?;
            Ok(RpcPlan::Call {
                method: args.method.clone(),
                params,
                wait: !args.no_wait,
                timeout_secs: args.timeout,
            })
        }
        Commands::Schema(args) => {
            let value = if args.errors {
                schema::list_errors_and_limits()?
            } else if let Some(method) = &args.method {
                schema::describe_method(method)?
            } else {
                schema::list_methods(args.filter.as_deref())?
            };
            Ok(RpcPlan::Local(value))
        }
        Commands::Describe => Ok(call_plan("api.describe", json!({}))),
        Commands::Collection { command } => collection::to_rpc_plan(command),
        Commands::Decks { command } => decks::to_rpc_plan(command),
        Commands::Notes { command } => match command {
            NotesCmd::Get { id } => Ok(call_plan("notes.get", json!({ "id": id }))),
            NotesCmd::Search {
                query,
                limit,
                cursor,
                all,
            } => Ok(search_plan(
                "notes.search",
                query,
                *limit,
                cursor.as_deref(),
                *all,
            )),
            NotesCmd::Create {
                deck,
                note_type,
                fields,
                tags,
            } => {
                let fields_val = parse_structured_object(fields, "--fields")?;
                Ok(call_plan(
                    "notes.create",
                    json!({
                        "deck": { "name": deck },
                        "noteType": { "name": note_type },
                        "fields": fields_val,
                        "tags": tags,
                    }),
                ))
            }
            NotesCmd::Validate {
                deck,
                note_type,
                fields,
                tags,
            } => {
                let fields_val = parse_structured_object(fields, "--fields")?;
                Ok(call_plan(
                    "notes.validate",
                    json!({
                        "note": {
                            "deck": { "name": deck },
                            "noteType": { "name": note_type },
                            "fields": fields_val,
                            "tags": tags,
                        }
                    }),
                ))
            }
            NotesCmd::Update { id, fields, tags } => {
                let mut patch = Map::new();
                if let Some(f) = fields {
                    let fields_val = parse_structured_object(f, "--fields")?;
                    patch.insert("fields".into(), fields_val);
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
        },
        Commands::Cards { command } => match command {
            CardsCmd::Get { id } => Ok(call_plan("cards.get", json!({ "id": id }))),
            CardsCmd::Search {
                query,
                limit,
                cursor,
                all,
            } => Ok(search_plan(
                "cards.search",
                query,
                *limit,
                cursor.as_deref(),
                *all,
            )),
            CardsCmd::Move { deck, ids } => Ok(call_plan(
                "cards.move",
                json!({ "ids": ids, "deck": { "name": deck } }),
            )),
            CardsCmd::Suspend { ids } => Ok(call_plan(
                "cards.setSuspended",
                json!({ "ids": ids, "suspended": true }),
            )),
            CardsCmd::Unsuspend { ids } => Ok(call_plan(
                "cards.setSuspended",
                json!({ "ids": ids, "suspended": false }),
            )),
            CardsCmd::Flags { flag, ids } => Ok(call_plan(
                "cards.setFlags",
                json!({ "ids": ids, "flag": flag }),
            )),
        },
        Commands::Operations { command } => operations::to_rpc_plan(command),
        Commands::Batch(args) => {
            let requests = parse_batch_requests(args)?;
            Ok(RpcPlan::Batch {
                mode: args.mode,
                requests,
                wait: !args.no_wait,
                timeout_secs: args.timeout,
            })
        }
    }
}

/// Execute an `RpcPlan` against an `AnkiClient`.
pub fn execute_plan(client: &AnkiClient, plan: RpcPlan) -> Result<Value, CliError> {
    match plan {
        RpcPlan::Local(value) => Ok(value),
        RpcPlan::Call {
            method,
            params,
            wait,
            timeout_secs,
        } => client.call_and_maybe_wait(&method, params, wait, timeout_secs),
        RpcPlan::Paginate {
            method,
            params,
            array_key,
        } => client.paginate_all(&method, params, array_key),
        RpcPlan::WaitOperation {
            operation_id,
            timeout_secs,
        } => client.wait_for_operation(&operation_id, timeout_secs),
        RpcPlan::Batch {
            mode,
            requests,
            wait,
            timeout_secs,
        } => match mode {
            BatchMode::Execute => client.execute_batch(requests, wait, timeout_secs),
            BatchMode::Transport => client.transport_batch(requests),
        },
    }
}

pub fn dispatch(client: &AnkiClient, cmd: Commands) -> Result<Value, CliError> {
    let plan = to_rpc_plan(&cmd)?;
    execute_plan(client, plan)
}

fn read_stdin() -> Result<String, CliError> {
    helpers::read_input_or_stdin(None, true, "input")
}

/// Parse a YAML or JSON string and require it to be a JSON object.
pub fn parse_structured_object(raw: &str, flag_name: &str) -> Result<Value, CliError> {
    helpers::parse_yaml_or_json_object(raw, flag_name).map(Value::Object)
}

fn parse_params(params: Option<&str>, use_stdin: bool) -> Result<Value, CliError> {
    let raw = if use_stdin {
        read_stdin()?
    } else if let Some(p) = params {
        p.to_string()
    } else {
        return Ok(json!({}));
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }

    parse_structured_object(trimmed, "--params")
}

fn parse_batch_requests(args: &BatchArgs) -> Result<Vec<(String, Value)>, CliError> {
    let raw = if let Some(commands) = &args.commands {
        commands.clone()
    } else if let Some(path) = &args.file {
        std::fs::read_to_string(path).map_err(|e| {
            CliError::InvalidInput(format!("Failed to read batch file '{path}': {e}"))
        })?
    } else if args.stdin {
        read_stdin()?
    } else {
        return Err(CliError::InvalidInput(
            "Provide one of --commands, --file, or --stdin".into(),
        ));
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CliError::InvalidInput(
            "Batch input must be a non-empty YAML or JSON array".into(),
        ));
    }

    let val: Value = serde_norway::from_str(trimmed)
        .map_err(|e| CliError::InvalidInput(format!("Invalid YAML/JSON batch input: {e}")))?;
    let arr = val.as_array().ok_or_else(|| {
        CliError::InvalidInput("Batch input must be a YAML or JSON array".into())
    })?;
    if arr.is_empty() {
        return Err(CliError::InvalidInput(
            "Batch input array must not be empty".into(),
        ));
    }

    let mut requests = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let obj = item.as_object().ok_or_else(|| {
            CliError::InvalidInput(format!("Batch item [{idx}] must be an object"))
        })?;
        let method = obj
            .get("method")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .ok_or_else(|| {
                CliError::InvalidInput(format!(
                    "Batch item [{idx}] must have a non-empty string 'method'"
                ))
            })?
            .to_string();
        let params = match obj.get("params") {
            None | Some(Value::Null) => json!({}),
            Some(v) if v.is_object() => v.clone(),
            Some(_) => {
                return Err(CliError::InvalidInput(format!(
                    "Batch item [{idx}].params must be an object"
                )));
            }
        };
        requests.push((method, params));
    }
    Ok(requests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_defaults_to_yaml_output_and_validation_enabled() {
        let cli = Cli::try_parse_from(["anki-cli", "decks", "list"]).expect("parse cli");
        assert_eq!(cli.output_format(), OutputFormat::Yaml);
        assert!(!cli.no_validate);
    }

    #[test]
    fn cli_resolves_json_and_pretty_flags() {
        let cli_json = Cli::try_parse_from(["anki-cli", "--json", "decks", "list"]).expect("json");
        assert_eq!(cli_json.output_format(), OutputFormat::Json);

        let cli_format_json =
            Cli::try_parse_from(["anki-cli", "--format", "json", "decks", "list"])
                .expect("format json");
        assert_eq!(cli_format_json.output_format(), OutputFormat::Json);

        let cli_pretty =
            Cli::try_parse_from(["anki-cli", "--pretty", "decks", "list"]).expect("pretty");
        assert_eq!(cli_pretty.output_format(), OutputFormat::JsonPretty);

        let cli_format_pretty =
            Cli::try_parse_from(["anki-cli", "-f", "json-pretty", "decks", "list"])
                .expect("format json-pretty");
        assert_eq!(cli_format_pretty.output_format(), OutputFormat::JsonPretty);
    }

    #[test]
    fn cli_parses_no_validate_and_schema_filter() {
        let cli = Cli::try_parse_from([
            "anki-cli",
            "--no-validate",
            "-f",
            "json",
            "schema",
            "--filter",
            "decks",
        ])
        .expect("parse schema with global -f");
        assert!(cli.no_validate);
        assert_eq!(cli.output_format(), OutputFormat::Json);
    }

    fn plan_for(args: &[&str]) -> RpcPlan {
        let mut argv = vec!["anki-cli"];
        argv.extend_from_slice(args);
        let cli = Cli::try_parse_from(argv).expect("parse cli");
        to_rpc_plan(&cli.command).expect("build rpc plan")
    }

    #[test]
    fn to_rpc_plan_call_supports_defaults_yaml_params_and_wait_flags() {
        assert_eq!(
            plan_for(&["call", "decks.list"]),
            RpcPlan::Call {
                method: "decks.list".into(),
                params: json!({}),
                wait: true,
                timeout_secs: 300,
            }
        );

        let yaml_params = "deck: {name: Default}\nnoteType: {name: Basic}\nfields: {Front: Q, Back: A}";
        let plan = plan_for(&["call", "notes.create", "--params", yaml_params]);
        assert_eq!(
            plan,
            RpcPlan::Call {
                method: "notes.create".into(),
                params: json!({
                    "deck": { "name": "Default" },
                    "noteType": { "name": "Basic" },
                    "fields": { "Front": "Q", "Back": "A" },
                }),
                wait: true,
                timeout_secs: 300,
            }
        );
        if let RpcPlan::Call { method, params, .. } = plan {
            schema::validate_method_params(&method, &params).expect("valid notes.create params");
        }

        assert_eq!(
            plan_for(&["call", "collection.sync", "--no-wait", "--timeout", "45"]),
            RpcPlan::Call {
                method: "collection.sync".into(),
                params: json!({}),
                wait: false,
                timeout_secs: 45,
            }
        );
    }

    #[test]
    fn to_rpc_plan_batch_parses_yaml_and_json_modes() {
        let yaml_batch = "- method: decks.list\n- method: tags.list\n  params: {}";
        assert_eq!(
            plan_for(&["batch", "--commands", yaml_batch]),
            RpcPlan::Batch {
                mode: BatchMode::Execute,
                requests: vec![
                    ("decks.list".into(), json!({})),
                    ("tags.list".into(), json!({})),
                ],
                wait: true,
                timeout_secs: 300,
            }
        );

        assert_eq!(
            plan_for(&[
                "batch",
                "--mode",
                "transport",
                "--no-wait",
                "--timeout",
                "12",
                "--commands",
                r#"[{"method":"api.describe"}]"#,
            ]),
            RpcPlan::Batch {
                mode: BatchMode::Transport,
                requests: vec![("api.describe".into(), json!({}))],
                wait: false,
                timeout_secs: 12,
            }
        );
    }

    #[test]
    fn to_rpc_plan_schema_and_describe() {
        assert!(matches!(plan_for(&["schema"]), RpcPlan::Local(_)));
        assert!(matches!(
            plan_for(&["schema", "notes.create"]),
            RpcPlan::Local(_)
        ));
        assert!(matches!(
            plan_for(&["schema", "--errors"]),
            RpcPlan::Local(_)
        ));

        assert_eq!(
            plan_for(&["describe"]),
            RpcPlan::Call {
                method: "api.describe".into(),
                params: json!({}),
                wait: false,
                timeout_secs: 0,
            }
        );
    }

    #[test]
    fn to_rpc_plan_domain_subcommands_produce_schema_valid_plans() {
        let call_cases: Vec<&[&str]> = vec![
            &["collection", "get"],
            &["collection", "sync", "--no-wait", "--timeout", "30"],
            &["collection", "refresh"],
            &["collection", "check-integrity"],
            &["collection", "undo"],
            &["decks", "list"],
            &["decks", "get", "--name", "Default"],
            &["decks", "create", "Japanese"],
            &["decks", "update", "--id", "123", "--new-name", "Japanese::Vocab"],
            &["decks", "delete", "--name", "Temp", "--delete-cards"],
            &["decks", "counts", "--name", "Default", "--include-children"],
            &["notes", "get", "1001"],
            &["notes", "search", "deck:Default", "--limit", "50"],
            &[
                "notes",
                "create",
                "--deck",
                "Default",
                "--note-type",
                "Basic",
                "--fields",
                "Front: Question\nBack: Answer",
                "--tag",
                "vocab",
            ],
            &[
                "notes",
                "validate",
                "--deck",
                "Default",
                "--note-type",
                "Basic",
                "--fields",
                r#"{"Front":"Q","Back":"A"}"#,
            ],
            &[
                "notes",
                "update",
                "1001",
                "--fields",
                "Front: Updated",
                "--tag",
                "reviewed",
            ],
            &["notes", "delete", "1001", "1002"],
            &["cards", "get", "2001"],
            &["cards", "search", "is:due", "--limit", "25"],
            &["cards", "move", "--deck", "Archive", "2001"],
            &["cards", "suspend", "2001"],
            &["cards", "unsuspend", "2001"],
            &["cards", "flags", "--flag", "2", "2001"],
            &["operations", "get", "op-1"],
            &["operations", "cancel", "op-1"],
        ];

        for args in call_cases {
            match plan_for(args) {
                RpcPlan::Call { method, params, .. } => {
                    schema::validate_method_params(&method, &params).unwrap_or_else(|err| {
                        panic!("schema validation failed for {args:?} ({method}): {err}")
                    });
                }
                other => panic!("expected RpcPlan::Call for {args:?}, got {other:?}"),
            }
        }

        match plan_for(&["notes", "search", "deck:Default", "--all"]) {
            RpcPlan::Paginate {
                method,
                params,
                array_key,
            } => {
                assert_eq!(array_key, "ids");
                schema::validate_method_params(&method, &Value::Object(params))
                    .expect("valid notes.search paginate params");
            }
            other => panic!("expected RpcPlan::Paginate, got {other:?}"),
        }

        match plan_for(&["cards", "search", "is:due", "--all"]) {
            RpcPlan::Paginate {
                method,
                params,
                array_key,
            } => {
                assert_eq!(array_key, "ids");
                schema::validate_method_params(&method, &Value::Object(params))
                    .expect("valid cards.search paginate params");
            }
            other => panic!("expected RpcPlan::Paginate, got {other:?}"),
        }

        assert_eq!(
            plan_for(&["operations", "wait", "op-99", "--timeout", "90"]),
            RpcPlan::WaitOperation {
                operation_id: "op-99".into(),
                timeout_secs: 90,
            }
        );
    }

    #[test]
    fn parse_structured_object_and_batch_reject_invalid_inputs() {
        let non_obj = parse_structured_object("- item1\n- item2", "--fields")
            .expect_err("array should be rejected");
        assert_eq!(non_obj.exit_code(), 2);

        let empty_batch = Cli::try_parse_from(["anki-cli", "batch", "--commands", "[]"])
            .expect("parse empty batch");
        let err = to_rpc_plan(&empty_batch.command).expect_err("empty batch should fail");
        assert_eq!(err.exit_code(), 2);

        let missing_method = Cli::try_parse_from(["anki-cli", "batch", "--commands", "- params: {}"])
            .expect("parse missing method");
        let err = to_rpc_plan(&missing_method.command).expect_err("missing method should fail");
        assert_eq!(err.exit_code(), 2);
    }
}

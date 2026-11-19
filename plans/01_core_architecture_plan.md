# Plan 1: Core Architecture, YAML-Default Output, Schema Validation & `RpcPlan` Layer

## Goal
Refactor `anki-skill` (`anki-cli`) into a library + thin binary crate with YAML output by default (`--format <yaml|json|json-pretty>`), cached schema metadata and pre-flight parameter validation against `schemas/api-v1.schema.json`, scope-aware session injection and batch support on `AnkiClient`, and a clean separation between CLI-to-RPC planning (`to_rpc_plan`) and execution (`execute_plan`).

### Implementation Status
- [x] **Phase 1 — Dependencies, Library Split & YAML/JSON Output Formatting (`1.1`–`1.4`)**
- [x] **Phase 2 — Schema Caching, Metadata Lookups & Pre-Flight Validation (`2.1`–`2.3`)**
- [x] **Phase 3 — Scope-Aware `AnkiClient`, Configurable Polling & Batch Support (`3.1`–`3.3`)**
- [x] **Phase 4 — `RpcPlan` Separation & Escape Hatch Subcommands (`4.1`–`4.3`)**

### Handoff Context (Updated After Each Task)
> **Next task**: `None` (Plan 1 complete)
>
> **Context from completed tasks**:
> - **1.1**: Updated `Cargo.toml` with `[lib]` (`anki_skill` at `src/lib.rs`), `[[bin]]` (`anki-cli` at `src/main.rs`), `serde_norway = "0.9"`, and `[dev-dependencies]` (`assert_cmd = "2"`, `jsonschema = "0.29"` with `default-features = false`, `mockito = "1"`, `predicates = "3"`). Created `src/lib.rs` exporting `pub mod client; pub mod commands; pub mod error; pub mod output; pub mod schema;` and a minimal stub `src/output.rs`. Note: `cargo clippy` emits a manifest warning for unused `serde_norway` until `1.2` uses it (still exits 0).
> - **1.2**: Implemented `OutputFormat` (`Yaml` default, `Json`, `JsonPretty`), `OutputFormat::resolve`, `format_value`, and `format_error` in `src/output.rs`, along with 4 unit tests covering default format, flag precedence, YAML/JSON/JSON-pretty rendering, and error formatting. Note: `format_value` currently maps `serde_norway::Error` to `CliError::InvalidInput` until `1.3` adds `CliError::Yaml`.
> - **1.3**: Added `CliError::Yaml(#[from] serde_norway::Error)` in `src/error.rs` (mapped to exit code `2` in `CliError::exit_code()`), updated `format_value` in `src/output.rs` to propagate `serde_norway::Error` via `?` as `CliError::Yaml`, and added 2 unit tests in `src/error.rs` (`exit_codes_are_deterministic` and `to_json_produces_expected_envelope_for_each_kind`).
> - **1.4**: Moved `pub struct Cli` into `src/commands/mod.rs` (re-exported as `pub use commands::Cli;` in `src/lib.rs`) with global `--format`/`-f`, `--json`, `--pretty`, `--no-validate`, and `Cli::output_format() -> OutputFormat`. Removed conflicting `short = 'f'` from `SchemaArgs::filter` (kept `--filter`). Updated `src/main.rs` to import from `anki_skill::*` and render both `Ok` (`stdout` via `output::format_value`) and `Err` (`stderr` via `output::format_error`) using the resolved `OutputFormat`. Added 3 CLI parsing unit tests in `src/commands/mod.rs`. Note: `cli.no_validate` is parsed and stored, and will be wired into `AnkiClient::with_validation(!cli.no_validate)` in Task `3.1`.
> - **2.1**: Replaced `load_schema()` in `src/schema.rs` with a process-wide `static SCHEMA: LazyLock<Value>` and `pub fn raw_schema() -> &'static Value`. Updated `list_methods`, `describe_method`, and `list_errors_and_limits` to read from `raw_schema()`. Added metadata helpers `pub fn method_spec(method: &str) -> Option<&'static Value>`, `pub fn method_scope(method: &str) -> Option<&'static str>`, `pub fn method_completion(method: &str) -> Option<&'static str>`, and `pub fn all_method_names() -> Vec<&'static str>` (sorted for deterministic order).
> - **2.2**: Implemented `pub fn validate_method_params(method: &str, params: &Value) -> Result<(), CliError>` and recursive `validate_value_against_schema` in `src/schema.rs`, supporting `$ref` (`#/$defs/...`), `const`, `enum`, `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`, `type` (string or array), object (`required`, `minProperties`, `properties`, `additionalProperties`), array (`minItems`, `maxItems`, `uniqueItems`, `items`), string (`minLength`, `maxLength`, `pattern` including `EntityId` `^[1-9][0-9]{0,18}$` positive `i64` check), and number/integer (`minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`). Unknown methods return `CliError::SchemaNotFound`; validation failures return `CliError::InvalidInput` with method and JSON path.
> - **2.3**: Added 5 unit tests in `src/schema.rs` (`all_method_names_returns_all_96_methods_sorted`, `method_scope_returns_expected_scopes`, `method_completion_returns_expected_modes`, `validate_method_params_accepts_valid_inputs`, and `validate_method_params_rejects_invalid_inputs`) covering all 96 methods, scope/completion lookups, valid parameter payloads, and rejections for unknown methods, missing required fields, `additionalProperties: false`, invalid `EntityId`s, and invalid `Reference` `oneOf` objects.
> - **3.1**: Added `validate: bool` (default `true`), `pub fn with_validation(mut self, validate: bool) -> Self`, `pub fn session(&self) -> Option<&str>`, and `pub fn prepare_params(&self, method: &str, params: Value) -> Result<Value, CliError>` on `AnkiClient` in `src/client.rs`. `prepare_params` only injects `"session"` when `schema::method_scope(method) == Some("collection")` and runs `schema::validate_method_params(method, &params)?` when `self.validate` is `true`. Updated `AnkiClient::call` to use `prepare_params`, and wired `.with_validation(!cli.no_validate)` in `src/main.rs`.
> - **3.2**: Added `poll_initial_delay: Duration` (default `250ms`) and `poll_max_delay: Duration` (default `2s`) fields plus `pub fn with_poll_delays(mut self, initial: Duration, max: Duration) -> Self` on `AnkiClient` in `src/client.rs`, updated `wait_for_operation` to use them, and added 3 unit tests (`prepare_params_injects_session_only_for_collection_scope`, `prepare_params_respects_validation_flag`, and `with_poll_delays_updates_polling_configuration`).
> - **3.3**: Implemented `pub fn execute_batch(&self, commands: Vec<(String, Value)>, wait: bool, timeout_secs: u64) -> Result<Value, CliError>` and `pub fn transport_batch(&self, requests: Vec<(String, Value)>) -> Result<Value, CliError>` on `AnkiClient` in `src/client.rs`, sharing `post_json` and `rpc_error_from_value` helpers with `call`/`parse_rpc_reply`. Both batch methods run `prepare_params` on each inner command for scope-aware session injection and pre-flight validation; `transport_batch` enforces 1..=100 requests (exit code `2` otherwise). Added 4 unit tests (`transport_batch_rejects_invalid_batch_sizes`, `prepare_execute_commands_applies_scope_and_validation`, `transport_batch_posts_array_and_returns_response_array`, and `execute_batch_posts_api_execute_and_returns_handle_when_no_wait`).
> - **4.1**: Defined `pub enum BatchMode` (`Execute`, `Transport`) and `pub enum RpcPlan` (`Local`, `Call`, `Paginate`, `WaitOperation`, `Batch`) in `src/commands/mod.rs` and re-exported them in `src/lib.rs`. Split `dispatch` into `pub fn to_rpc_plan(cmd: &Commands) -> Result<RpcPlan, CliError>` (with `call_plan` and `search_plan` helpers, and `parse_params`/`make_ref` taking `&str`) and `pub fn execute_plan(client: &AnkiClient, plan: RpcPlan) -> Result<Value, CliError>`, with `dispatch` delegating to both.
> - **4.2**: Added `pub fn parse_structured_object(raw: &str, flag_name: &str) -> Result<Value, CliError>` using `serde_norway::from_str` in `src/commands/mod.rs` and updated `parse_params` as well as `NotesCmd::Create`, `NotesCmd::Validate`, and `NotesCmd::Update` (`--fields`) to accept both YAML and JSON objects. Added `Commands::Batch(BatchArgs)` (`--file`, `--stdin`, `--commands`/`-c`, `--mode`, `--no-wait`, `--timeout`; omitted `short = 'f'` on `--file` to avoid conflicting with global `-f`/`--format`) and `parse_batch_requests`, mapping `Commands::Batch` to `RpcPlan::Batch` in `to_rpc_plan`.
> - **4.3**: Added 5 unit tests in `src/commands/mod.rs` (`to_rpc_plan_call_supports_defaults_yaml_params_and_wait_flags`, `to_rpc_plan_batch_parses_yaml_and_json_modes`, `to_rpc_plan_schema_and_describe`, `to_rpc_plan_domain_subcommands_produce_schema_valid_plans`, and `parse_structured_object_and_batch_reject_invalid_inputs`) covering `call`, `batch`, `schema`, `describe`, all domain subcommands (`collection`, `decks`, `notes`, `cards`, `operations`) validated against `schema::validate_method_params`, and invalid input rejection. Also fixed `NotesCmd::Validate` in `src/commands/mod.rs` to wrap the payload under `"note"` per `notes_validate_Params`, and added `allow_hyphen_values = true` on `BatchArgs::commands` so YAML list strings starting with `- ` parse cleanly.

> [!IMPORTANT]
> ### Autonomous Execution Protocol: `"Continue with next task @plans/01_core_architecture_plan.md"`
> When the user says **"Continue with next task @plans/01_core_architecture_plan.md"** (or similar):
> 1. **Read Context & Find Next Task**: Read **Implementation Status**, **Handoff Context**, and **Architectural Constraints & Verification**, then scan **Build Order & Tasks** for the **first unchecked task** (`- [ ] **<Task ID>**`).
> 2. **Execute ONLY That One Task**: Implement only the files and scope listed for that single task. Do NOT jump ahead to subsequent tasks.
> 3. **Verify**: Run the verification commands listed in the task / **Architectural Constraints & Verification** and confirm all checks pass.
> 4. **Update Plan & Handoff Context**:
>    - Check off `- [x] **<Task ID>**` in **Build Order & Tasks** (and check off the Phase in **Implementation Status** when all tasks in that phase are complete).
>    - Update **Handoff Context** with the new `Next task` (or `None` when complete) and a concise bullet summarizing what was implemented (exact file paths, symbols, signatures, keys, and caveats).
> 5. **Stop & Report**: Briefly summarize what was done, show verification results, explicitly announce the next pending task, and stop.

## Architectural Constraints & Verification
- **Constraints**:
  - Keep blocking synchronous architecture (`reqwest::blocking::Client`); do not introduce an async runtime (`tokio`).
  - Default output on both `stdout` and `stderr` must be **YAML** via `serde_norway`, with `--format <yaml|json|json-pretty>` (short `-f`) and convenience aliases `--json` and `--pretty`.
  - Session injection in `AnkiClient` must be strictly scope-aware: only inject `"session"` into `params` when `schema::method_scope(method) == Some("collection")`. Never inject `"session"` into `scope: "application"` methods (`operations.get`, `operations.cancel`, `api.describe`, `api.execute`, `profiles.*`, `access.requestPermission`, `application.exit`).
  - All generated RPC payloads and `api.execute` batch items must strictly conform to `schemas/api-v1.schema.json` (`additionalProperties: false`).
  - Deterministic exit codes must be preserved: `0` (success), `2` (invalid CLI input, YAML/JSON parse error, or schema validation failure), `3` (transport or HTTP status error), `4` (JSON-RPC or async operation failure), `5` (operation polling timeout).
- **Verification**: `cargo test --all-targets` and `cargo clippy --all-targets -- -D warnings` (must pass after every task).

---

## Build Order & Tasks

### Phase 1 — Dependencies, Library Split & YAML/JSON Output Formatting

- [x] **1.1 Update `Cargo.toml` and create `src/lib.rs`**
  - **File(s)**:
    - `Cargo.toml`
    - `src/lib.rs` (new)
    - `src/output.rs` (new minimal stub so `src/lib.rs` compiles)
  - **Scope**:
    - Update `Cargo.toml` to define both library and binary targets:
      ```toml
      [lib]
      name = "anki_skill"
      path = "src/lib.rs"

      [[bin]]
      name = "anki-cli"
      path = "src/main.rs"
      ```
    - Add `serde_norway = "0.9"` under `[dependencies]`.
    - Add `[dev-dependencies]` for subsequent unit, schema-contract, and mock HTTP tests:
      ```toml
      [dev-dependencies]
      assert_cmd = "2"
      jsonschema = { version = "0.29", default-features = false }
      mockito = "1"
      predicates = "3"
      ```
    - Create `src/lib.rs` exporting the core public modules:
      ```rust
      pub mod client;
      pub mod commands;
      pub mod error;
      pub mod output;
      pub mod schema;
      ```
    - Create an initial placeholder `src/output.rs` (to be fleshed out in `1.2`) so `cargo test` and `cargo clippy` pass immediately.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **1.2 Implement `OutputFormat` and YAML/JSON formatters in `src/output.rs`**
  - **File(s)**:
    - `src/output.rs`
  - **Scope**:
    - Define `OutputFormat`:
      ```rust
      use clap::ValueEnum;
      use serde_json::Value;
      use crate::error::CliError;

      #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
      pub enum OutputFormat {
          #[default]
          Yaml,
          Json,
          #[value(name = "json-pretty")]
          JsonPretty,
      }
      ```
    - Implement `OutputFormat::resolve(format: Option<OutputFormat>, json_flag: bool, pretty_flag: bool) -> OutputFormat`:
      - If `format` is explicitly `Some(fmt)`, if `pretty_flag && fmt == OutputFormat::Json` return `OutputFormat::JsonPretty`, else return `fmt`.
      - Else if `json_flag && pretty_flag` or `pretty_flag` alone is set, return `OutputFormat::JsonPretty`.
      - Else if `json_flag` is set, return `OutputFormat::Json`.
      - Otherwise return `OutputFormat::Yaml`.
    - Implement `pub fn format_value(value: &Value, format: OutputFormat) -> Result<String, CliError>`:
      - `OutputFormat::Yaml`: serialize with `serde_norway::to_string(value)` and trim trailing newline (`trim_end().to_string()`) so `println!("{s}")` does not emit double blank lines.
      - `OutputFormat::Json`: serialize with `serde_json::to_string(value)`.
      - `OutputFormat::JsonPretty`: serialize with `serde_json::to_string_pretty(value)`.
    - Implement `pub fn format_error(err: &CliError, format: OutputFormat) -> String`:
      - Convert `err.to_json()` into a `Value`, format it with `format_value(&json_err, format)`, and fall back to a safe inline string (`error:\n  kind: cli\n  message: ...` for YAML or `{"error":{"kind":"cli","message":...}}` for JSON) if serialization ever fails.
    - Add unit tests in `src/output.rs` verifying:
      - `OutputFormat::default()` is `OutputFormat::Yaml`.
      - `OutputFormat::resolve` precedence for `--format`, `--json`, and `--pretty`.
      - `format_value` produces valid YAML without trailing newline for `OutputFormat::Yaml`, compact JSON for `OutputFormat::Json`, and indented JSON for `OutputFormat::JsonPretty`.
      - `format_error` formats `CliError::Rpc` and `CliError::InvalidInput` in both YAML and JSON.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **1.3 Add YAML serialization error handling and unit tests in `src/error.rs`**
  - **File(s)**:
    - `src/error.rs`
  - **Scope**:
    - Add `Yaml(#[from] serde_norway::Error)` variant to `CliError`:
      ```rust
      #[error("YAML serialization error: {0}")]
      Yaml(#[from] serde_norway::Error),
      ```
    - Map `CliError::Yaml(_)` to exit code `2` in `CliError::exit_code()`.
    - Add unit tests in `src/error.rs` verifying:
      - Deterministic exit codes:
        - `2` for `InvalidInput`, `SchemaNotFound`, `Json`, and `Yaml`.
        - `3` for `HttpStatus`.
        - `4` for `Rpc` and `OperationFailed`.
        - `5` for `Timeout`.
      - `to_json()` envelope structure for each error kind (`rpc`, `operation`, `http`, `timeout`, `cli`).
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **1.4 Wire library crate, `--format`/`--json`/`--pretty`, `--no-validate`, and public `Cli` into `src/commands/mod.rs` and `src/main.rs`**
  - **File(s)**:
    - `src/commands/mod.rs`
    - `src/lib.rs`
    - `src/main.rs`
  - **Scope**:
    - Move `pub struct Cli` definition into `src/commands/mod.rs` (and re-export `pub use commands::Cli;` in `src/lib.rs`) so integration and contract tests can call `Cli::try_parse_from(...)`.
    - Enhance `Cli` global flags:
      - `#[arg(long, default_value = "http://127.0.0.1:8765/api/v1", env = "ANKI_CONNECT_URL", global = true)] pub url: String`
      - `#[arg(long, env = "ANKI_CONNECT_API_KEY", global = true)] pub api_key: Option<String>`
      - `#[arg(long, env = "ANKI_CONNECT_SESSION", global = true)] pub session: Option<String>`
      - `#[arg(long, short = 'f', value_enum, global = true)] pub format: Option<OutputFormat>`
      - `#[arg(long, global = true)] pub json: bool` (convenience flag for `--format json`)
      - `#[arg(long, global = true)] pub pretty: bool` (convenience flag for `--format json-pretty`)
      - `#[arg(long, global = true)] pub no_validate: bool` (bypass client-side schema validation)
      - `#[command(subcommand)] pub command: Commands`
    - Add helper `pub fn output_format(&self) -> OutputFormat` on `Cli` delegating to `OutputFormat::resolve(self.format, self.json, self.pretty)`.
    - Update `src/main.rs` to import from `anki_skill::*` (removing duplicate `mod client; mod commands; ...` declarations in `src/main.rs`), resolve `let format = cli.output_format();`, and render both `Ok(val)` on `stdout` via `output::format_value(&val, format)` and `Err(err)` on `stderr` via `output::format_error(&err, format)`.
    - Add unit tests in `src/commands/mod.rs` verifying `Cli::try_parse_from` resolves `OutputFormat::Yaml` by default, `OutputFormat::Json` with `--json` or `--format json`, `OutputFormat::JsonPretty` with `--pretty` or `--format json-pretty`, and sets `no_validate` when `--no-validate` is passed.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 2 — Schema Caching, Metadata Lookups & Pre-Flight Validation

- [x] **2.1 Cache parsed schema with `LazyLock` and add method metadata helpers in `src/schema.rs`**
  - **File(s)**:
    - `src/schema.rs`
  - **Scope**:
    - Replace repeated `serde_json::from_str(RAW_SCHEMA)` calls with a process-wide cached `LazyLock<Value>`:
      ```rust
      use std::sync::LazyLock;

      static SCHEMA: LazyLock<Value> = LazyLock::new(|| {
          serde_json::from_str(RAW_SCHEMA)
              .expect("embedded schemas/api-v1.schema.json must be valid JSON")
      });

      pub fn raw_schema() -> &'static Value {
          &SCHEMA
      }
      ```
    - Update `list_methods`, `describe_method`, and `list_errors_and_limits` to read from `raw_schema()`.
    - Implement metadata lookup helpers on `&'static Value`:
      - `pub fn method_spec(method: &str) -> Option<&'static Value>`: returns `SCHEMA.get("x-methods")?.get(method)`.
      - `pub fn method_scope(method: &str) -> Option<&'static str>`: returns `method_spec(method)?.get("scope")?.as_str()` (`"collection"` or `"application"`).
      - `pub fn method_completion(method: &str) -> Option<&'static str>`: returns `method_spec(method)?.get("completion")?.as_str()` (`"completed"` or `"operation"`).
      - `pub fn all_method_names() -> Vec<&'static str>`: returns all method names under `x-methods` in deterministic order.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **2.2 Implement client-side parameter validation `validate_method_params` in `src/schema.rs`**
  - **File(s)**:
    - `src/schema.rs`
  - **Scope**:
    - Implement `pub fn validate_method_params(method: &str, params: &Value) -> Result<(), CliError>`:
      - Look up `spec = method_spec(method).ok_or_else(|| CliError::SchemaNotFound(format!("Unknown method '{method}'")))?`.
      - Resolve `params_schema = spec.get("params")` against `SCHEMA.get("$defs")` (following `$ref: "#/$defs/<Name>"`).
      - Implement recursive JSON Schema validation helper `validate_value_against_schema(value: &Value, schema_node: &Value, defs: &Map<String, Value>, path: &str) -> Result<(), CliError>` supporting the JSON Schema constructs used by `api-v1.schema.json`:
        - `$ref`: strip `"#/$defs/"`, look up target in `defs`, and recurse.
        - `oneOf` / `anyOf`: succeed if at least one branch matches (for `oneOf`, exactly one branch matches); on failure return a descriptive `CliError::InvalidInput` with `path`.
        - `allOf`: require all branches to match.
        - `const`: require `value == const_val`.
        - `enum`: require `enum_arr.contains(value)`.
        - `type`:
          - `"object"`: require `value.is_object()`; check all `required` keys exist; check `minProperties` if present; for each `(k, v)`, if `properties` has `k`, recurse on `properties[k]` with path `"{path}.{k}"`, else if `additionalProperties == Some(Value::Bool(false))` return `CliError::InvalidInput(format!("{path}: unknown property '{k}' is not allowed"))`, else if `additionalProperties` is a schema object recurse on it.
          - `"array"`: require `value.is_array()`; check `minItems` and `maxItems` if present; if `items` is a schema object, validate each element with path `"{path}[{idx}]"`.
          - `"string"`: require `value.is_string()`; check `minLength` if present; if `schema_node` is `EntityId` (or has `"pattern": "^[1-9][0-9]{0,18}$"`), verify the string is a positive decimal `i64` (`s.parse::<i64>().is_ok_and(|n| n > 0)` and no leading zero).
          - `"integer"`: require `value.is_i64() || value.is_u64()`; check `minimum` and `maximum` if present.
          - `"number"`: require `value.is_number()`; check `minimum` and `maximum` if present.
          - `"boolean"`: require `value.is_boolean()`.
          - `"null"`: require `value.is_null()`.
      - All validation errors return `CliError::InvalidInput(format!("Schema validation failed for '{method}' ({path}): {reason}"))` (exit code `2`) except an unknown method name which returns `CliError::SchemaNotFound(...)` (also exit code `2`).
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **2.3 Add unit tests for schema metadata and parameter validation in `src/schema.rs`**
  - **File(s)**:
    - `src/schema.rs`
  - **Scope**:
    - Add unit tests in `src/schema.rs` covering:
      - `all_method_names()` returns all 96 methods in `schemas/api-v1.schema.json`.
      - `method_scope`: returns `Some("collection")` for `"decks.list"` and `"notes.create"`, `Some("application")` for `"operations.get"`, `"api.describe"`, `"api.execute"`, `"profiles.list"`, and `None` for `"unknown.method"`.
      - `method_completion`: returns `Some("operation")` for `"collection.sync"`, `"api.execute"`, and `"profiles.switch"`, and `Some("completed")` for `"decks.list"` and `"operations.get"`.
      - `validate_method_params` accepts valid params for `"notes.create"`, `"decks.get"` (by `name` and by `id`), `"operations.get"`, and `"api.describe"`.
      - `validate_method_params` rejects:
        - Unknown method (`"nonexistent.method"` -> `CliError::SchemaNotFound`).
        - Missing required field (e.g. `"notes.get"` with `{}` -> `CliError::InvalidInput`).
        - `additionalProperties: false` violation (e.g. `"operations.get"` with `{"operationId": "op-1", "session": "s-1"}` -> `CliError::InvalidInput`).
        - Invalid `EntityId` format (e.g. `"notes.get"` with `{"id": "0"}` or `{"id": "abc"}` -> `CliError::InvalidInput`).
        - Invalid `Reference` `oneOf` (e.g. `"decks.get"` with `{"deck": {"id": "1", "name": "Default"}}` or `{"deck": {}}` -> `CliError::InvalidInput`).
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 3 — Scope-Aware `AnkiClient`, Configurable Polling & Batch Support

- [x] **3.1 Make `AnkiClient::with_session` scope-aware and add pre-flight schema validation in `src/client.rs`**
  - **File(s)**:
    - `src/client.rs`
    - `src/main.rs`
  - **Scope**:
    - Add `validate: bool` field to `AnkiClient` (default `true` in `AnkiClient::new`), plus builder/setter `pub fn with_validation(mut self, validate: bool) -> Self` and getter `pub fn session(&self) -> Option<&str>`.
    - Update `src/main.rs` to chain `.with_validation(!cli.no_validate)` when constructing `AnkiClient`.
    - Change `with_session(&self, params: Value) -> Value` to `pub fn prepare_params(&self, method: &str, params: Value) -> Result<Value, CliError>`:
      - Only inject `"session"` into `params` when `self.session.is_some()` **and** `schema::method_scope(method) == Some("collection")`.
      - If `self.validate` is `true`, call `schema::validate_method_params(method, &params)?` on the prepared `params` before any network request is made.
      - Return the prepared `params`.
    - Update `AnkiClient::call(&self, method: &str, params: Value)` to call `let params = self.prepare_params(method, params)?;`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **3.2 Add configurable polling delays and scope-aware session unit tests in `src/client.rs`**
  - **File(s)**:
    - `src/client.rs`
  - **Scope**:
    - Add `poll_initial_delay: Duration` (default `Duration::from_millis(250)`) and `poll_max_delay: Duration` (default `Duration::from_secs(2)`) fields to `AnkiClient`.
    - Add builder method `pub fn with_poll_delays(mut self, initial: Duration, max: Duration) -> Self` so mock HTTP integration tests can configure millisecond polling (`Duration::from_millis(5)`).
    - Update `AnkiClient::wait_for_operation` to use `self.poll_initial_delay` and `self.poll_max_delay`.
    - Add unit tests in `src/client.rs` verifying:
      - When `session` is `Some("sess-123".into())`:
        - `client.prepare_params("decks.list", json!({}))` returns `Ok(json!({"session": "sess-123"}))`.
        - `client.prepare_params("operations.get", json!({"operationId": "op-1"}))` returns `Ok(json!({"operationId": "op-1"}))` (no `"session"` injected, passes `operations_get_Params` schema validation).
        - `client.prepare_params("api.describe", json!({}))` returns `Ok(json!({}))`.
        - Explicit `"session"` already present in `params` for a collection-scoped method is preserved and not overwritten.
      - When `validate` is `true`, `prepare_params` rejects invalid parameters before making any HTTP call; when `with_validation(false)` is set, `prepare_params` skips schema validation.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **3.3 Implement `execute_batch` (`api.execute`) and `transport_batch` (JSON-RPC array batch) on `AnkiClient`**
  - **File(s)**:
    - `src/client.rs`
  - **Scope**:
    - Implement `pub fn execute_batch(&self, commands: Vec<(String, Value)>, wait: bool, timeout_secs: u64) -> Result<Value, CliError>`:
      - For each `(method, params)` in `commands`:
        - Call `let prepared = self.prepare_params(&method, params)?;` so collection-scoped inner commands get `"session"` injected and each inner command is validated against its method schema when `self.validate` is `true`.
        - Build `json!({ "method": method, "params": prepared })`.
      - Call `self.call_and_maybe_wait("api.execute", json!({ "commands": items }), wait, timeout_secs)`.
    - Implement `pub fn transport_batch(&self, requests: Vec<(String, Value)>) -> Result<Value, CliError>`:
      - If `requests.is_empty()` or `requests.len() > 100`, return `Err(CliError::InvalidInput("Transport batch must contain between 1 and 100 requests".into()))`.
      - Build a JSON-RPC 2.0 request array where each element has `"jsonrpc": "2.0"`, a unique `"id"`, `"method"`, and `"params": self.prepare_params(&method, params)?`.
      - POST the JSON array to `self.url` with optional `Authorization: Bearer <key>` header, check HTTP status, and return the parsed JSON array response (`TransportBatchResponse`).
    - Add unit tests in `src/client.rs` testing ` transport_batch` size validation (empty or >100 rejected with exit code 2) and inner command preparation for `execute_batch`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 4 — `RpcPlan` Separation & Escape Hatch Subcommands

- [x] **4.1 Define `RpcPlan` and split `dispatch` into `to_rpc_plan` + `execute_plan` in `src/commands/mod.rs`**
  - **File(s)**:
    - `src/commands/mod.rs`
    - `src/lib.rs`
  - **Scope**:
    - Define `BatchMode` and `RpcPlan` in `src/commands/mod.rs` (and re-export in `src/lib.rs`):
      ```rust
      use clap::ValueEnum;

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
      ```
    - Refactor `dispatch` into:
      - `pub fn to_rpc_plan(cmd: &Commands) -> Result<RpcPlan, CliError>`: pure function (aside from reading stdin/file when explicitly requested by `--stdin` or `--file`) that translates any `Commands` variant into an `RpcPlan`.
      - `pub fn execute_plan(client: &AnkiClient, plan: RpcPlan) -> Result<Value, CliError>`: executes `RpcPlan` against `AnkiClient` (`Local(v) => Ok(v)`, `Call { .. } => client.call_and_maybe_wait(..)`, `Paginate { .. } => client.paginate_all(..)`, `WaitOperation { .. } => client.wait_for_operation(..)`, `Batch { .. } => match mode { Execute => client.execute_batch(..), Transport => client.transport_batch(..) }`).
      - Keep `pub fn dispatch(client: &AnkiClient, cmd: Commands) -> Result<Value, CliError>` as `let plan = to_rpc_plan(&cmd)?; execute_plan(client, plan)`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **4.2 Upgrade YAML/JSON input parsing (`--params`, `--fields`) and add `batch` subcommand in `src/commands/mod.rs`**
  - **File(s)**:
    - `src/commands/mod.rs`
  - **Scope**:
    - Upgrade `parse_params` and a new helper `pub fn parse_structured_object(raw: &str, flag_name: &str) -> Result<Value, CliError>` to parse inputs using `serde_norway::from_str::<Value>(trimmed)`:
      - Because YAML 1.2 is a superset of JSON, `serde_norway::from_str` seamlessly parses both JSON (`'{"Front":"Q","Back":"A"}'`) and YAML (`'Front: Q\nBack: A'` or `'deck: {name: Default}'`).
      - Verify that the parsed `Value` is a `Value::Object`, returning `CliError::InvalidInput(format!("{flag_name} must be a YAML or JSON object"))` otherwise.
    - Update `NotesCmd::Create`, `NotesCmd::Validate`, and `NotesCmd::Update` to use `parse_structured_object(&fields, "--fields")?`.
    - Add `Batch(BatchArgs)` variant to `Commands`:
      ```rust
      #[derive(Args)]
      pub struct BatchArgs {
          /// Path to a YAML or JSON file containing an array of {method, params} objects
          #[arg(long, short = 'f', conflicts_with = "stdin")]
          pub file: Option<String>,

          /// Read YAML or JSON array of {method, params} objects from stdin
          #[arg(long, conflicts_with = "file")]
          pub stdin: bool,

          /// Inline YAML or JSON array of {method, params} objects
          #[arg(long, short = 'c', conflicts_with_all = ["file", "stdin"])]
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
      ```
    - Implement `parse_batch_requests(args: &BatchArgs) -> Result<Vec<(String, Value)>, CliError>`:
      - Read raw text from `args.commands`, `args.file` (`std::fs::read_to_string`), or `args.stdin`; error with `CliError::InvalidInput` if none is provided.
      - Parse with `serde_norway::from_str::<Value>` and require a non-empty array where each item is an object with `"method"` (non-empty string) and optional `"params"` (object, defaulting to `json!({})`).
      - Map `Commands::Batch(args)` in `to_rpc_plan` to `RpcPlan::Batch { mode: args.mode, requests, wait: !args.no_wait, timeout_secs: args.timeout }`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **4.3 Add unit tests for `to_rpc_plan` and YAML/JSON input parsing in `src/commands/mod.rs`**
  - **File(s)**:
    - `src/commands/mod.rs`
  - **Scope**:
    - Add unit tests in `src/commands/mod.rs` verifying `Cli::try_parse_from` + `to_rpc_plan`:
      - `call decks.list` -> `RpcPlan::Call { method: "decks.list", params: json!({}), wait: true, timeout_secs: 300 }`.
      - `call notes.create --params "deck: {name: Default}\nnoteType: {name: Basic}\nfields: {Front: Q, Back: A}"` (YAML params) -> `RpcPlan::Call` with matching JSON object.
      - `call collection.sync --no-wait --timeout 45` -> `RpcPlan::Call { method: "collection.sync", params: json!({}), wait: false, timeout_secs: 45 }`.
      - `batch --commands "- method: decks.list\n- method: tags.list\n  params: {}"` -> `RpcPlan::Batch` with `BatchMode::Execute` and 2 requests.
      - `batch --mode transport --commands '[{"method":"api.describe"}]'` -> `RpcPlan::Batch` with `BatchMode::Transport`.
      - `schema`, `schema notes.create`, and `schema --errors` -> `RpcPlan::Local(...)`.
      - `describe` -> `RpcPlan::Call { method: "api.describe", params: json!({}), .. }`.
      - Existing domain subcommands (`collection`, `decks`, `notes` including YAML `--fields`, `cards`, `operations`) produce valid `RpcPlan` variants whose `(method, params)` pass `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

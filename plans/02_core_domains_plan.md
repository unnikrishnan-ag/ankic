# Plan 2: Core Domains & Command Modularization (`collection`, `operations`, `decks`, `deckPresets`, `notes`, `cards`, `tags`, `noteTypes`)

## Goal
Modularize `src/commands/` into clean domain files and expand the core Anki domain subcommands (`collection`, `operations`, `decks`, `deck-presets`, `notes`, `cards`, `tags`, `note-types`) to achieve 100% method coverage across all 51 JSON-RPC methods in those 8 namespaces. Every subcommand variant translates deterministically via `to_rpc_plan` into schema-compliant `(method, params)` payloads with YAML/JSON input support and unit test coverage against `schemas/api-v1.schema.json`.

### Implementation Status
- [x] **Phase 1 — Command Modularization & Collection, Operations, Decks, Deck Presets (`1.1`–`1.3`)**
- [x] **Phase 2 — Complete `notes` Domain (All 12 Methods) (`2.1`–`2.3`)**
- [ ] **Phase 3 — Complete `cards` & `tags` Domains (12 Methods) (`3.1`–`3.2`)**
- [ ] **Phase 4 — Complete `note-types` Domain (All 8 Methods) (`4.1`–`4.2`)**

### Handoff Context (Updated After Each Task)
> **Next task**: `3.1` (Extract & Complete `cards` Domain (`src/commands/cards.rs`))
>
> **Context from completed tasks**:
> - **1.1**: Created `src/commands/helpers.rs` (`call_plan`, `make_ref`, `make_refs`, `make_sort`, `parse_yaml_or_json_value`, `parse_yaml_or_json_object`, `parse_yaml_or_json_array`, `read_input_or_stdin`), `src/commands/collection.rs` (`CollectionCmd`, `to_rpc_plan` covering `collection.get`, `collection.sync`, `collection.refresh`, `collection.checkIntegrity`, `collection.undo`), and `src/commands/operations.rs` (`OperationsCmd`, `to_rpc_plan` covering `operations.get`, `operations.cancel`, `RpcPlan::WaitOperation`). Wired `helpers`, `collection`, and `operations` into `src/commands/mod.rs` with `pub use collection::CollectionCmd; pub use operations::OperationsCmd;`.
> - **1.2**: Created `src/commands/decks.rs` (`DecksCmd`, `to_rpc_plan` covering `decks.list`, `decks.get`, `decks.create`, `decks.update`, `decks.delete` with single `--name`/`--id` or multi-deck `--names`/`--ids` and `--delete-cards` enforcement, and `decks.getCounts` defaulting `includeChildren: true` with `--no-children` and `--include-children`). Wired `pub mod decks; pub use decks::DecksCmd;` into `src/commands/mod.rs` and removed the now-unused `make_ref` helper from `src/commands/mod.rs`.
> - **1.3**: Created `src/commands/deck_presets.rs` (`DeckPresetsCmd`, `to_rpc_plan` covering `deckPresets.list`, `deckPresets.get`, `deckPresets.create` with optional `cloneFrom`, `deckPresets.update` with `anyOf` validation for `--new-name`/`--settings` and async `--no-wait`/`--timeout`, `deckPresets.assign` with repeatable `--deck-name`/`--deck-id`, and `deckPresets.delete` with optional `replacement`). Wired `pub mod deck_presets; pub use deck_presets::DeckPresetsCmd;` and `Commands::DeckPresets` (`deck-presets`) into `src/commands/mod.rs`.
> - **2.1**: Created `src/commands/notes.rs` (`NotesCmd`, `to_rpc_plan`) covering `notes.get`, `notes.getMany` (`get-many`), `notes.search` (with `--sort-by`/`--sort-dir` and `--all` pagination), `notes.findEmpty` (`find-empty` with `--criterion`, optional `--query`, and `--all` pagination), `notes.validate` (wrapping `NoteInput` under `"note"`, supporting `--deck`/`--deck-id`, `--note-type`/`--note-type-id`, `--allow-duplicates`, `--reject-empty-questions`), and `notes.validateMany` (`validate-many` with `--notes`/`--items` or `--stdin`), while preserving existing `Create`/`Update`/`Delete` variants. Wired `pub mod notes; pub use notes::NotesCmd;` into `src/commands/mod.rs`.
> - **2.2**: Enhanced `NotesCmd::Create` (`--deck`/`--deck-id`, `--note-type`/`--note-type-id`, YAML/JSON `--fields`, `--tag`, `--allow-duplicates`, `--duplicate-scope`, `--duplicate-deck`/`--duplicate-deck-id`, `--reject-empty-questions`), added `NotesCmd::CreateMany` (`create-many` with `--notes`/`--items` or `--stdin`, `--atomic`), and added shared `build_note_input` helper in `src/commands/notes.rs` alongside unit tests for `notes.create`, `notes.createMany`, `notes.update`, and `notes.delete`.
> - **2.3**: Added `NotesCmd::PlanConversion` (`plan-conversion` -> `notes.planConversion` with `--source-name`/`--source-id` and `--target-name`/`--target-id`) and `NotesCmd::Convert` (`convert` -> `notes.convert` with positional `ids`, `--target-name`/`--target-id`, YAML/JSON `--fields` and `--templates` arrays of `ConversionMapping`, and `--plan-token`) in `src/commands/notes.rs`, plus unit tests validating both methods against `schemas/api-v1.schema.json`.

> [!IMPORTANT]
> ### Autonomous Execution Protocol: `"Continue with next task @plans/02_core_domains_plan.md"`
> When the user says **"Continue with next task @plans/02_core_domains_plan.md"** (or similar):
> 1. **Read Context & Find Next Task**: Read **Implementation Status**, **Handoff Context**, and **Architectural Constraints & Verification**, then scan **Build Order & Tasks** for the **first unchecked task** (`- [ ] **<Task ID>**`).
> 2. **Execute ONLY That One Task**: Implement only the files and scope listed for that single task. Do NOT jump ahead to subsequent tasks.
> 3. **Verify**: Run the verification commands listed in the task / **Architectural Constraints & Verification** and confirm all checks pass.
> 4. **Update Plan & Handoff Context**:
>    - Check off `- [x] **<Task ID>**` in **Build Order & Tasks** (and check off the Phase in **Implementation Status** when all tasks in that phase are complete).
>    - Update **Handoff Context** with the new `Next task` (or `None` when complete) and a concise bullet summarizing what was implemented (exact file paths, symbols, signatures, keys, and caveats).
> 5. **Stop & Report**: Briefly summarize what was done, show verification results, explicitly announce the next pending task, and stop.

## Architectural Constraints & Verification
- **Constraints**:
  - **Prerequisite Foundation (from Plan 1)**: Assumes `src/lib.rs`, `RpcPlan` (`RpcPlan::Local`, `RpcPlan::Call`, `RpcPlan::Paginate`, `RpcPlan::WaitOperation`, `RpcPlan::Batch`), `to_rpc_plan(cmd: &Commands) -> Result<RpcPlan, CliError>`, and `schema::validate_method_params(method: &str, params: &Value) -> Result<(), CliError>` are in place.
  - **Pure Command-to-RPC Planning**: Domain modules (`src/commands/*.rs`) must expose a pure `pub fn to_rpc_plan(&self) -> Result<RpcPlan, CliError>` (or `to_rpc_plan(cmd: &<Domain>Cmd) -> Result<RpcPlan, CliError>`) that builds the `RpcPlan` without performing network I/O, so unit and contract tests can validate every subcommand offline.
  - **Exact Schema Compliance**: Every method's parameter object must match `#/$defs/<method>_Params` in `schemas/api-v1.schema.json` (where `additionalProperties: false` is enforced across all method parameter schemas). Optional parameters (`cursor`, `sort`, `cloneFrom`, `replacement`, etc.) must be omitted from the JSON map when `None`, never serialized as `null` unless the schema explicitly allows `"type": "null"`.
  - **YAML & JSON Input Compatibility**: All structured CLI flag or `stdin` inputs (`--fields`, `--settings`, `--items`, `--notes`, `--patch`, `--edits`, `--templates`, `--spec`) must parse via `serde_norway::from_str` so agents can pass either YAML or JSON seamlessly.
- **Verification**: `cargo test --all-targets` and `cargo clippy --all-targets -- -D warnings` (must pass after every task).

---

## Build Order & Tasks

### Phase 1 — Command Modularization & Collection, Operations, Decks, Deck Presets

- [x] **1.1 Shared Command Helpers & `collection` / `operations` Modules**
  - **File(s)**:
    - `src/commands/helpers.rs` (new)
    - `src/commands/collection.rs` (new)
    - `src/commands/operations.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/helpers.rs` with reusable parameter builders and YAML/JSON input parsers:
      - `pub fn make_ref(name: Option<&str>, id: Option<&str>, label: &str) -> Result<Value, CliError>`: Validates that exactly one of `name` (`{"name": n}`) or `id` (`{"id": i}`) is provided, returning `CliError::InvalidInput` with a message naming `label` (e.g. `"Specify exactly one of --name or --id"`).
      - `pub fn make_refs(names: &[String], ids: &[String], label: &str) -> Result<Vec<Value>, CliError>`: Builds a non-empty list of `#/$defs/Reference` objects from repeatable name/id flags, returning `CliError::InvalidInput` if empty.
      - `pub fn make_sort(sort_by: Option<&str>, sort_dir: Option<&str>) -> Result<Option<Value>, CliError>`: Builds `Some(json!({ "column": col, "direction": dir }))` conforming to `#/$defs/Sort` (`direction` in `["ascending", "descending"]`, default `"ascending"` when `--sort-by` is set).
      - `pub fn parse_yaml_or_json_value(raw: &str, flag_name: &str) -> Result<Value, CliError>`: Parses YAML or JSON into `serde_json::Value` using `serde_norway::from_str`.
      - `pub fn parse_yaml_or_json_object(raw: &str, flag_name: &str) -> Result<Map<String, Value>, CliError>`: Parses YAML or JSON and verifies `.is_object()`.
      - `pub fn parse_yaml_or_json_array(raw: &str, flag_name: &str) -> Result<Vec<Value>, CliError>`: Parses YAML or JSON and verifies `.is_array()`.
      - `pub fn read_input_or_stdin(inline: Option<&str>, use_stdin: bool, flag_name: &str) -> Result<String, CliError>`: Reads from `inline` string or `io::stdin()`.
    - Extract `CollectionCmd` to `src/commands/collection.rs` with `pub fn to_rpc_plan(cmd: &CollectionCmd) -> Result<RpcPlan, CliError>` covering all 5 `collection.*` methods:
      - `Get` -> `collection.get` (`completion: "completed"`, params: `{}`)
      - `Sync { include_media, no_wait, timeout }` -> `collection.sync` (`completion: "operation"`, params: `{"includeMedia": include_media}`, `wait: !no_wait`, `timeout`)
      - `Refresh { target }` -> `collection.refresh` (`completion: "completed"`, params: `{"target": target}` where `target` is `"windows"` or `"cachesAndWindows"`, default `"windows"`)
      - `CheckIntegrity { no_wait, timeout }` -> `collection.checkIntegrity` (`completion: "operation"`, params: `{}`, `wait: !no_wait`, `timeout`)
      - `Undo { no_wait, timeout }` -> `collection.undo` (`completion: "operation"`, params: `{}`, `wait: !no_wait`, `timeout`)
    - Extract `OperationsCmd` to `src/commands/operations.rs` with `pub fn to_rpc_plan(cmd: &OperationsCmd) -> Result<RpcPlan, CliError>` covering:
      - `Get { operation_id }` -> `operations.get` (`scope: "application"`, `completion: "completed"`, params: `{"operationId": operation_id}`)
      - `Cancel { operation_id }` -> `operations.cancel` (`scope: "application"`, `completion: "completed"`, params: `{"operationId": operation_id}`)
      - `Wait { operation_id, timeout }` -> `RpcPlan::WaitOperation { operation_id, timeout }`
    - Wire `pub mod helpers; pub mod collection; pub mod operations;` into `src/commands/mod.rs`.
    - Add unit tests in `helpers.rs`, `collection.rs`, and `operations.rs` asserting exact `RpcPlan` output and validating every generated `(method, params)` against `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **1.2 Extract & Enhance `decks` Domain (`src/commands/decks.rs`)**
  - **File(s)**:
    - `src/commands/decks.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Move `DecksCmd` into `src/commands/decks.rs` and implement `pub fn to_rpc_plan(cmd: &DecksCmd) -> Result<RpcPlan, CliError>` covering all 6 `decks.*` methods (`scope: "collection"`, `completion: "completed"`):
      - `List` -> `decks.list`, params `{}`.
      - `Get { name, id }` -> `decks.get`, params `{"deck": make_ref(name, id, "deck")?}`.
      - `Create { name }` -> `decks.create`, params `{"name": name}`.
      - `Update { name, id, new_name }` -> `decks.update`, params `{"deck": make_ref(name, id, "deck")?, "name": new_name}`.
      - `Delete { name, id, names, ids, delete_cards }` -> `decks.delete`, params `{"decks": [...], "deleteCards": true}`:
        - Support single `--name` / `--id` OR repeatable `--names` / `--ids` so one or many decks can be deleted in a single call.
        - Enforce `--delete-cards` (`delete_cards == true`); if `false`, return `CliError::InvalidInput("decks.delete requires --delete-cards to confirm card deletion".into())` because `decks_delete_Params` defines `"deleteCards": { "const": true }`.
      - `Counts { name, id, include_children, no_children }` -> `decks.getCounts`, params `{"deck": make_ref(name, id, "deck")?, "includeChildren": !no_children}` (default `includeChildren: true` matching schema default, while keeping `--include-children` accepted for backwards compatibility).
    - Wire `pub mod decks;` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/decks.rs` testing all 6 `decks.*` methods (single and multi-deck delete, missing `--delete-cards` error, reference by name vs id) and validating params via `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **1.3 Implement `deck-presets` Domain (`src/commands/deck_presets.rs`)**
  - **File(s)**:
    - `src/commands/deck_presets.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/deck_presets.rs` defining `DeckPresetsCmd` (mounted on `Commands::DeckPresets` as subcommand `deck-presets`) and `pub fn to_rpc_plan(cmd: &DeckPresetsCmd) -> Result<RpcPlan, CliError>` covering all 6 `deckPresets.*` methods:
      - `List` -> `deckPresets.list` (`completion: "completed"`, params: `{}`).
      - `Get { name, id }` -> `deckPresets.get` (`completion: "completed"`, params: `{"preset": make_ref(name, id, "preset")?}`).
      - `Create { name, clone_from_name, clone_from_id }` -> `deckPresets.create` (`completion: "completed"`, params: `{"name": name}` plus optional `"cloneFrom": Reference` when `--clone-from-name` or `--clone-from-id` is given).
      - `Update { name, id, new_name, settings, reschedule_fsrs, no_wait, timeout }` -> `deckPresets.update` (`completion: "operation"`, `wait: !no_wait`, `timeout` default `300`):
        - Builds `"preset": make_ref(name, id, "preset")?`.
        - Inserts `"name": new_name` if `--new-name` is provided.
        - Parses `--settings` (YAML/JSON `#/$defs/PresetPatch` object) and inserts `"settings"` if provided.
        - Enforces schema `anyOf` requirement that at least one of `--new-name` or `--settings` is provided (returning `CliError::InvalidInput` otherwise).
        - If `--reschedule-fsrs` is set, inserts `"rescheduleFsrs": true`.
      - `Assign { preset_name, preset_id, deck_names, deck_ids, reschedule_fsrs, no_wait, timeout }` -> `deckPresets.assign` (`completion: "operation"`, `wait: !no_wait`, `timeout` default `300`):
        - Builds `"preset": make_ref(preset_name, preset_id, "preset")?` and `"decks": make_refs(&deck_names, &deck_ids, "deck")?` (via repeatable `--deck-name` / `--deck-id`).
        - Inserts `"rescheduleFsrs": reschedule_fsrs` when set.
      - `Delete { name, id, replacement_name, replacement_id, no_wait, timeout }` -> `deckPresets.delete` (`completion: "operation"`, `wait: !no_wait`, `timeout` default `300`):
        - Builds `"preset": make_ref(name, id, "preset")?` plus optional `"replacement": Reference` if `--replacement-name` or `--replacement-id` is passed.
    - Wire `Commands::DeckPresets` in `src/commands/mod.rs`.
    - Add unit tests in `src/commands/deck_presets.rs` covering all 6 `deckPresets.*` methods and validating generated params with `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 2 — Complete `notes` Domain (All 12 Methods)

- [x] **2.1 Extract `notes` Domain: Read & Validation Methods (`get`, `get-many`, `search`, `find-empty`, `validate`, `validate-many`)**
  - **File(s)**:
    - `src/commands/notes.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Extract `NotesCmd` to `src/commands/notes.rs` and implement `to_rpc_plan` for the 6 read and validation methods (`scope: "collection"`, `completion: "completed"`):
      - `Get { id }` -> `notes.get`, params `{"id": id}`.
      - `GetMany { ids }` (subcommand `get-many`) -> `notes.getMany`, params `{"ids": ids}` (`#[arg(required = true)] ids: Vec<String>`).
      - `Search { query, limit, cursor, sort_by, sort_dir, all }` -> `notes.search`, params `{"query": query, "limit": limit, "cursor"?: cursor, "sort"?: Sort}` (`RpcPlan::Paginate { array_key: "ids" }` when `all` is true, else `RpcPlan::Call`).
      - `FindEmpty { criterion, query, limit, cursor, all }` (subcommand `find-empty`) -> `notes.findEmpty`, params `{"criterion": criterion, "query"?: query, "limit": limit, "cursor"?: cursor}` where `criterion` is one of `"allFieldsEmpty"`, `"noCards"`, `"allQuestionsEmpty"` (default `"allFieldsEmpty"` or required flag/positional; supports `--all` auto-pagination via `RpcPlan::Paginate { array_key: "ids" }`).
      - `Validate { deck, deck_id, note_type, note_type_id, fields, tags, allow_duplicates, reject_empty_questions }` -> `notes.validate`:
        - **Bug Fix**: Wrap the `NoteInput` object under the `"note"` key as required by `notes_validate_Params` (`{"note": {"deck": Reference, "noteType": Reference, "fields": fields_obj, "tags": tags, ...}}`).
        - Support `--deck` (by name) or `--deck-id` (by ID), `--note-type` (by name) or `--note-type-id` (by ID), and parse `--fields` as YAML or JSON (`parse_yaml_or_json_object`).
      - `ValidateMany { notes, stdin }` (subcommand `validate-many`) -> `notes.validateMany`, params `{"notes": notes_array}` parsed as YAML/JSON array of `#/$defs/NoteInput` from `--notes` (alias `--items`) or `--stdin`.
    - Add unit tests in `src/commands/notes.rs` for these 6 methods and verify params with `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **2.2 Implement `notes` Mutation Methods (`create`, `create-many`, `update`, `delete`)**
  - **File(s)**:
    - `src/commands/notes.rs` (modify)
  - **Scope**:
    - Implement and enhance the 4 note mutation subcommands in `src/commands/notes.rs` (`scope: "collection"`, `completion: "completed"`):
      - `Create { deck, deck_id, note_type, note_type_id, fields, tags, allow_duplicates, duplicate_scope, duplicate_deck, duplicate_deck_id, reject_empty_questions }` -> `notes.create`:
        - Support `--deck` or `--deck-id` (`Reference`) and `--note-type` or `--note-type-id` (`Reference`).
        - Parse `--fields` as YAML or JSON object (`FieldValues`).
        - Support optional `--allow-duplicates` (builds `"duplicates": {"allow": true, ...}` conforming to `#/$defs/DuplicatePolicy`) and optional `--reject-empty-questions <bool>`.
      - `CreateMany { notes, stdin, atomic }` (subcommand `create-many`) -> `notes.createMany`:
        - Parse `--notes` (alias `--items`) or `--stdin` as a YAML/JSON array of `#/$defs/NoteInput` objects.
        - Include `"atomic": true` when `--atomic` is passed, building `{"notes": notes_array, "atomic": atomic}`.
      - `Update { id, fields, tags }` -> `notes.update`:
        - Parse `--fields` as a YAML/JSON object (`FieldValues`).
        - Require at least one of `--fields` or `--tag` (`minProperties: 1` on `#/$defs/NotePatch`), producing `{"id": id, "patch": patch}`.
      - `Delete { ids }` -> `notes.delete`, params `{"ids": ids}`.
    - Add unit tests in `src/commands/notes.rs` covering `create` (with YAML and JSON `--fields`, deck/note-type by name and ID, duplicate policy), `create-many` (YAML/JSON array + `--atomic`), `update`, and `delete`, validating all generated params via `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **2.3 Implement `notes` Conversion Methods (`plan-conversion`, `convert`)**
  - **File(s)**:
    - `src/commands/notes.rs` (modify)
  - **Scope**:
    - Add `PlanConversion` and `Convert` variants to `NotesCmd` in `src/commands/notes.rs` (`scope: "collection"`, `completion: "completed"`):
      - `PlanConversion { source_name, source_id, target_name, target_id }` (subcommand `plan-conversion`) -> `notes.planConversion`:
        - Builds `{"source": make_ref(source_name, source_id, "source")?, "target": make_ref(target_name, target_id, "target")?}`.
      - `Convert { ids, target_name, target_id, fields, templates, plan_token }` (subcommand `convert`) -> `notes.convert`:
        - Builds `{"ids": ids, "target": make_ref(target_name, target_id, "target")?, "fields": parse_yaml_or_json_array(&fields, "--fields")?, "templates": parse_yaml_or_json_array(&templates, "--templates")?, "planToken": plan_token}` where each item in `fields` and `templates` conforms to `#/$defs/ConversionMapping` (`{"target": "...", "source": "..." | null}`).
    - Add unit tests in `src/commands/notes.rs` verifying `plan-conversion` and `convert` ( ensuring all 12 `notes.*` methods are covered and pass `schema::validate_method_params`).
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 3 — Complete `cards` & `tags` Domains (12 Methods)

- [ ] **3.1 Extract & Complete `cards` Domain (`src/commands/cards.rs`)**
  - **File(s)**:
    - `src/commands/cards.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Extract `CardsCmd` into `src/commands/cards.rs` and implement `pub fn to_rpc_plan(cmd: &CardsCmd) -> Result<RpcPlan, CliError>` covering all 7 `cards.*` methods (`scope: "collection"`, `completion: "completed"`):
      - `Get { id }` -> `cards.get`, params `{"id": id}`.
      - `GetMany { ids }` (subcommand `get-many`) -> `cards.getMany`, params `{"ids": ids}` (`#[arg(required = true)] ids: Vec<String>`).
      - `Search { query, limit, cursor, sort_by, sort_dir, all }` -> `cards.search`, params `{"query": query, "limit": limit, "cursor"?: cursor, "sort"?: Sort}` (`RpcPlan::Paginate { array_key: "ids" }` when `all` is true).
      - `Render { id }` (subcommand `render`) -> `cards.render`, params `{"id": id}`.
      - `Move { deck, deck_id, ids }` -> `cards.move`, params `{"ids": ids, "deck": make_ref(deck, deck_id, "deck")?}` (supports `--deck` by name or `--deck-id` by ID).
      - `Suspend { ids }` -> `cards.setSuspended`, params `{"ids": ids, "suspended": true}`.
      - `Unsuspend { ids }` -> `cards.setSuspended`, params `{"ids": ids, "suspended": false}`.
      - `Flags { flag, ids }` -> `cards.setFlags`, params `{"ids": ids, "flag": flag}` (validates `flag <= 7`, returning `CliError::InvalidInput` if `flag > 7`).
    - Wire `pub mod cards;` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/cards.rs` covering all 7 `cards.*` methods (including `get-many`, `render`, `move` with `--deck` and `--deck-id`, and `flags` out-of-range check) and validating generated params with `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [ ] **3.2 Implement `tags` Domain (`src/commands/tags.rs`)**
  - **File(s)**:
    - `src/commands/tags.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/tags.rs` defining `TagsCmd` (mounted on `Commands::Tags` as subcommand `tags`) and `pub fn to_rpc_plan(cmd: &TagsCmd) -> Result<RpcPlan, CliError>` covering all 5 `tags.*` methods (`scope: "collection"`, `completion: "completed"`):
      - `List` -> `tags.list`, params `{}`.
      - `Add { note_ids, tags }` -> `tags.add`, params `{"noteIds": note_ids, "tags": tags}` (via `--note-id` / `--note-ids` repeatable and `--tag` / `--tags` repeatable; validates both arrays are non-empty).
      - `Remove { note_ids, tags }` -> `tags.remove`, params `{"noteIds": note_ids, "tags": tags}` (via `--note-id` / `--note-ids` repeatable and `--tag` / `--tags` repeatable; validates both arrays are non-empty).
      - `Replace { from, to, note_ids, all_notes }` -> `tags.replace`:
        - Enforces the `tags_replace_Params` `oneOf` rule:
          - If `all_notes` is `true` (and `note_ids` is empty): emits `{"from": from, "to": to, "allNotes": true}` (omitting `"noteIds"`).
          - If `note_ids` is non-empty (and `all_notes` is `false`): emits `{"from": from, "to": to, "noteIds": note_ids}` (omitting `"allNotes"`).
          - Otherwise returns `CliError::InvalidInput("Specify either --note-id(s) or --all-notes (mutually exclusive)".into())`.
      - `DeleteUnused` (subcommand `delete-unused`) -> `tags.deleteUnused`, params `{}`.
    - Wire `Commands::Tags` in `src/commands/mod.rs`.
    - Add unit tests in `src/commands/tags.rs` covering all 5 `tags.*` methods (including both branches of `tags.replace` and mutual-exclusion validation) and validating params with `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 4 — Complete `note-types` Domain (All 8 Methods)

- [ ] **4.1 Implement `note-types` Core Methods (`list`, `get`, `create`, `update`, `template-usage`)**
  - **File(s)**:
    - `src/commands/note_types.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/note_types.rs` defining `NoteTypesCmd` (mounted on `Commands::NoteTypes` as subcommand `note-types`) and `pub fn to_rpc_plan(cmd: &NoteTypesCmd) -> Result<RpcPlan, CliError>` for the first 5 `noteTypes.*` methods (`scope: "collection"`, `completion: "completed"`):
      - `List` -> `noteTypes.list`, params `{}`.
      - `Get { name, id }` -> `noteTypes.get`, params `{"noteType": make_ref(name, id, "noteType")?}`.
      - `Create { name, kind, fields, templates, css, sort_field, latex_header, latex_footer, latex_svg, spec, stdin }` -> `noteTypes.create`:
        - Supports either a full YAML/JSON object via `--spec` / `--stdin`, OR individual flags (`--name`, `--kind` default `"normal"`, `--fields` YAML/JSON array of `#/$defs/FieldConfig` or shorthand field names converted to `{"name": ...}`, `--templates` YAML/JSON array of `#/$defs/TemplateConfig`, plus optional `--css`, `--sort-field`, `--latex-header`, `--latex-footer`, `--latex-svg`).
      - `Update { name, id, new_name, css, sort_field, latex_header, latex_footer, latex_svg, patch }` -> `noteTypes.update`:
        - Builds `"noteType": make_ref(name, id, "noteType")?`.
        - Builds `"patch"` (`#/$defs/NoteTypePatch`, `minProperties: 1`) from `--patch` YAML/JSON object and/or `--new-name` (`"name"`), `--css`, `--sort-field` (`"sortField"`), `--latex-header` (`"latexHeader"`), `--latex-footer` (`"latexFooter"`), `--latex-svg` (`"latexSvg"`). Returns `CliError::InvalidInput` if `patch` is empty.
      - `TemplateUsage { name, id }` (subcommand `template-usage`) -> `noteTypes.getTemplateUsage`, params `{"noteType": make_ref(name, id, "noteType")?}`.
    - Wire `Commands::NoteTypes` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/note_types.rs` for `list`, `get`, `create`, `update`, and `template-usage`, verifying generated params against `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [ ] **4.2 Implement `note-types` Structural & Bulk Methods (`edit-structure`, `replace-text`, `delete-unused`)**
  - **File(s)**:
    - `src/commands/note_types.rs` (modify)
  - **Scope**:
    - Add the remaining 3 `noteTypes.*` subcommands to `NoteTypesCmd` in `src/commands/note_types.rs` (`scope: "collection"`, `completion: "completed"`):
      - `EditStructure { name, id, edits, stdin }` (subcommand `edit-structure`) -> `noteTypes.editStructure`:
        - Builds `"noteType": make_ref(name, id, "noteType")?`.
        - Parses `--edits` or `--stdin` as a non-empty YAML/JSON array of `#/$defs/StructureEdit` objects (`addField`, `updateField`, `moveField`, `removeField`, `addTemplate`, `updateTemplate`, `moveTemplate`, `removeTemplate`), emitting `{"noteType": note_type_ref, "edits": edits_array}`.
      - `ReplaceText { find, replace, names, ids, all_note_types, targets }` (subcommand `replace-text`) -> `noteTypes.replaceText`:
        - Enforces the `noteTypes_replaceText_Params` `oneOf` rule:
          - If `all_note_types` is `true` (and `names`/`ids` are empty): includes `"allNoteTypes": true` and omits `"noteTypes"`.
          - If `names` or `ids` are provided (and `all_note_types` is `false`): includes `"noteTypes": make_refs(&names, &ids, "noteType")?` and omits `"allNoteTypes"`.
          - Otherwise returns `CliError::InvalidInput("Specify either --name/--id or --all-note-types (mutually exclusive)".into())`.
        - If `--target` (`question`, `answer`, `css`) is provided, includes `"targets": targets`.
      - `DeleteUnused { names, ids }` (subcommand `delete-unused`) -> `noteTypes.deleteUnused`:
        - Builds `{"noteTypes": make_refs(&names, &ids, "noteType")?}` from repeatable `--name` / `--id` flags.
    - Add unit tests in `src/commands/note_types.rs` covering `edit-structure`, `replace-text` (both `noteTypes` and `allNoteTypes` modes + error case), and `delete-unused`, confirming all 8 `noteTypes.*` methods pass `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

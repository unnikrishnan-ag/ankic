# Plan 3: Extended Domain Subcommands (`media`, `packages`, `scheduling`, `reviews`, `statistics`, `profiles`, `access`, `advanced`, `gui`)

## Goal
Implement first-class, schema-validated CLI subcommands for all 12 remaining Anki-Connect API v1 namespaces (`media`, `packages`, `scheduling`, `reviews`, `statistics`, `profiles`, `access`, `advanced`, `application`, `browser`, `editors`, and `reviewer` — 43 methods total). Combined with Plans 1 and 2, this brings `anki-cli` to **100% first-class typed CLI subcommand coverage across all 96 API v1 methods and 21 namespaces**, with pure `to_rpc_plan` translation, YAML/JSON input parsing, cursor auto-pagination, and async operation polling.

### Implementation Status
- [x] **Phase 1 — Media & Packages Subcommands (`1.1`–`1.2`)**
- [ ] **Phase 2 — Scheduling & Reviews Subcommands (`2.1`–`2.3`)**
- [ ] **Phase 3 — Statistics, Profiles, Access & Advanced Subcommands (`3.1`–`3.2`)**
- [ ] **Phase 4 — GUI Subcommands (`application`, `browser`, `editors`, `reviewer`) (`4.1`–`4.2`)**

### Handoff Context (Updated After Each Task)
> **Next task**: `2.2` (Add `scheduling` Mutation Subcommands in `src/commands/scheduling.rs`)
>
> **Context from completed tasks**:
> - **Task 1.1 (`src/commands/media.rs`, `src/commands/mod.rs`)**: Implemented `MediaCmd` (`List`, `Get`, `Store`, `Delete`, `Directory`) and `pub fn to_rpc_plan(cmd: &MediaCmd) -> Result<RpcPlan, CliError>` covering all 5 `media.*` methods (`media.list`, `media.get`, `media.store`, `media.delete`, `media.getDirectory`), wired into `Commands::Media` (`pub use media::MediaCmd`). `Store` enforces mutually exclusive `--data` (alias `--data-base64`) / `--path` / `--url`, optional `--collision` (`rename|replace|error`), paired `--skip-hash-alg` (`md5|sha256`) + `--skip-hash-digest`, and `--no-wait`/`--timeout` (`wait: !no_wait`, default `300`). Unit tests validate all 5 methods and error cases against `schema::validate_method_params`.
> - **Task 1.2 (`src/commands/packages.rs`, `src/commands/mod.rs`)**: Implemented `PackagesCmd` (`Import`, `Export`) and `pub fn to_rpc_plan(cmd: &PackagesCmd) -> Result<RpcPlan, CliError>` covering both `packages.*` operation methods (`packages.import`, `packages.export`), wired into `Commands::Packages` (`pub use packages::PackagesCmd`). `Import` takes positional `path` with `--no-wait`/`--timeout` (default `300`); `Export` takes `--name` (aliases `--deck-name`, `--deck`) / `--id` (alias `--deck-id`) via `helpers::make_ref`, `--path`, `--include-scheduling`, and `--no-wait`/`--timeout` (default `300`). Unit tests validate both methods against `schema::validate_method_params` as well as missing deck reference and empty path errors.
> - **Task 2.1 (`src/commands/scheduling.rs`, `src/commands/mod.rs`)**: Implemented `SchedulingCmd` (`Get`, `Queue`, `Predict`, `Due`) and `pub fn to_rpc_plan(cmd: &SchedulingCmd) -> Result<RpcPlan, CliError>` covering the 4 `scheduling.*` inspection methods (`scheduling.get`, `scheduling.getQueue`, `scheduling.predict`, `scheduling.getDue`), wired into `Commands::Scheduling` (`pub use scheduling::SchedulingCmd`). `Queue` validates optional `--fetch-limit` in `1..=1000` and omits `fetchLimit` when `None`; `Due` passes `cardId` and `includeLearnAhead` (`--include-learn-ahead`, default `false`). Unit tests validate all 4 methods and `fetchLimit` bounds against `schema::validate_method_params`.

> [!IMPORTANT]
> ### Autonomous Execution Protocol: `"Continue with next task @plans/03_extended_domains_plan.md"`
> When the user says **"Continue with next task @plans/03_extended_domains_plan.md"** (or similar):
> 1. **Read Context & Find Next Task**: Read **Implementation Status**, **Handoff Context**, and **Architectural Constraints & Verification**, then scan **Build Order & Tasks** for the **first unchecked task** (`- [ ] **<Task ID>**`).
> 2. **Execute ONLY That One Task**: Implement only the files and scope listed for that single task. Do NOT jump ahead to subsequent tasks.
> 3. **Verify**: Run the verification commands listed in the task / **Architectural Constraints & Verification** and confirm all checks pass.
> 4. **Update Plan & Handoff Context**:
>    - Check off `- [x] **<Task ID>**` in **Build Order & Tasks** (and check off the Phase in **Implementation Status** when all tasks in that phase are complete).
>    - Update **Handoff Context** with the new `Next task` (or `None` when complete) and a concise bullet summarizing what was implemented (exact file paths, symbols, signatures, keys, and caveats).
> 5. **Stop & Report**: Briefly summarize what was done, show verification results, explicitly announce the next pending task, and stop.

## Architectural Constraints & Verification
- **Constraints**:
  - **Prerequisite Foundation (from Plans 1 & 2)**: Assumes `RpcPlan` (`RpcPlan::Local`, `RpcPlan::Call`, `RpcPlan::Paginate`, `RpcPlan::WaitOperation`, `RpcPlan::Batch`), `to_rpc_plan(cmd: &Commands) -> Result<RpcPlan, CliError>`, `src/commands/helpers.rs` (`make_ref`, `make_refs`, `make_sort`, `parse_yaml_or_json_value`, `parse_yaml_or_json_object`, `parse_yaml_or_json_array`, `read_input_or_stdin`), and `schema::validate_method_params(method: &str, params: &Value) -> Result<(), CliError>` are in place.
  - **Pure Command-to-RPC Planning**: Every new domain module (`src/commands/*.rs`) must expose a pure `pub fn to_rpc_plan(cmd: &<Domain>Cmd) -> Result<RpcPlan, CliError>` that constructs the `RpcPlan` without performing network I/O so unit and schema contract tests can verify every subcommand offline.
  - **Strict Schema Conformance**: Every method parameter object must conform to `#/$defs/<method>_Params` in `schemas/api-v1.schema.json` (which sets `additionalProperties: false` on every method). Optional fields must be omitted from the JSON map when `None` (never serialized as `null` unless the schema explicitly allows `"type": "null"`), and schema `oneOf` / `not` constraints (such as `media.store` `MediaSource`, `reviews.list` `cardIds` vs `deck`, `browser.select` `cardIds` vs `noteIds`, and `application.navigate` `view` vs `deck`) must be enforced in `to_rpc_plan` with `CliError::InvalidInput`.
  - **Scope & Completion Accuracy**: Methods with `completion: "operation"` (`media.store`, `packages.import`, `packages.export`, `profiles.switch`, `advanced.reviews.import`, `application.navigate`, `application.importFile`, `browser.open`, `browser.select`, `editors.openNote`, `editors.openAdd`, `editors.updateDraft`, `reviewer.answer`) must expose `--no-wait` and `--timeout <secs>` flags and map to `RpcPlan::Call { method, params, wait: !no_wait, timeout }`. Methods with `scope: "application"` (`profiles.*`, `access.requestPermission`, `application.exit`) must never have `"session"` injected into their parameters.
  - **YAML & JSON Input Compatibility**: Structured CLI flag or `stdin` inputs (`--answers`, `--patch`, `--reviews`, `--fields`, `--note`) must parse via `serde_norway::from_str` (`helpers::parse_yaml_or_json_*`) so callers can supply either YAML or JSON.
- **Verification**: `cargo test --all-targets` and `cargo clippy --all-targets -- -D warnings` (must pass after every task).

---

## Build Order & Tasks

### Phase 1 — Media & Packages Subcommands

- [x] **1.1 Implement `media` Domain Subcommands (`src/commands/media.rs`)**
  - **File(s)**:
    - `src/commands/media.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/media.rs` defining `MediaCmd` (mounted on `Commands::Media` as subcommand `media`) and `pub fn to_rpc_plan(cmd: &MediaCmd) -> Result<RpcPlan, CliError>` covering all 5 `media.*` methods (`scope: "collection"`):
      - `List { pattern: Option<String> }` -> `media.list` (`completion: "completed"`):
        - Params: `{}` when `pattern` is `None`, or `{"pattern": pattern}` when provided (`#/$defs/MediaFilename`, schema default `"*"`).
      - `Get { filename: String }` -> `media.get` (`completion: "completed"`):
        - Params: `{"filename": filename}`.
      - `Store { filename: String, data: Option<String>, path: Option<String>, url: Option<String>, collision: Option<String>, skip_hash_alg: Option<String>, skip_hash_digest: Option<String>, no_wait: bool, timeout: u64 }` -> `media.store` (`completion: "operation"`, `wait: !no_wait`, `timeout` default `300`):
        - Support `--data` (with alias `--data-base64`), `--path`, and `--url` as mutually exclusive source flags; validate that **exactly one** is provided, building `"source"` as `{"data": d}`, `{"path": p}`, or `{"url": u}` to match `#/$defs/MediaSource` (`oneOf`). Return `CliError::InvalidInput` if zero or more than one source flag is given.
        - Optional `--collision <rename|replace|error>`: when provided, validate against `["rename", "replace", "error"]` and insert `"collision"`.
        - Optional `--skip-hash-alg <md5|sha256>` and `--skip-hash-digest <digest>`: when both are provided, validate `algorithm` in `["md5", "sha256"]` and insert `"skipIfHash": {"algorithm": alg, "digest": digest}` (`#/$defs/MediaSkip`); if only one of the two flags is provided, return `CliError::InvalidInput`.
      - `Delete { filename: String }` -> `media.delete` (`completion: "completed"`):
        - Params: `{"filename": filename}`.
      - `Directory` -> `media.getDirectory` (`completion: "completed"`):
        - Params: `{}`.
    - Wire `pub mod media;` and `Commands::Media { command: media::MediaCmd }` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/media.rs` covering all 5 `media.*` methods (`store` with `--data`, `--path`, `--url`, `--collision`, `--skip-hash-alg`/`--skip-hash-digest`, and error cases for missing/conflicting source flags) and validating every generated `(method, params)` with `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [x] **1.2 Implement `packages` Domain Subcommands (`src/commands/packages.rs`)**
  - **File(s)**:
    - `src/commands/packages.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/packages.rs` defining `PackagesCmd` (mounted on `Commands::Packages` as subcommand `packages`) and `pub fn to_rpc_plan(cmd: &PackagesCmd) -> Result<RpcPlan, CliError>` covering both `packages.*` methods (`scope: "collection"`, `completion: "operation"`):
      - `Import { path: String, no_wait: bool, timeout: u64 }` -> `packages.import` (`wait: !no_wait`, `timeout` default `300`):
        - Params: `{"path": path}` (`minLength: 1`).
      - `Export { name: Option<String>, id: Option<String>, path: String, include_scheduling: bool, no_wait: bool, timeout: u64 }` -> `packages.export` (`wait: !no_wait`, `timeout` default `300`):
        - Accept `--name` (visible alias `--deck-name` / `--deck`) and `--id` (visible alias `--deck-id`) via `helpers::make_ref(name.as_deref(), id.as_deref(), "deck")?`.
        - Params: `{"deck": deck_ref, "path": path, "includeScheduling": include_scheduling}` (schema requires `"deck"` and `"path"`, with optional `"includeScheduling"` boolean defaulting to `false`).
    - Wire `pub mod packages;` and `Commands::Packages { command: packages::PackagesCmd }` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/packages.rs` testing `import` (with default wait and `--no-wait`), `export` (by deck `--name` and `--id`, with `--include-scheduling`), and missing deck reference validation, asserting `schema::validate_method_params` passes for all valid plans.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 2 — Scheduling & Reviews Subcommands

- [x] **2.1 Implement `scheduling` Inspection Subcommands (`src/commands/scheduling.rs`)**
  - **File(s)**:
    - `src/commands/scheduling.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/scheduling.rs` defining `SchedulingCmd` (mounted on `Commands::Scheduling` as subcommand `scheduling`) and implement `pub fn to_rpc_plan(cmd: &SchedulingCmd) -> Result<RpcPlan, CliError>` for the 4 read/inspection methods in `scheduling.*` (`scope: "collection"`, `completion: "completed"`):
      - `Get { card_id: String }` -> `scheduling.get`:
        - Params: `{"cardId": card_id}` (`#/$defs/EntityId`).
      - `Queue { fetch_limit: Option<u64> }` -> `scheduling.getQueue`:
        - Params: `{}` when `fetch_limit` is `None`, or `{"fetchLimit": limit}` when provided (validating `1 <= limit <= 1000` per `scheduling_getQueue_Params`).
      - `Predict { card_id: String }` -> `scheduling.predict`:
        - Params: `{"cardId": card_id}` (`#/$defs/EntityId`).
      - `Due { card_id: String, include_learn_ahead: bool }` -> `scheduling.getDue`:
        - Params: `{"cardId": card_id, "includeLearnAhead": include_learn_ahead}` (`cardId` required, `includeLearnAhead` default `false`).
    - Wire `pub mod scheduling;` and `Commands::Scheduling { command: scheduling::SchedulingCmd }` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/scheduling.rs` for `get`, `queue`, `predict`, and `due` verified against `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [ ] **2.2 Add `scheduling` Mutation Subcommands (`src/commands/scheduling.rs`)**
  - **File(s)**:
    - `src/commands/scheduling.rs` (modify)
  - **Scope**:
    - Extend `SchedulingCmd` and `to_rpc_plan` in `src/commands/scheduling.rs` with all 5 mutation methods in `scheduling.*` (`scope: "collection"`, `completion: "completed"`):
      - `Answer { card_id: String, rating: String, state_token: String, elapsed_ms: Option<u64> }` -> `scheduling.answer`:
        - Validate `rating` is one of `["again", "hard", "good", "easy"]` (`#/$defs/Rating`).
        - Params: `{"cardId": card_id, "rating": rating, "stateToken": state_token}` plus optional `"elapsedMs": ms` when `elapsed_ms` is `Some(ms)`.
      - `AnswerMany { answers: Option<String>, stdin: bool }` -> `scheduling.answerMany`:
        - Parse `--answers <yaml-or-json>` or `--stdin` via `helpers::read_input_or_stdin` and `helpers::parse_yaml_or_json_array(&raw, "--answers")` (each item conforming to `#/$defs/Answer`: `{"cardId", "rating", "stateToken", optional "elapsedMs"}`).
        - Params: `{"answers": answers_array}`.
      - `Reposition { card_ids: Vec<String>, starting_from: u64, step_size: u64, randomize: bool, shift_existing: bool }` -> `scheduling.repositionNewCards`:
        - Validate `card_ids` is non-empty (`minItems: 1`), `starting_from <= 2_147_483_647`, and `1 <= step_size <= 2_147_483_647` (default `1`).
        - Params: `{"cardIds": card_ids, "startingFrom": starting_from, "stepSize": step_size, "randomize": randomize, "shiftExisting": shift_existing}`.
      - `Reset { card_ids: Vec<String>, no_restore_position: bool, reset_counts: bool, no_log: bool }` -> `scheduling.reset`:
        - Params: `{"cardIds": card_ids, "restorePosition": !no_restore_position, "resetCounts": reset_counts, "log": !no_log}` (matching schema defaults `restorePosition: true`, `resetCounts: false`, `log: true`; also support `--restore-position` / `--log` aliases if desired).
      - `SetDueDate { card_ids: Vec<String>, days: String }` -> `scheduling.setDueDate`:
        - Validate `days` is non-empty (`minLength: 1`, e.g. `"0"`, `"3"`, `"3-7!"`).
        - Params: `{"cardIds": card_ids, "days": days}`.
    - Expand unit tests in `src/commands/scheduling.rs` to cover all 9 `scheduling.*` methods (including YAML and JSON inputs for `answer-many` and invalid rating rejection) and validate every generated `(method, params)` via `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [ ] **2.3 Implement `reviews` Domain Subcommands (`src/commands/reviews.rs`)**
  - **File(s)**:
    - `src/commands/reviews.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/reviews.rs` defining `ReviewsCmd` (mounted on `Commands::Reviews` as subcommand `reviews`) and `pub fn to_rpc_plan(cmd: &ReviewsCmd) -> Result<RpcPlan, CliError>` covering both `reviews.*` methods (`scope: "collection"`, `completion: "completed"`):
      - `List { card_ids: Vec<String>, deck_name: Option<String>, deck_id: Option<String>, include_children: bool, membership: Option<String>, after_id: Option<String>, no_manual: bool, limit: u64, cursor: Option<String>, all: bool }` -> `reviews.list`:
        - Enforce the schema's `"not": { "required": ["cardIds", "deck"] }` rule: if `!card_ids.is_empty()` and (`deck_name.is_some()` || `deck_id.is_some()`), return `CliError::InvalidInput("reviews.list cannot filter by both --card-id and --deck-name/--deck-id".into())`.
        - Validate `1 <= limit <= 1000` (default `100`) and optional `membership` in `["current", "home"]`.
        - Build `params` map with `"limit": limit`, `"includeChildren": include_children`, `"includeManual": !no_manual`, plus optional `"cardIds"` (when non-empty), optional `"deck"` (via `make_ref` when `deck_name` or `deck_id` is set), optional `"membership"`, optional `"afterId"`, and optional `"cursor"`.
        - When `all == true`, return `RpcPlan::Paginate { method: "reviews.list".into(), params, array_key: "items".into() }` (matching `ReviewPage.items` and `ReviewPage.nextCursor`). Otherwise return `RpcPlan::Call { method: "reviews.list".into(), params: Value::Object(params), wait: false, timeout: 0 }`.
      - `LatestId { card_ids: Vec<String>, deck_name: Option<String>, deck_id: Option<String>, membership: Option<String> }` -> `reviews.getLatestId`:
        - Enforce mutual exclusion between `cardIds` and `deck` (`"not": { "required": ["cardIds", "deck"] }`).
        - Build `params` map with optional `"cardIds"` (when non-empty), optional `"deck"` (`make_ref`), and optional `"membership"` (`"current"` | `"home"`).
    - Wire `pub mod reviews;` and `Commands::Reviews { command: reviews::ReviewsCmd }` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/reviews.rs` covering `list` (unfiltered, by `--card-id`, by `--deck-name`, with `--all` returning `RpcPlan::Paginate` on `"items"`, and rejecting simultaneous `--card-id` + `--deck-name`) and `latest-id`, validated via `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 3 — Statistics, Profiles, Access & Advanced Subcommands

- [ ] **3.1 Implement `statistics` & `profiles` Domain Subcommands (`src/commands/statistics.rs`, `src/commands/profiles.rs`)**
  - **File(s)**:
    - `src/commands/statistics.rs` (new)
    - `src/commands/profiles.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/statistics.rs` defining `StatisticsCmd` (mounted on `Commands::Statistics` as subcommand `statistics`) and `pub fn to_rpc_plan(cmd: &StatisticsCmd) -> Result<RpcPlan, CliError>` covering all 4 `statistics.*` methods (`scope: "collection"`, `completion: "completed"`):
      - `Today` -> `statistics.getToday`, params: `{}`.
      - `ReviewDays` -> `statistics.getReviewDays`, params: `{}`.
      - `Summary { name: Option<String>, id: Option<String> }` -> `statistics.getSummary`:
        - Accept optional `--name` (alias `--deck-name`) or `--id` (alias `--deck-id`). When neither is provided, params is `{}`; when one is provided, params is `{"deck": make_ref(name.as_deref(), id.as_deref(), "deck")?}`.
      - `Html { name: Option<String>, id: Option<String> }` -> `statistics.getHtml`:
        - Accept optional `--name` (alias `--deck-name`) or `--id` (alias `--deck-id`). When neither is provided, params is `{}`; when one is provided, params is `{"deck": make_ref(name.as_deref(), id.as_deref(), "deck")?}`.
    - Create `src/commands/profiles.rs` defining `ProfilesCmd` (mounted on `Commands::Profiles` as subcommand `profiles`) and `pub fn to_rpc_plan(cmd: &ProfilesCmd) -> Result<RpcPlan, CliError>` covering all 3 `profiles.*` methods (`scope: "application"`):
      - `List` -> `profiles.list` (`completion: "completed"`), params: `{}`.
      - `Active` -> `profiles.getActive` (`completion: "completed"`), params: `{}`.
      - `Switch { name: String, no_wait: bool, timeout: u64 }` -> `profiles.switch` (`completion: "operation"`, `wait: !no_wait`, `timeout` default `300`), params: `{"name": name}`.
    - Wire `pub mod statistics; pub mod profiles;` and `Commands::Statistics` / `Commands::Profiles` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/statistics.rs` and `src/commands/profiles.rs` covering all 7 methods and validating generated `(method, params)` against `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [ ] **3.2 Implement `access` & `advanced` Domain Subcommands (`src/commands/advanced.rs`)**
  - **File(s)**:
    - `src/commands/advanced.rs` (new)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Create `src/commands/advanced.rs` defining:
      - `AccessCmd` (mounted on `Commands::Access` as subcommand `access`) with `pub fn to_rpc_plan_access(cmd: &AccessCmd) -> Result<RpcPlan, CliError>`:
        - `RequestPermission` -> `access.requestPermission` (`scope: "application"`, `completion: "completed"`), params: `{}`.
      - `AdvancedCmd` (mounted on `Commands::Advanced` as subcommand `advanced`) with `pub fn to_rpc_plan_advanced(cmd: &AdvancedCmd) -> Result<RpcPlan, CliError>` covering both `advanced.*` methods (`scope: "collection"`):
        - `CardsUpdate { card_id: String, patch: Option<String>, interval_days: Option<u64>, sm2_ease_permille: Option<u64>, repetitions: Option<u64>, lapses: Option<u64>, remaining_steps: Option<u64>, fsrs_memory: Option<String>, custom_data: Option<String> }` -> `advanced.cards.update` (`completion: "completed"`):
          - Build `"patch"` object conforming to `#/$defs/RawCardPatch` (`minProperties: 1`, `additionalProperties: false`, allowed keys: `intervalDays`, `sm2EasePermille`, `repetitions`, `lapses`, `remainingSteps`, `fsrsMemory`, `customData`).
          - Support starting from `--patch <yaml-or-json>` (via `helpers::parse_yaml_or_json_object`) and/or merging explicit convenience flags (`--interval-days`, `--sm2-ease-permille`, `--repetitions`, `--lapses`, `--remaining-steps`, `--fsrs-memory` parsed via `parse_yaml_or_json_value`, `--custom-data`).
          - Return `CliError::InvalidInput` if the resulting `patch` object is empty (`minProperties: 1`).
          - Params: `{"cardId": card_id, "patch": patch}`.
        - `ReviewsImport { reviews: Option<String>, stdin: bool, no_wait: bool, timeout: u64 }` -> `advanced.reviews.import` (`completion: "operation"`, `wait: !no_wait`, `timeout` default `300`):
          - Read `--reviews <yaml-or-json>` or `--stdin` via `helpers::read_input_or_stdin` and `helpers::parse_yaml_or_json_array(&raw, "--reviews")` (each item conforming to `#/$defs/ReviewImport`: `id`, `cardId`, `rating`, `interval`, `previousInterval`, `factor`, `elapsedMs`, `kind`).
          - Params: `{"reviews": reviews_array}`.
    - Wire `pub mod advanced;` and `Commands::Access { command: advanced::AccessCmd }` + `Commands::Advanced { command: advanced::AdvancedCmd }` into `src/commands/mod.rs`.
    - Add unit tests in `src/commands/advanced.rs` covering `access request-permission`, `advanced cards-update` (both via `--patch` YAML/JSON and via individual flags, plus empty-patch error), and `advanced reviews-import`, validated against `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

---

### Phase 4 — GUI Subcommands (`application`, `browser`, `editors`, `reviewer`)

- [ ] **4.1 Implement `application`, `browser`, and `editors` Subcommands in `src/commands/gui.rs`**
  - **File(s)**:
    - `src/commands/gui.rs` (new)
  - **Scope**:
    - Create `src/commands/gui.rs` defining `GuiCmd` (to be mounted on `Commands::Gui` as subcommand `gui`) and implement `pub fn to_rpc_plan(cmd: &GuiCmd) -> Result<RpcPlan, CliError>` for the `application.*` (3 methods), `browser.*` (3 methods), and `editors.*` (3 methods) namespaces:
      - **`application.*` methods**:
        - `Navigate { view: String, deck_name: Option<String>, deck_id: Option<String>, no_wait: bool, timeout: u64 }` -> `application.navigate` (`scope: "collection"`, `completion: "operation"`, `wait: !no_wait`, `timeout` default `60`):
          - Validate `view` in `["deckBrowser", "deckOverview", "review"]`.
          - Enforce the schema `oneOf` rule:
            - When `view == "deckBrowser"`, `deck_name` and `deck_id` MUST both be `None` (return `CliError::InvalidInput` if a deck is passed). Params: `{"view": "deckBrowser"}`.
            - When `view` is `"deckOverview"` or `"review"`, `"deck"` IS required via `helpers::make_ref(deck_name.as_deref(), deck_id.as_deref(), "deck")?`. Params: `{"view": view, "deck": deck_ref}`.
        - `ImportFile { path: Option<String>, no_wait: bool, timeout: u64 }` -> `application.importFile` (`scope: "collection"`, `completion: "operation"`, `wait: !no_wait`, `timeout` default `300`):
          - Params: `{}` when `path` is `None`, or `{"path": path}` when provided (`minLength: 1`).
        - `Exit` -> `application.exit` (`scope: "application"`, `completion: "completed"`):
          - Params: `{}`.
      - **`browser.*` methods**:
        - `BrowserOpen { query: Option<String>, sort_by: Option<String>, sort_dir: Option<String>, no_wait: bool, timeout: u64 }` -> `browser.open` (`scope: "collection"`, `completion: "operation"`, `wait: !no_wait`, `timeout` default `60`):
          - Params: optional `"query": q`, optional `"sort": s` via `helpers::make_sort(sort_by.as_deref(), sort_dir.as_deref())?`.
        - `BrowserSelection` -> `browser.getSelection` (`scope: "collection"`, `completion: "completed"`):
          - Params: `{}`.
        - `BrowserSelect { card_ids: Vec<String>, note_ids: Vec<String>, no_wait: bool, timeout: u64 }` -> `browser.select` (`scope: "collection"`, `completion: "operation"`, `wait: !no_wait`, `timeout` default `60`):
          - Enforce schema `oneOf` rule: require **either** non-empty `--card-id` / `card_ids` (`{"cardIds": card_ids}`) **or** non-empty `--note-id` / `note_ids` (`{"noteIds": note_ids}`), returning `CliError::InvalidInput` if neither or both are supplied.
      - **`editors.*` methods**:
        - `EditorOpenNote { note_id: String, no_wait: bool, timeout: u64 }` -> `editors.openNote` (`scope: "collection"`, `completion: "operation"`, `wait: !no_wait`, `timeout` default `60`):
          - Params: `{"noteId": note_id}`.
        - `EditorOpenAdd { note: Option<String>, deck_name: Option<String>, deck_id: Option<String>, note_type_name: Option<String>, note_type_id: Option<String>, fields: Option<String>, tags: Vec<String>, no_wait: bool, timeout: u64 }` -> `editors.openAdd` (`scope: "collection"`, `completion: "operation"`, `wait: !no_wait`, `timeout` default `60`):
          - When no note/deck/note-type flags are provided, params is `{}`.
          - When `--note <yaml-or-json>` is provided, parse via `helpers::parse_yaml_or_json_object(&raw, "--note")` into `{"note": note_obj}`.
          - Otherwise build `"note"` conforming to `#/$defs/DraftInput` requiring both `"deck": make_ref(deck_name, deck_id, "deck")?` and `"noteType": make_ref(note_type_name, note_type_id, "note-type")?`, plus optional `"fields"` (parsed via `parse_yaml_or_json_object`) and optional `"tags"` (when non-empty).
        - `EditorUpdateDraft { editor_id: String, patch: Option<String>, deck_name: Option<String>, deck_id: Option<String>, note_type_name: Option<String>, note_type_id: Option<String>, fields: Option<String>, tags: Option<Vec<String>>, mode: Option<String>, no_wait: bool, timeout: u64 }` -> `editors.updateDraft` (`scope: "collection"`, `completion: "operation"`, `wait: !no_wait`, `timeout` default `60`):
          - Build `"patch"` conforming to `#/$defs/EditorPatch` (`minProperties: 1`, allowed keys: `deck`, `noteType`, `fields`, `tags`) from `--patch <yaml-or-json>` and/or convenience flags (`--deck-name`/`--deck-id`, `--note-type-name`/`--note-type-id`, `--fields`, `--tag`). Return `CliError::InvalidInput` if `"patch"` is empty.
          - Optional `--mode <replace|append>`: validate in `["replace", "append"]` and insert `"mode"` when present.
          - Params: `{"editorId": editor_id, "patch": patch}` (plus optional `"mode"`).
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

- [ ] **4.2 Add `reviewer` Subcommands to `src/commands/gui.rs`, Wire `Gui` into `Commands` & Add Unit Tests**
  - **File(s)**:
    - `src/commands/gui.rs` (modify)
    - `src/commands/mod.rs` (modify)
  - **Scope**:
    - Extend `GuiCmd` and `to_rpc_plan` in `src/commands/gui.rs` with all 6 `reviewer.*` methods (`scope: "collection"`):
      - `ReviewerGet` -> `reviewer.get` (`completion: "completed"`):
        - Params: `{}`.
      - `ReviewerStartTimer { state_token: String }` -> `reviewer.startTimer` (`completion: "completed"`):
        - Params: `{"stateToken": state_token}`.
      - `ReviewerShowQuestion { state_token: String }` -> `reviewer.showQuestion` (`completion: "completed"`):
        - Params: `{"stateToken": state_token}`.
      - `ReviewerShowAnswer { state_token: String }` -> `reviewer.showAnswer` (`completion: "completed"`):
        - Params: `{"stateToken": state_token}`.
      - `ReviewerAnswer { state_token: String, rating: String, no_wait: bool, timeout: u64 }` -> `reviewer.answer` (`completion: "operation"`, `wait: !no_wait`, `timeout` default `60`):
        - Validate `rating` in `["again", "hard", "good", "easy"]` (`#/$defs/Rating`).
        - Params: `{"stateToken": state_token, "rating": rating}`.
      - `ReviewerPlayAudio { state_token: String }` -> `reviewer.playAudio` (`completion: "completed"`):
        - Params: `{"stateToken": state_token}`.
    - Wire `pub mod gui;` and `Commands::Gui { command: gui::GuiCmd }` into `src/commands/mod.rs`.
    - Add comprehensive unit tests in `src/commands/gui.rs` covering all 15 GUI methods (`navigate` across `deckBrowser`/`deckOverview`/`review` + invalid deck combinations, `import-file`, `exit`, `browser-open`, `browser-selection`, `browser-select` with `--card-id` vs `--note-id` + conflict error, `editor-open-note`, `editor-open-add` empty and with draft, `editor-update-draft` + empty-patch error, and all 6 `reviewer-*` subcommands) and validate every generated `(method, params)` against `schema::validate_method_params`.
  - **Verify**: `cargo test --all-targets && cargo clippy --all-targets -- -D warnings`

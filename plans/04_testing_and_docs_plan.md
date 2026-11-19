# Plan 4: Comprehensive Integration/Contract Test Suites & Documentation

## Goal
Implement the three-layer automated integration and contract test suites (`tests/schema_contract_tests.rs`, `tests/mock_http_tests.rs`, and `tests/cli_e2e_tests.rs`) proving 100% conformance of `anki-cli` across all 96 Anki-Connect API v1 methods, scope-aware session handling, pagination, operation polling, batching, YAML/JSON formatting, and deterministic exit codes. Update `SKILL.md` and `README.md` to document YAML-by-default output, YAML/JSON parameter inputs, pre-flight schema validation, batch workflows, and the complete subcommand reference.

### Implementation Status
- [x] **Phase 1 — Schema Contract Test Suite (`tests/schema_contract_tests.rs`) (`1.1`–`1.3`)**
- [ ] **Phase 2 — Mock HTTP Server Integration Tests (`tests/mock_http_tests.rs`) (`2.1`–`2.3`)**
- [ ] **Phase 3 — CLI Binary End-to-End Tests & Documentation (`3.1`–`3.2`)**

### Handoff Context (Updated After Each Task)
> **Next task**: `2.2` (Add Mock HTTP Tests for Cursor Auto-Pagination & Async `OperationHandle` Lifecycle)
>
> **Context from completed tasks**:
> - **1.1**: Created `tests/schema_contract_tests.rs` with `ROOT_SCHEMA` and `METHOD_VALIDATORS` (`jsonschema::draft202012::new` compiled for every `x-methods.<method>.params` and `x-methods.<method>.request` with `$defs`), `validate_rpc_contract(method: &str, raw_params: &Value) -> Result<(), String>` (validates both no-session and scope-aware `--session "test-session"` modes, plus `anki_skill::schema::validate_method_params`), and `assert_cli_args_conform(args: &[&str]) -> Vec<String>` (handling `RpcPlan::Local`, `Call`, `Paginate`, `WaitOperation`, and `Batch` in `Execute`/`Transport` modes). Added self-tests `schema_x_examples_pass_contract_validation` (16 examples) and `schema_x_invalid_examples_are_rejected` (17 invalid examples).
> - **1.2**: Added `core_domain_cli_cases()`, `escape_hatches_and_schema_local_contract`, and `core_domain_subcommands_conform_to_schema` in `tests/schema_contract_tests.rs`, covering all 53 core domain & escape-hatch RPC methods (`api.describe`, `api.execute`, `collection` [5], `operations` [2 + `wait`], `decks` [6], `deckPresets` [6], `notes` [12], `cards` [7], `tags` [5], `noteTypes` [8]) plus local `schema`, `schema notes.create`, and `schema --errors` assertions, YAML/JSON `--params`/`--fields`, `--name`/`--id` refs, and `--all` pagination.
> - **1.3**: Added `extended_domain_cli_cases()`, `extended_domain_subcommands_conform_to_schema`, and `all_96_schema_methods_covered_by_cli_subcommands` in `tests/schema_contract_tests.rs`, covering all 43 extended domain methods (`media` [5], `packages` [2], `scheduling` [9], `reviews` [2], `statistics` [4], `profiles` [3], `access` [1], `advanced` [2], `gui` [15]) and asserting 100% (96/96) `x-methods` coverage across all CLI subcommands. Also fixed a clap flag collision in `src/commands/media.rs` where `MediaCmd::Store::url` (`--url`) collided with global `Cli::url`, renaming the subcommand flag to `--source-url` (`id = "source_url"`).
> - **2.1**: Created `tests/mock_http_tests.rs` using `mockito::Server::new()` and `rpc_ok_mock` helper, with 3 integration tests exercising `AnkiClient` and `execute_plan`: `jsonrpc_envelope_and_bearer_auth_header` (verifying `Authorization: Bearer secret-token`, `Content-Type: application/json`, JSON-RPC 2.0 envelope via `Cli::try_parse_from` + `to_rpc_plan` + `execute_plan`), `scope_aware_session_injected_for_collection_methods` (`decks.list`, `notes.get` receive `"session": "s-1"`), and `scope_aware_session_omitted_for_application_methods` (`operations.get`, `api.describe`, `profiles.list`, `access.requestPermission` omit `"session"`).

> [!IMPORTANT]
> ### Autonomous Execution Protocol: `"Continue with next task @plans/04_testing_and_docs_plan.md"`
> When the user says **"Continue with next task @plans/04_testing_and_docs_plan.md"** (or similar):
> 1. **Read Context & Find Next Task**: Read **Implementation Status**, **Handoff Context**, and **Architectural Constraints & Verification**, then scan **Build Order & Tasks** for the **first unchecked task** (`- [ ] **<Task ID>**`).
> 2. **Execute ONLY That One Task**: Implement only the files and scope listed for that single task. Do NOT jump ahead to subsequent tasks.
> 3. **Verify**: Run the verification commands listed in the task / **Architectural Constraints & Verification** and confirm all checks pass.
> 4. **Update Plan & Handoff Context**:
>    - Check off `- [x] **<Task ID>**` in **Build Order & Tasks** (and check off the Phase in **Implementation Status** when all tasks in that phase are complete).
>    - Update **Handoff Context** with the new `Next task` (or `None` when complete) and a concise bullet summarizing what was implemented (exact file paths, symbols, signatures, keys, and caveats).
> 5. **Stop & Report**: Briefly summarize what was done, show verification results, explicitly announce the next pending task, and stop.

## Architectural Constraints & Verification
- **Constraints**:
  - Integration tests in `tests/` must import public library exports from `anki_skill` (`src/lib.rs`: `client::AnkiClient`, `commands::{Cli, Commands, RpcPlan, to_rpc_plan, execute_plan}`, `error::CliError`, `output::{OutputFormat, format_value, format_error}`, `schema::{method_scope, method_completion, validate_method_params, list_methods, describe_method, list_errors_and_limits}`).
  - Never connect to a live `127.0.0.1:8765` port in automated tests; use offline `to_rpc_plan` inspection or isolated `mockito::Server::new()` instances.
  - Operation polling and timeout tests in `tests/mock_http_tests.rs` and `tests/cli_e2e_tests.rs` must use millisecond polling intervals (e.g., `10ms`–`50ms`) so the entire test suite completes in under a few seconds.
  - Schema contract tests must validate every generated `(method, params)` pair both **without** a session and **with** `--session "test-session"` applied via scope-aware session injection (`scope == "collection"` receives `"session": "test-session"`, while `scope == "application"` omits `"session"`).
- **Verification**: `cargo test --all-targets` and `cargo clippy --all-targets -- -D warnings` (must pass after every task).

---

## Build Order & Tasks

### Phase 1 — Schema Contract Test Suite (`tests/schema_contract_tests.rs`)

- [x] **1.1 Create Draft 2020-12 Schema Validation Harness in `tests/schema_contract_tests.rs`**
  - **File(s)**: `tests/schema_contract_tests.rs`
  - **Scope**:
    - Load `schemas/api-v1.schema.json` once in the test module (`serde_json::from_str(include_str!("../schemas/api-v1.schema.json"))`).
    - Implement a schema contract validation harness (`validate_rpc_contract(method: &str, raw_params: &Value)`):
      - Use the `jsonschema` crate (Draft 2020-12 validator compiled against the root schema / `$defs` subschema referenced by `x-methods.<method>.params` and `x-methods.<method>.request`) or a deterministic recursive Draft 2020-12 `$defs` validator supporting `$ref`, `type`, `required`, `properties`, `additionalProperties`, `propertyNames`, `enum`, `const`, `oneOf`, `anyOf`, `allOf`, `not`, `if`/`then`, `minLength`, `pattern`, `minimum`, `maximum`, `minItems`, `maxItems`, `minProperties`, and `uniqueItems`.
      - Also invoke `anki_skill::schema::validate_method_params(method, params)` to verify that client-side pre-flight validation agrees with the full JSON Schema validator.
      - Verify both session modes for every tested call:
        1. **Without session**: Validate `{"jsonrpc": "2.0", "id": 1, "method": method, "params": raw_params}` against `x-methods.<method>.request` and `raw_params` against `x-methods.<method>.params`.
        2. **With `--session "test-session"`**: Apply scope-aware session injection (injecting `"session": "test-session"` if and only if `anki_skill::schema::method_scope(method) == Some("collection")`) and assert that the resulting `params` and request envelope still validate against `x-methods.<method>.params` and `x-methods.<method>.request` (confirming that none of the 9 `scope: "application"` methods with `additionalProperties: false` receive `"session"`).
    - Add a helper `assert_cli_args_conform(args: &[&str]) -> Vec<String>` that:
      - Parses `args` via `Cli::try_parse_from(args)` (or `Commands` parser),
      - Converts `cli.command` via `to_rpc_plan(&cli.command)` into an `RpcPlan`,
      - Extracts `(method, params)` from `RpcPlan::Call`, `RpcPlan::Paginate`, `RpcPlan::BatchExecute` (validating `api.execute` plus each inner command's `(method, params)`), or `RpcPlan::BatchTransport` (validating each request item),
      - Runs `validate_rpc_contract(method, &params)` and returns the list of exercised method names.
    - Add self-tests in `tests/schema_contract_tests.rs` verifying that every entry in `x-examples` passes `validate_rpc_contract` and every entry in `x-invalidExamples` in `schemas/api-v1.schema.json` is rejected.
  - **Verify**: `cargo test --test schema_contract_tests` and `cargo clippy --all-targets -- -D warnings`

- [x] **1.2 Add Core Domain Subcommand CLI-to-Schema Contract Tests**
  - **File(s)**: `tests/schema_contract_tests.rs`
  - **Scope**:
    - Add table-driven CLI-to-schema contract tests using `assert_cli_args_conform` for all core domain subcommands and escape hatches, covering both required and optional flag combinations (including `--name` vs `--id` `Reference` variants and YAML vs JSON `--fields` / `--params` inputs):
      - **Escape hatches & introspection (`2` RPC methods + local `schema`)**:
        - `describe` -> `api.describe`
        - `call decks.list`, `call notes.get --params "id: '1700000000001'"` (YAML input), `call notes.get --params '{"id":"1700000000001"}'` (JSON input)
        - `batch --mode execute` -> `api.execute` (with inner commands `notes.create` and `collection.sync`)
        - `batch --mode transport` -> `TransportBatch` array validation
        - `schema`, `schema notes.create`, `schema --errors` -> `RpcPlan::Local` output structure assertions
      - **`collection` (`5` methods)**:
        - `collection.get`, `collection.sync`, `collection.refresh`, `collection.checkIntegrity`, `collection.undo`
      - **`operations` (`2` methods + `wait`)**:
        - `operations.get`, `operations.cancel`, plus `operations wait op-1` -> `RpcPlan::WaitOperation`
      - **`decks` (`6` methods)**:
        - `decks.list`, `decks.get` (`--name` and `--id`), `decks.create`, `decks.update` (`--name` and `--id`), `decks.delete` (`--delete-cards`), `decks.getCounts` (`--include-children`)
      - **`deck-presets` (`6` methods)**:
        - `deckPresets.list`, `deckPresets.get`, `deckPresets.create`, `deckPresets.update`, `deckPresets.assign`, `deckPresets.delete`
      - **`notes` (`12` methods)**:
        - `notes.get`, `notes.getMany`, `notes.search` (single page and `--all` -> `RpcPlan::Paginate`), `notes.validate`, `notes.validateMany`, `notes.create` (with JSON and YAML `--fields`), `notes.createMany` (`--atomic`), `notes.update`, `notes.delete`, `notes.planConversion`, `notes.convert`, `notes.findEmpty`
      - **`cards` (`7` methods)**:
        - `cards.get`, `cards.getMany`, `cards.search` (single page and `--all`), `cards.render`, `cards.move`, `cards.setSuspended` (via `cards suspend` and `cards unsuspend`), `cards.setFlags`
      - **`tags` (`5` methods)**:
        - `tags.list`, `tags.add`, `tags.remove`, `tags.replace` (with `--note-ids` and `--all-notes`), `tags.deleteUnused`
      - **`note-types` (`8` methods)**:
        - `noteTypes.list`, `noteTypes.get`, `noteTypes.create`, `noteTypes.update`, `noteTypes.editStructure`, `noteTypes.getTemplateUsage`, `noteTypes.replaceText` (with `--note-types` and `--all-note-types`), `noteTypes.deleteUnused`
  - **Verify**: `cargo test --test schema_contract_tests` and `cargo clippy --all-targets -- -D warnings`

- [x] **1.3 Add Extended Domain Subcommand Contract Tests & 96-Method Exhaustive Coverage Assertion**
  - **File(s)**: `tests/schema_contract_tests.rs`
  - **Scope**:
    - Add table-driven CLI-to-schema contract tests using `assert_cli_args_conform` for all remaining domain subcommands:
      - **`media` (`5` methods)**:
        - `media.store` (`--data`, `--path`, `--url`, `--collision`), `media.get`, `media.list`, `media.delete`, `media.getDirectory`
      - **`packages` (`2` methods)**:
        - `packages.import`, `packages.export`
      - **`scheduling` (`9` methods)**:
        - `scheduling.get`, `scheduling.getQueue`, `scheduling.predict`, `scheduling.getDue`, `scheduling.answer`, `scheduling.answerMany`, `scheduling.repositionNewCards`, `scheduling.reset`, `scheduling.setDueDate`
      - **`reviews` (`2` methods)**:
        - `reviews.list`, `reviews.getLatestId`
      - **`statistics` (`4` methods)**:
        - `statistics.getToday`, `statistics.getReviewDays`, `statistics.getSummary`, `statistics.getHtml`
      - **`profiles` (`3` methods)**:
        - `profiles.list`, `profiles.getActive`, `profiles.switch`
      - **`access` (`1` method)**:
        - `access.requestPermission`
      - **`advanced` (`2` methods)**:
        - `advanced.cards.update`, `advanced.reviews.import`
      - **`gui` (`15` methods across `browser`, `editors`, `reviewer`, `application`)**:
        - `browser.open`, `browser.getSelection`, `browser.select` (`--card-ids` and `--note-ids`)
        - `editors.openNote`, `editors.openAdd`, `editors.updateDraft`
        - `reviewer.get`, `reviewer.startTimer`, `reviewer.showQuestion`, `reviewer.showAnswer`, `reviewer.answer`, `reviewer.playAudio`
        - `application.navigate` (`deckBrowser`, `deckOverview`, `review`), `application.importFile`, `application.exit`
    - Add an exhaustive coverage test `all_96_schema_methods_covered_by_cli_subcommands`:
      - Read all keys from `x-methods` in `schemas/api-v1.schema.json`,
      - Assert that `x_methods.len() == 96`,
      - Collect the set of method names produced by the core + extended subcommand contract test tables,
      - Assert that the difference between `x-methods` keys and tested CLI subcommand methods is empty (`missing.is_empty()`), proving 100% first-class subcommand and schema contract coverage across all 96 methods and 21 namespaces.
  - **Verify**: `cargo test --test schema_contract_tests` and `cargo clippy --all-targets -- -D warnings`

---

### Phase 2 — Mock HTTP Server Integration Tests (`tests/mock_http_tests.rs`)

- [x] **2.1 Create Mock HTTP Tests for Envelopes, Bearer Auth, and Scope-Aware Session Injection**
  - **File(s)**: `tests/mock_http_tests.rs`
  - **Scope**:
    - Create `tests/mock_http_tests.rs` using `mockito::Server::new()`.
    - Add tests exercising `AnkiClient` and `execute_plan`:
      - `jsonrpc_envelope_and_bearer_auth_header`:
        - Configure `AnkiClient` with `server.url()`, `Some("secret-token".into())`, and `None` session.
        - Mock `POST /` matching `Authorization: Bearer secret-token`, `Content-Type: application/json`, and valid JSON-RPC 2.0 body `{"jsonrpc": "2.0", "id": <u64>, "method": "decks.list", "params": {}}`.
        - Return `{"jsonrpc": "2.0", "id": <id>, "result": [{"id": "1", "name": "Default"}]}` and assert parsed result.
      - `scope_aware_session_injected_for_collection_methods`:
        - Configure `AnkiClient` with `session = Some("s-1".into())`.
        - Call `decks.list` (`scope: "collection"`) and assert request body contains `"params": {"session": "s-1"}`.
        - Call `notes.get` (`scope: "collection"`) with `{"id": "1700000000001"}` and assert request body contains `"params": {"id": "1700000000001", "session": "s-1"}`.
      - `scope_aware_session_omitted_for_application_methods`:
        - With `session = Some("s-1".into())` configured on `AnkiClient`, call `operations.get` (`{"operationId": "op-1"}`), `api.describe` (`{}`), `profiles.list` (`{}`), and `access.requestPermission` (`{}`).
        - Assert that none of the outgoing HTTP request bodies contain `"session"` in `"params"`.
  - **Verify**: `cargo test --test mock_http_tests` and `cargo clippy --all-targets -- -D warnings`

- [ ] **2.2 Add Mock HTTP Tests for Cursor Auto-Pagination & Async `OperationHandle` Lifecycle**
  - **File(s)**: `tests/mock_http_tests.rs`
  - **Scope**:
    - Add cursor auto-pagination tests:
      - `paginate_all_notes_and_cards_search_across_three_pages`:
        - Mock 3 sequential `notes.search` responses with `session = Some("s-1".into())`:
          - Page 1 (`cursor` absent): returns `{"ids": ["101", "102"], "nextCursor": "cur-2"}`
          - Page 2 (`cursor: "cur-2"`): returns `{"ids": ["103", "104"], "nextCursor": "cur-3"}`
          - Page 3 (`cursor: "cur-3"`): returns `{"ids": ["105"], "nextCursor": null}`
        - Execute `notes search "deck:Default" --limit 2 --all` via `to_rpc_plan` + `execute_plan` and assert the merged result equals `{"ids": ["101", "102", "103", "104", "105"], "pages": 3, "total": 5}`.
        - Repeat for `cards.search` (`cards search "is:due" --all`).
    - Add async `OperationHandle` lifecycle tests (configuring `AnkiClient` with fast millisecond poll intervals, e.g. `initial_poll_delay = 10ms`, `max_poll_delay = 25ms`, and `session = Some("s-1".into())`):
      - `async_operation_polls_pending_running_to_succeeded`:
        - Mock `collection.sync` returning `{"operationId": "op-sync-1", "method": "collection.sync", "session": "s-1"}`.
        - Mock 3 sequential `operations.get` calls (verifying `"session"` is **not** sent in `operations.get` params!) returning `state: "pending"`, `state: "running"`, and `state: "succeeded"` with `"result": {"collectionComplete": true, "mediaComplete": true}`.
        - Verify `call_and_maybe_wait("collection.sync", ..., true, 5)` returns `{"collectionComplete": true, "mediaComplete": true}`.
      - `async_operation_no_wait_returns_handle_immediately`:
        - Verify `wait = false` (`--no-wait`) returns the `OperationHandle` without calling `operations.get`.
      - `async_operation_failed_and_cancelled_return_exit_code_4`:
        - Mock `operations.get` returning `state: "failed"` (with `error.code = 1012`, `error.data.type = "ioError"`) and `state: "cancelled"` (with `error.code = 1007`, `error.data.type = "cancelled"`).
        - Assert `CliError::OperationFailed` is returned with `err.exit_code() == 4` and structured error details.
      - `async_operation_polling_timeout_returns_exit_code_5`:
        - Mock `operations.get` perpetually returning `state: "running"`.
        - Poll with a tight timeout and millisecond poll interval; assert `CliError::Timeout` is returned with `err.exit_code() == 5`.
  - **Verify**: `cargo test --test mock_http_tests` and `cargo clippy --all-targets -- -D warnings`

- [ ] **2.3 Add Mock HTTP Tests for Batch Execution & Deterministic Error Mapping**
  - **File(s)**: `tests/mock_http_tests.rs`
  - **Scope**:
    - Add batch execution tests:
      - `batch_execute_mode_calls_api_execute_and_polls_operation`:
        - Mock `api.execute` receiving `{"commands": [{"method": "decks.create", "params": {"name": "BatchDeck"}}, {"method": "collection.sync", "params": {}}]}` (without top-level `"session"` since `api.execute` has `scope: "application"`), returning an `OperationHandle` `op-batch-1`, followed by `operations.get` returning `state: "succeeded"` with per-command items.
      - `batch_transport_mode_sends_jsonrpc_array_batch`:
        - Mock `POST /` receiving a JSON-RPC 2.0 array (`TransportBatch`) of 2 request objects (verifying `scope: "collection"` item receives `"session": "s-1"` while `scope: "application"` item omits `"session"`), and returning a `TransportBatchResponse` array.
    - Add deterministic error mapping tests:
      - `http_401_and_403_map_to_exit_code_3`:
        - Mock HTTP `401 Unauthorized` and `403 Forbidden` responses; assert `CliError::HttpStatus` with `err.exit_code() == 3` and `err.to_json()["error"]["kind"] == "http"`.
      - `jsonrpc_error_1003_editor_conflict_maps_to_exit_code_4`:
        - Mock JSON-RPC error response `{"jsonrpc": "2.0", "id": 1, "error": {"code": 1003, "message": "Editor conflict.", "data": {"type": "editorConflict", "retryable": false}}}`; assert `CliError::Rpc` with `err.exit_code() == 4`, `code == 1003`, and `data.type == "editorConflict"`.
      - `preflight_schema_validation_rejects_invalid_params_with_exit_code_2_before_http`:
        - Configure a mock server with `.expect(0)` requests.
        - Attempt to execute an invalid request (e.g., `call notes.get --params '{"id": 123}'` or unknown method `call unknown.method`) with validation enabled; assert it fails immediately with `err.exit_code() == 2` and zero HTTP requests sent.
        - Verify that passing `--no-validate` bypasses client-side pre-flight validation and sends the request to the mock server.
  - **Verify**: `cargo test --test mock_http_tests` and `cargo clippy --all-targets -- -D warnings`

---

### Phase 3 — CLI Binary End-to-End Tests & Documentation

- [ ] **3.1 Create Compiled Binary End-to-End Tests in `tests/cli_e2e_tests.rs`**
  - **File(s)**: `tests/cli_e2e_tests.rs`
  - **Scope**:
    - Create `tests/cli_e2e_tests.rs` using `assert_cmd::Command::cargo_bin("anki-cli")`, `predicates`, and `mockito::Server`.
    - Test offline & live-mocked binary execution across output formats, input formats, and exit codes:
      - **Default YAML output on `stdout`**:
        - Run `anki-cli schema notes.create` and `anki-cli --url <mock> decks list`; assert exit code `0`, verify `stdout` parses via `serde_norway::from_str::<Value>`, and verify output is clean YAML (not compact JSON braces) containing expected keys (`method: notes.create`, `$defs`, etc.).
      - **`--format json` and `--format json-pretty` (plus `--json` and `--pretty` aliases)**:
        - Run `anki-cli --format json schema notes.create` and `anki-cli --format json-pretty --url <mock> decks list`; assert `stdout` is valid compact or pretty-printed JSON via `serde_json::from_str::<Value>`.
      - **YAML and JSON input on `--params`, `--fields`, and `--stdin`**:
        - Run `anki-cli --url <mock> notes create --deck Default --note-type Basic --fields "Front: 猫\nBack: cat"` (YAML `--fields`) and `--fields '{"Front":"猫","Back":"cat"}'` (JSON `--fields`).
        - Run `anki-cli --url <mock> call notes.get --stdin` with YAML (`id: "1700000000001"`) piped on `stdin`.
      - **Structured error envelopes on `stderr` and deterministic exit codes (`0`, `2`, `3`, `4`, `5`)**:
        - Exit code `0`: successful command execution.
        - Exit code `2`: invalid input / schema lookup failure / pre-flight validation error (`anki-cli schema nonexistent.method` or `anki-cli call notes.get --params "id: 123"`); assert `stderr` is valid YAML by default (and valid JSON when `--format json` is passed) with `error.kind: cli`.
        - Exit code `3`: HTTP 401/403 from mock server; assert `stderr` has `error.kind: http` and `error.status: 401`.
        - Exit code `4`: JSON-RPC error `1003` (`editorConflict`) or failed operation from mock server; assert `stderr` has `error.kind: rpc`, `error.code: 1003`, `error.data.type: editorConflict`.
        - Exit code `5`: operation polling timeout against mock server returning `state: "running"` with `--timeout 0` (or fast poll env/flag); assert `stderr` has `error.kind: timeout`.
  - **Verify**: `cargo test --test cli_e2e_tests` and `cargo clippy --all-targets -- -D warnings`

- [ ] **3.2 Update `SKILL.md` and `README.md` for YAML-First Output, Expanded Subcommands & Workflows**
  - **File(s)**: `SKILL.md`, `README.md`
  - **Scope**:
    - Update `SKILL.md`:
      - Document that `anki-cli` outputs **YAML by default** on `stdout` and `stderr` for token efficiency, and supports `--format <yaml|json|json-pretty>` (plus `--json` / `--pretty`).
      - Document that `--params`, `--fields`, and `--stdin` accept **both YAML and JSON** interchangeably.
      - Document scope-aware `--session` / `ANKI_CONNECT_SESSION` handling (automatically injected only into `scope: "collection"` methods and omitted on `scope: "application"` methods like `operations.get`).
      - Document client-side pre-flight schema validation against the embedded `api-v1.schema.json` (and `--no-validate` to bypass).
      - Expand the ergonomic subcommands reference to cover all domains: `collection`, `decks`, `deck-presets`, `notes`, `cards`, `note-types`, `tags`, `media`, `scheduling`, `reviews`, `statistics`, `profiles`, `packages`, `access`, `advanced`, `gui` (`browser`, `editors`, `reviewer`, `application`), `operations`, `batch` (`--mode execute|transport`), `call`, `describe`, and `schema`.
    - Update `README.md`:
      - Update the **Features** and **Configuration** sections to reflect YAML-by-default output (`--format <yaml|json|json-pretty>`), pre-flight validation (`--no-validate`), batch execution (`batch`), and complete subcommand coverage across all 96 API v1 methods / 21 namespaces.
      - Include practical CLI examples showing YAML output, YAML/JSON parameter inputs, `--all` auto-pagination, batch execution, and testing commands (`cargo test --all-targets`).
  - **Verify**: `cargo test --all-targets` and `cargo clippy --all-targets -- -D warnings`

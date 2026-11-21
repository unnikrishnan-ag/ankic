---
name: anki
description: Interact with a running Anki Desktop instance via ankic (Anki-Connect API v1). Use when querying or modifying decks, notes, cards, tags, scheduling, media, statistics, or GUI state in Anki.
---

# Anki (`ankic`)

Use `ankic` to interact with Anki Desktop over Anki-Connect API v1 (`http://127.0.0.1:8765/api/v1` by default; override with `--url` / `ANKI_CONNECT_URL` and `--api-key` / `ANKI_CONNECT_API_KEY`).

## Output, Input, Session & Validation Behavior

- **YAML-by-default output:** Both `stdout` (results) and `stderr` (structured errors) default to compact **YAML** for token efficiency. Override with `--format <yaml|json|json-pretty>` (or convenience aliases `--json` and `--pretty`).
- **YAML & JSON inputs:** `--params`, `--fields`, `--stdin`, and `batch` (`--commands`, `--file`, `--stdin`) accept **both YAML and JSON** interchangeably.
- **Scope-aware `--session` / `ANKI_CONNECT_SESSION`:** When a session ID is configured, `ankic` automatically injects `"session"` **only** into `scope: "collection"` methods and omits it on `scope: "application"` methods (such as `operations.get`, `api.describe`, `api.execute`, `profiles.*`, `access.requestPermission`, and `gui.application.*`).
- **Pre-flight schema validation:** All RPC calls are validated client-side against the embedded `api-v1.schema.json` before any HTTP request is sent (failing fast with exit code `2` on invalid parameters or unknown methods). Pass `--no-validate` to bypass.
- **Deterministic exit codes:** `0` = success, `2` = CLI / input / pre-flight validation error (`error.kind: cli`), `3` = transport / HTTP error (`error.kind: http`), `4` = JSON-RPC or failed/cancelled operation (`error.kind: rpc` or `operation`), `5` = operation polling timeout (`error.kind: timeout`).

## Core Workflow

1. **Verify connectivity & capture session:**
   ```bash
   ankic collection get
   ```
   Pass `--session <session>` (or set `ANKI_CONNECT_SESSION`) on subsequent collection calls to guard against profile switches.

2. **Discover any method's schema on demand (offline, no Anki required):**
   ```bash
   ankic schema --filter noteTypes
   ankic schema notes.create
   ankic schema --errors
   ```

3. **Use ergonomic subcommands across all 21 namespaces (96 API v1 methods):**
   - **`collection`**: `get`, `sync`, `refresh`, `check-integrity`, `undo`
   - **`decks`**: `list`, `get`, `create`, `update`, `delete`, `counts`
   - **`deck-presets`**: `list`, `get`, `create`, `update`, `assign`, `delete`
   - **`notes`**: `get`, `get-many`, `search` (`--all` auto-pagination), `validate`, `validate-many`, `create`, `create-many`, `update`, `delete`, `plan-conversion`, `convert`, `find-empty`
   - **`cards`**: `get`, `get-many`, `search` (`--all` auto-pagination), `render`, `move`, `suspend`, `unsuspend`, `flags`
   - **`note-types`**: `list`, `get`, `create`, `update`, `edit-structure`, `template-usage`, `replace-text`, `delete-unused`
   - **`tags`**: `list`, `add`, `remove`, `replace`, `delete-unused`
   - **`media`**: `list`, `get`, `store` (`--data` / `--path` / `--source-url`), `delete`, `directory`
   - **`scheduling`**: `get`, `queue`, `predict`, `due`, `answer`, `answer-many`, `reposition`, `reset`, `set-due-date`
   - **`reviews`**: `list`, `latest-id`
   - **`statistics`**: `today`, `review-days`, `summary`, `html`
   - **`profiles`**: `list`, `active`, `switch`
   - **`packages`**: `import`, `export`
   - **`access`**: `request-permission`
   - **`advanced`**: `cards-update`, `reviews-import`
   - **`gui`**:
     - `gui browser`: `open`, `selection`, `select`
     - `gui editors`: `open-note`, `open-add`, `update-draft`
     - `gui reviewer`: `get`, `start-timer`, `show-question`, `show-answer`, `answer`, `play-audio`
     - `gui application`: `navigate`, `import-file`, `exit`
   - **`operations`**: `get`, `wait`, `cancel` (async `OperationHandle` results are polled automatically unless `--no-wait` is passed)

4. **Batch & escape-hatch workflows:**
   - **Batch (`batch`):** Execute multiple `{method, params}` items sequentially via `api.execute` (`--mode execute`, default, polls `OperationHandle` unless `--no-wait`) or as a JSON-RPC array (`--mode transport`):
     ```bash
     ankic batch --mode execute --commands '[{"method":"decks.create","params":{"name":"Japanese"}},{"method":"collection.sync","params":{}}]'
     ankic batch --mode transport --file commands.yaml
     ```
   - **Direct RPC (`call`) & live capabilities (`describe`):**
     ```bash
     ankic describe
     ankic call scheduling.predict --params "cardId: '1700000000001'"
     ankic call notes.get --stdin < params.yaml
     ```

## Safety Rules

- **Validate before bulk note creation:** Use `notes validate` / `notes validate-many` before `notes create-many` when duplicate or empty-field eligibility matters.
- **Respect `editorConflict` (code `1003`):** Collection writes fail if an affected note editor is open in Anki. Ask the user to close the editor rather than retrying blindly.
- **Store media before referencing it:** Call `media store` first and insert the *returned* filename into note HTML or `[sound:...]` tags.
- **Do not auto-retry failed writes:** A request ID is not an idempotency key. Inspect the structured error on stderr (`error.data.type`) first.

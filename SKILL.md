---
name: anki
description: Interact with a running Anki Desktop instance via anki-cli (Anki-Connect API v1). Use when querying or modifying decks, notes, cards, tags, scheduling, media, statistics, or GUI state in Anki.
---

# Anki (`anki-cli`)

Use `anki-cli` to interact with Anki Desktop over Anki-Connect API v1 (`http://127.0.0.1:8765/api/v1` by default; override with `ANKI_CONNECT_URL` / `ANKI_CONNECT_API_KEY`).

## Core Workflow

1. **Verify connectivity & capture session:**
   ```bash
   anki-cli collection get
   ```
   Pass `--session <session>` (or set `ANKI_CONNECT_SESSION`) on subsequent collection calls to guard against profile switches.

2. **Discover any method's schema on demand (offline, no Anki required):**
   ```bash
   anki-cli schema --filter noteTypes
   anki-cli schema notes.create --pretty
   anki-cli schema --errors --pretty
   ```

3. **Use ergonomic subcommands for common tasks:**
   - **Decks:** `anki-cli decks list`, `anki-cli decks create "Japanese::Vocab"`, `anki-cli decks counts --name "Default" --include-children`
   - **Notes:** `anki-cli notes search "deck:Default" --all`, `anki-cli notes get <id>`, `anki-cli notes validate ...`, `anki-cli notes create ...`, `anki-cli notes update <id> --fields '{"Front":"..."}'`, `anki-cli notes delete <id>`
   - **Cards:** `anki-cli cards search "is:due"`, `anki-cli cards move --deck "Target" <id>...`, `anki-cli cards suspend <id>...`, `anki-cli cards flags --flag 1 <id>...`
   - **Collection:** `anki-cli collection sync`, `anki-cli collection undo`

4. **Use `call` for 100% V1 API coverage:**
   Any of the 96 V1 methods can be invoked directly:
   ```bash
   anki-cli call scheduling.predict --params '{"cardId":"1700000000001"}'
   anki-cli call api.execute --stdin < commands.json
   ```
   Async operations (`OperationHandle`) are polled automatically until completion unless `--no-wait` is passed.

## Safety Rules

- **Validate before bulk note creation:** Use `notes.validate` / `notes.validateMany` before `notes.createMany` when duplicate or empty-field eligibility matters.
- **Respect `editorConflict` (code `1003`):** Collection writes fail if an affected note editor is open in Anki. Ask the user to close the editor rather than retrying blindly.
- **Store media before referencing it:** Call `media.store` first and insert the *returned* filename into note HTML or `[sound:...]` tags.
- **Do not auto-retry failed writes:** A request ID is not an idempotency key. Inspect the structured JSON error on stderr (`error.data.type`) first.

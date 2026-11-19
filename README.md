# anki-skill (`anki-cli`)

A Rust CLI for [Anki-Connect API v1](../anki-connect/API_V1.md) (`POST /api/v1`), built for **100% API coverage** and **LLM agent skill** usage.

## Build

Requires Rust (1.75+ / 2021 edition):

```bash
cargo build --release
```

The binary is at `target/release/anki-cli`.

## Features

- **JSON-first output:** Compact JSON on stdout by default (use `--pretty` for human-readable output) and structured JSON errors on stderr with categorized exit codes (`2` = input/schema, `3` = transport/HTTP, `4` = RPC/operation failure, `5` = timeout).
- **Offline schema introspection (`schema`):** Embeds `schemas/api-v1.schema.json` so agents can inspect any of the 96 V1 methods and their parameter/result schemas without bloating prompts.
- **100% API coverage (`call`):** Raw JSON-RPC escape hatch for any method (`anki-cli call <method> --params '{...}'`), with automatic `OperationHandle` polling unless `--no-wait` is passed.
- **Ergonomic subcommands:** `collection`, `decks`, `notes`, `cards`, `operations`, and `describe`, including `--all` auto-pagination on search commands.

## Configuration

| Flag | Env Var | Default |
| --- | --- | --- |
| `--url` | `ANKI_CONNECT_URL` | `http://127.0.0.1:8765/api/v1` |
| `--api-key` | `ANKI_CONNECT_API_KEY` | unset |
| `--session` | `ANKI_CONNECT_SESSION` | unset |

## Examples

```bash
# List all available V1 methods offline
anki-cli schema

# Inspect exact parameter/result schema for a method
anki-cli schema notes.create --pretty

# Check active collection & session
anki-cli collection get

# Search notes (auto-paginating)
anki-cli notes search "deck:Default" --all

# Call any V1 method directly (waits for async operations by default)
anki-cli call statistics.getToday
anki-cli call packages.export --params '{"deck":{"name":"Default"},"path":"/tmp/out.apkg"}'
```

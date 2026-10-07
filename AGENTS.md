# AGENTS.md

## Project

`xbrowsersync-api-rs` — xBrowserSync REST API (wire contract v1.1.13) rewritten in Rust (axum) with a single SQLite file instead of MongoDB. Drop-in replacement for xBrowserSync-compatible browser extension clients. License: GPL-3.0-only.

## Commands

- `cargo build` / `cargo build --release` — build (binary: `target/release/xbrowsersync-api-rs`)
- `cargo test` — unit tests (config, uuid/timestamps) + integration (`tests/e2e.rs`, full HTTP flows). 15 tests total; all must pass.
- `cargo clippy --all-targets -- -D warnings` — must be clean, no suppressions.
- `cargo fmt` — applied before commits.

Rust toolchain on Windows: `$env:USERPROFILE\.cargo\bin\cargo.exe` (plain `cargo` is not on PATH).

## Structure

- `src/main.rs` — startup, tracing init, bind/shutdown (SIGINT + SIGTERM), purge task spawn.
- `src/lib.rs` — `build_router` (routes, CORS, TraceLayer, body limit) and `purge_loop`.
- `src/config.rs` — precedence: compiled defaults (`config/settings.default.json`, `include_str!`) -> `config/settings.json` -> `XBSAPI_*` env vars. Deep-merge of JSON values lives here.
- `src/db.rs` — one lock-guarded SQLite connection (`AppState::conn()`), migrations, PRAGMAs.
- `src/error.rs` — `ApiError` enum mapped to upstream `{code, message}` JSON envelopes; codes are wire contract literals.
- `src/services/` — bookmarks, info, new_sync_logs (IP throttle log), business logic; routes layer (`src/routes/`) is thin validation/parsing only.
- `src/uuid.rs` — sync-ID format (UUIDv4 as 32 hex, no dashes) and JS-`toISOString` parity timestamps (UTC, exactly 3ms digits).
- `docs.html` — docs page served at `GET /`; `include_str!`'d, templated with `{version}` / `{max_sync_size}`.

## Wire contract quirks (do not "fix")

- Unknown sync id -> 401, not 404.
- Unknown route -> 404 with `NotImplementedException`.
- `PUT` with stale `lastUpdated` -> 409 `SyncConflictException`.
- Body over limit -> bare 413 (no JSON envelope); `RequestBodyLimitLayer` handles it.
- v1 clients POST `{bookmarks}` only; v1.1.3+ send `{version}`.

## Conventions / constraints

- Single-connection SQLite behind a mutex is deliberate (see `ponytail:` note in `src/db.rs`); don't reintroduce a pooling crate without measurements.
- Config fields must exist in `config/settings.default.json`; env vars are `XBSAPI_<PLAIN_CAPS>` mapping to dot-paths in `config.rs`.
- Container layout: everything under one `/data` volume (db `/data/db/`, logs `/data/logs/`, optional config `/data/config/settings.json`); no bundled runtime config/docs (compiled in).
- Timestamps must stay JS-compatible: always UTC with exactly 3 subsecond digits.

## Deployment

- Docker (`Dockerfile`, distroless) + `compose.yaml` — one `/data` volume; graceful stop via SIGTERM handling.

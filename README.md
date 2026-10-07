# xbrowsersync-api-rs

xBrowserSync API in Rust + SQLite.

Rust rewrite of the xBrowserSync REST API (`xbrowsersync/api` v1.1.13 wire contract), backed by a single SQLite file. Drop-in replacement for the MarkSync / xBrowserSync browser extension clients.

Status: functional core (all 7 legacy-e2e-derived integration tests + unit tests green; MarkSync contract suite is the final acceptance gate - see "Contract testing").

## Why

- Original: Node/Express/Mongoose/MongoDB, heavy dependency tree.
- This: Rust + axum + plain SQLite, single static binary, one data volume.
- FerretDB was evaluated and skipped: 2.x is Postgres-only (DocumentDB); 1.x's SQLite backend is frozen. Payloads are opaque client-encrypted blobs, so no document store is needed.

## Run

```
cargo run --release
```

Server listens on `0.0.0.0:8080`, SQLite at `data/xbrowsersync.sqlite3` (WAL).

Your clients (MarkSync extension) point at the service URL, e.g. `http://127.0.0.1:8080/`.

## Configuration

Precedence: compiled defaults (`config/settings.default.json`) -> `config/settings.json` (optional) -> `XBSAPI_*` env vars.

| Env var | Default | Meaning |
|---|---|---|
| `XBSAPI_DB_PATH` | `data/xbrowsersync.sqlite3` | SQLite file path |
| `XBSAPI_MAXSYNCSIZE` | `512000` | max request body bytes (413 above) |
| `XBSAPI_MAXSYNCS` | `0` (off) | global cap on stored syncs |
| `XBSAPI_DAILYNEWSYNCSLIMIT` | `0` (off) | new-syncs-per-IP-per-day limit |
| `XBSAPI_LOCATION` | *(empty)* | ISO 3166-1 alpha-2 shown in `/info` |
| `XBSAPI_STATUS_ONLINE` | `true` | false = offline (503 on writes) |
| `XBSAPI_STATUS_ALLOWNEWSYNCS` | `true` | false = existing syncs keep working |
| `XBSAPI_STATUS_MESSAGE` | *(empty)* | status panel message (script tags stripped) |
| `XBSAPI_SERVER_HOST` / `XBSAPI_SERVER_PORT` | `0.0.0.0` / `8080` | listen address |
| `XBSAPI_SERVER_RELATIVEPATH` | `/` | path prefix |
| `XBSAPI_SERVER_BEHINDPROXY` | `false` | trust X-Forwarded-For for client IP |
| `XBSAPI_ALLOWEDORIGINS` | *(all)* | comma-separated CORS origins |
| `XBSAPI_PURGEINTERVALSECONDS` | `60` | background purge cadence |
| `XBSAPI_PURGESTALESYNCDAYS` | `21` | purge syncs untouched for N days (0=off) |
| `XBSAPI_LOG_FILE_PATH` | *(off)* | enable rolling file log |
| `XBSAPI_LOG_STDOUT_LEVEL` | `info` | log filter level (see Logging below) |

## Logging

All output (stdout, and the file log when enabled) is filtered by a single `tracing` env filter: `RUST_LOG` if set, otherwise `XBSAPI_LOG_STDOUT_LEVEL` (default `info`). To increase verbosity:

```
XBSAPI_LOG_STDOUT_LEVEL=debug cargo run --release
```

Uses standard `RUST_LOG` filter syntax, so per-crate tuning works too: `XBSAPI_LOG_STDOUT_LEVEL=info,axum=trace` shows every HTTP request. `trace` is the most verbose level. File logging is turned on by config (`"log": {"file": {"enabled": true, "path": "logs/api.log"}}` in `config/settings.json` or the `XBSAPI_LOG_FILE_PATH` env var) and shares the same filter.

## API

Full wire parity with upstream 1.1.x, incl. quirks (unknown sync id = 401, catch-all 404 = `NotImplementedException`, `{code,message}` error envelope, UUIDv4 hex-32 sync IDs, ISO-8601-with-ms timestamps). `GET /` serves the docs page.

| Method | Path | Quirk / notes |
|---|---|---|
| GET | `/info` | `{location, maxSyncSize, message, status, version}`; status 0/1/2 |
| POST | `/bookmarks` | `{bookmarks}` (v1) or `{version}` (>=1.1.3) -> `{id, lastUpdated[, version]}` |
| GET | `/bookmarks/:id` | -> `{bookmarks, version, lastUpdated}`; touches lastAccessed |
| PUT | `/bookmarks/:id` | `{bookmarks[, lastUpdated, version]}`; stale `lastUpdated` -> 409 |
| GET | `/bookmarks/:id/lastUpdated` | -> `{lastUpdated}` |
| GET | `/bookmarks/:id/version` | -> `{version}` |

## Tests

```
cargo test
```

- unit: config precedence/merge, UUID v4 hex-32, JS-`toISOString` timestamp parity, conflict compare
- integration (`tests/e2e.rs`): full flows ported from upstream `test/e2e` (create/get/update/conflict 409/401/400/404/503/stale purge)

## Contract testing (acceptance gate)

Point MarkSync's contract suite at this server:

```
# server side
XBSAPI_SERVER_PORT=8080 cargo run --release
# app-next side (needs a reference-compatible backend env; extension e2e)
XBS_CONTRACT_URL=http://localhost:8080 pnpm test:e2e        # extension in browser
```

## Deployment

- `cargo build --release` -> `target/release/xbrowsersync-api-rs(.exe)`; run it, systemd unit optional (`Restart=on-failure`, `WorkingDirectory=` where data/ lives).
- Docker: see `Dockerfile` + `compose.yaml`. Everything lives in a single mounted volume at `/data`: DB at `/data/db/`, file logs at `/data/logs/`, and optional config overrides at `/data/config/settings.json` (defaults are compiled into the binary, so no bundled config file is needed). One volume = data + config together; to bring host config in, add `- ./config:/data/config:ro`.

## License

GPL-3.0-only, lineage: `xbrowsersync/api` -> `swarnat/xbrowsersync-api` -> `rwjack/xbrowsersync-api` -> this clean rewrite.

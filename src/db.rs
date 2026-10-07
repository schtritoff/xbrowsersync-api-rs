use std::sync::{Arc, Mutex};

use rusqlite_migration::{Migrations, M};

pub type Db = Arc<Mutex<rusqlite::Connection>>;
pub type SharedState = Arc<AppState>;

pub struct AppState {
    pub db: Db,
    pub config: crate::config::Config,
}

impl AppState {
    /// Single lock-guarded sqlite connection.
    ///
    /// ponytail: one shared connection instead of r2d2+r2d2_sqlite; the mutex
    /// only ever contends per-op (fast), swap a pool back in if measurements
    /// ever disagree.
    pub fn conn(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        self.db.lock().expect("sqlite connection lock poisoned")
    }
}

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            r#"
            CREATE TABLE bookmarks (
                id            TEXT PRIMARY KEY,
                bookmarks     TEXT,
                version       TEXT,
                last_accessed TEXT NOT NULL,
                last_updated  TEXT NOT NULL
            );
            CREATE INDEX idx_bookmarks_last_accessed ON bookmarks (last_accessed);
            "#,
        ),
        M::up(
            r#"
            CREATE TABLE newsynclogs (
                id           TEXT PRIMARY KEY,
                ip_address   TEXT NOT NULL,
                expires_at   TEXT NOT NULL,
                sync_created TEXT NOT NULL
            );
            CREATE INDEX idx_newsynclogs_ip ON newsynclogs (ip_address);
            CREATE INDEX idx_newsynclogs_expires ON newsynclogs (expires_at);
            "#,
        ),
    ])
}

pub fn init_db(cfg: &crate::config::Config) -> Result<Db, Box<dyn std::error::Error>> {
    let path = std::path::Path::new(&cfg.db_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = rusqlite::Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;",
    )?;
    migrations().to_latest(&mut conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

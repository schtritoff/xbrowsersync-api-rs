use std::net::SocketAddr;
use std::sync::Arc;

use xbrowsersync_api_rs::{build_router, config, db, purge_loop};

fn init_tracing(cfg: &config::Config) {
    use tracing_subscriber::EnvFilter;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cfg.log.stdout.level));
    if cfg.log.file.enabled && !cfg.log.file.path.is_empty() {
        let path = std::path::Path::new(&cfg.log.file.path);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let file_name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "api.log".to_string());
        let appender = tracing_appender::rolling::daily(path.parent().unwrap(), file_name);
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::sync::Mutex::new(appender))
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

#[tokio::main]
async fn main() {
    let cfg = match config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {e}");
            std::process::exit(1);
        }
    };
    init_tracing(&cfg);
    tracing::info!(version = %config::API_VERSION, "xBrowserSync API starting");

    let db = match db::init_db(&cfg) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to initialize database: {e}");
            std::process::exit(1);
        }
    };
    let state: db::SharedState = Arc::new(db::AppState { db, config: cfg });

    tokio::spawn(purge_loop(state.clone()));
    let router = build_router(state.clone());

    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, addr = %addr, "failed to bind");
            std::process::exit(1);
        }
    };
    tracing::info!(addr = %addr, "Service started");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server error");
    tracing::info!("Service shutting down");
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler failed");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await.expect("SIGINT handler failed");
}

pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod services;
pub mod uuid;

use std::time::Duration;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

/// Build the full application router (docs page, info, bookmarks routes,
/// CORS, tracing, body-size limit, fallback 404).
///
/// ponytail: body size limit currently returns a bare 413 with no JSON
/// envelope, and rusqlite calls below run inline in async handlers (fast
/// per-op, multi-thread runtime absorbs it). Both get a from_fn wrapper /
/// spawn_blocking if measurements ever disagree.
pub fn build_router(state: db::SharedState) -> Router {
    let prefix = {
        let rel = state.config.relative_path();
        if rel == "/" {
            String::new()
        } else {
            rel.trim_end_matches('/').to_string()
        }
    };

    let allowed = if state.config.allowed_origins.is_empty() {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(
            state
                .config
                .allowed_origins
                .iter()
                .filter_map(|s| axum::http::HeaderValue::from_str(s).ok())
                .collect::<Vec<_>>(),
        )
    };
    let cors = CorsLayer::new()
        .allow_origin(allowed)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(vec![
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ]);

    Router::new()
        .route("/", get(routes::info::docs_page))
        .route("/info", get(routes::info::get_info_route))
        .route(
            &format!("{prefix}/bookmarks", prefix = prefix),
            post(routes::bookmarks::create_bookmarks),
        )
        .route(
            &format!("{prefix}/bookmarks/{{id}}", prefix = prefix),
            get(routes::bookmarks::get_bookmarks).put(routes::bookmarks::update_bookmarks),
        )
        .route(
            &format!("{prefix}/bookmarks/{{id}}/lastUpdated", prefix = prefix),
            get(routes::bookmarks::get_last_updated),
        )
        .route(
            &format!("{prefix}/bookmarks/{{id}}/version", prefix = prefix),
            get(routes::bookmarks::get_version),
        )
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(state.config.max_sync_size))
        .fallback(routes::info::not_found)
        .with_state(state)
}

/// Start the purge background task (expired new-sync logs, stale syncs).
pub async fn purge_loop(state: db::SharedState) {
    let interval = Duration::from_secs(state.config.purge_interval_seconds.max(1));
    loop {
        tokio::time::sleep(interval).await;
        if let Err(e) = services::new_sync_logs::purge_expired(&state).await {
            tracing::warn!(error = %e, "newsynclogs purge failed");
        }
        if state.config.purge_stale_syncs_days > 0 {
            match services::bookmarks::purge_stale(&state, state.config.purge_stale_syncs_days)
                .await
            {
                Ok(n) if n > 0 => tracing::info!(purged = n, "stale syncs purged"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "stale syncs purge failed"),
            }
        }
    }
}

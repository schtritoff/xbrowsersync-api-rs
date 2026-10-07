use axum::extract::State;
use axum::response::Html;
use axum::Json;

use crate::db::SharedState;
use crate::error::ApiError;
use crate::services::info::{self, ServiceInfo};

/// GET /info
pub async fn get_info_route(
    State(state): State<SharedState>,
) -> Result<Json<ServiceInfo>, ApiError> {
    info::get_info(state).await.map(Json)
}

/// GET / — docs homepage (static HTML; version/maxSyncSize templated live).
pub async fn docs_page(State(state): State<SharedState>) -> Html<String> {
    let cfg = &state.config;
    let html = include_str!("../../docs.html")
        .replacen("{version}", env!("CARGO_PKG_VERSION"), 1)
        .replacen("{max_sync_size}", &cfg.max_sync_size.to_string(), 1);
    Html(html)
}

/// Catch-all 404 → NotImplementedException (upstream quirk).
pub async fn not_found() -> ApiError {
    ApiError::NotImplemented
}

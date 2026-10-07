use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use std::net::SocketAddr;

use crate::db::SharedState;
use crate::error::ApiError;
use crate::services::{bookmarks as svc, new_sync_logs as logs};
use crate::uuid;

fn parse_id(raw: &str) -> Result<String, ApiError> {
    let id = raw.trim().to_lowercase();
    if !uuid::valid_sync_id(&id) {
        return Err(ApiError::InvalidSyncId);
    }
    Ok(id)
}

fn client_ip(
    state: &SharedState,
    connect_info: ConnectInfo<SocketAddr>,
    headers: &HeaderMap,
) -> Option<String> {
    let fwd = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    logs::client_ip(state.config.server.behind_proxy, Some(connect_info.0), fwd)
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateBody {
    pub bookmarks: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateBody {
    pub bookmarks: Option<String>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: Option<String>,
    pub version: Option<String>,
}

/// POST /bookmarks — v1 sends {bookmarks}, v1.1.3+ sends {version}.
pub async fn create_bookmarks(
    State(state): State<SharedState>,
    connect_info: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<svc::CreateBookmarksResponse>, ApiError> {
    let parsed: CreateBody = if body.is_empty() {
        CreateBody::default()
    } else {
        serde_json::from_slice(&body).map_err(|_| ApiError::RequiredDataNotFound)?
    };
    if parsed.bookmarks.is_none() && parsed.version.is_none() {
        return Err(ApiError::RequiredDataNotFound);
    }
    let ip = client_ip(&state, connect_info, &headers);
    let resp = svc::create_bookmarks(&state, parsed.bookmarks, parsed.version, ip).await?;
    Ok(Json(resp))
}

/// GET /bookmarks/:id
pub async fn get_bookmarks(
    State(state): State<SharedState>,
    Path(raw): Path<String>,
) -> Result<Json<svc::GetBookmarksResponse>, ApiError> {
    let id = parse_id(&raw)?;
    Ok(Json(svc::get_bookmarks(&state, &id).await?))
}

/// GET /bookmarks/:id/lastUpdated
pub async fn get_last_updated(
    State(state): State<SharedState>,
    Path(raw): Path<String>,
) -> Result<Json<svc::LastUpdatedResponse>, ApiError> {
    let id = parse_id(&raw)?;
    Ok(Json(svc::get_last_updated(&state, &id).await?))
}

/// GET /bookmarks/:id/version
pub async fn get_version(
    State(state): State<SharedState>,
    Path(raw): Path<String>,
) -> Result<Json<svc::VersionResponse>, ApiError> {
    let id = parse_id(&raw)?;
    Ok(Json(svc::get_version(&state, &id).await?))
}

/// PUT /bookmarks/:id — v1 {bookmarks}; v1.1.3+ {bookmarks, lastUpdated, version}.
pub async fn update_bookmarks(
    State(state): State<SharedState>,
    Path(raw): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<svc::LastUpdatedResponse>, ApiError> {
    let id = parse_id(&raw)?;
    let parsed: UpdateBody = if body.is_empty() {
        UpdateBody::default()
    } else {
        serde_json::from_slice(&body).map_err(|_| ApiError::RequiredDataNotFound)?
    };
    if parsed.bookmarks.is_none() {
        return Err(ApiError::RequiredDataNotFound);
    }
    Ok(Json(
        svc::update_bookmarks(
            &state,
            &id,
            parsed.bookmarks,
            parsed.last_updated,
            parsed.version,
        )
        .await?,
    ))
}

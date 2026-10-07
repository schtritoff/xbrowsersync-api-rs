use std::sync::Arc;

use axum::http::StatusCode;
use tower::ServiceExt;
use uuid::Uuid;

use xbrowsersync_api_rs::build_router;
use xbrowsersync_api_rs::config::Config;
use xbrowsersync_api_rs::db::{self, AppState, SharedState as State};

fn test_state(db_path: &str) -> State {
    let mut cfg = default_cfg();
    cfg.db_path = format!("{db_path}/db.sqlite3");
    let db = db::init_db(&cfg).unwrap();
    Arc::new(AppState { db, config: cfg })
}

fn default_cfg() -> Config {
    serde_json::from_value::<Config>(serde_json::json!({})).unwrap()
}

// raw HTTP helper (RequestBodyLimit + ConnectInfo are part of the stack in main)
async fn call(
    router: axum::Router,
    method: &str,
    uri: &str,
    body: Option<String>,
) -> (StatusCode, serde_json::Value) {
    let req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    let req = match body {
        Some(b) => req.body(axum::body::Body::from(b)).unwrap(),
        None => req.body(axum::body::Body::empty()).unwrap(),
    };
    // ConnectInfo extractor needs the extension (injected by
    // into_make_service_with_connect_info in production)
    let mut req = req;
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:9999".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

#[tokio::test]
async fn info_reports_service_status() {
    let dir = tempdir();
    let state = test_state(&dir);
    let app = build_router(state);
    let (status, body) = call(app, "GET", "/info", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], 0);
    assert!(body["version"].is_string());
    assert_eq!(body["maxSyncSize"], 512000);
}

#[tokio::test]
async fn create_get_update_bookmarks_flow() {
    let dir = tempdir();
    let state = test_state(&dir);
    let app = build_router(state);

    // create (v1 shape)
    let (status, body) = call(
        app.clone(),
        "POST",
        "/bookmarks",
        Some(r#"{"bookmarks":"<encrypted payload>"}"#.into()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create failed: {body}");
    let id = body["id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(body["lastUpdated"].is_string());
    assert!(body["version"].is_null());

    // get
    let (status, body) = call(app.clone(), "GET", &format!("/bookmarks/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bookmarks"], "<encrypted payload>");
    assert!(body["lastUpdated"].is_string());

    // update (v2 shape with lastUpdated for conflict check)
    let last_updated = body["lastUpdated"].as_str().unwrap().to_string();
    let (status, body) = call(
        app.clone(),
        "PUT",
        &format!("/bookmarks/{id}"),
        Some(
            serde_json::json!({"bookmarks":"<new payload>", "lastUpdated": last_updated})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update failed: {body}");
    assert!(body["lastUpdated"].is_string());

    // conflict: stale lastUpdated must 409
    let (status, body) = call(
        app.clone(),
        "PUT",
        &format!("/bookmarks/{id}"),
        Some(
            serde_json::json!({"bookmarks":"<newer payload>", "lastUpdated": last_updated})
                .to_string(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "expected conflict: {body}");
    assert_eq!(body["code"], "SyncConflictException");

    // lastUpdated endpoint reflects latest
    let (status, body) = call(
        app.clone(),
        "GET",
        &format!("/bookmarks/{id}/lastUpdated"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["lastUpdated"].is_string());

    // version endpoint
    let (status, body) = call(
        app.clone(),
        "POST",
        "/bookmarks",
        Some(r#"{"version":"1.1.3"}"#.into()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], "1.1.3");
    let vid = body["id"].as_str().unwrap().to_string();
    let (status, body) = call(
        app.clone(),
        "GET",
        &format!("/bookmarks/{vid}/version"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], "1.1.3");

    drop(app);
    drop(dir);
}

#[tokio::test]
async fn unknown_sync_id_is_401() {
    let dir = tempdir();
    let state = test_state(&dir);
    let app = build_router(state);

    let id = uuid_like_hex32();
    let (status, body) = call(app.clone(), "GET", &format!("/bookmarks/{id}"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "SyncNotFoundException");

    // malformed id
    let (status, body) = call(app, "GET", "/bookmarks/not-a-uuid", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "InvalidSyncIdException");

    drop(dir);
}

#[tokio::test]
async fn missing_data_is_400() {
    let dir = tempdir();
    let state = test_state(&dir);
    let app = build_router(state);

    let (status, body) = call(app.clone(), "POST", "/bookmarks", Some("{}".into())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "RequiredDataNotFoundException");

    let id = uuid_like_hex32();
    let (status, body) = call(app, "PUT", &format!("/bookmarks/{id}"), Some("{}".into())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "RequiredDataNotFoundException");

    drop(dir);
}

#[tokio::test]
async fn unknown_route_is_not_implemented_404() {
    let dir = tempdir();
    let state = test_state(&dir);
    let app = build_router(state);
    let (status, body) = call(app, "GET", "/definitely/missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NotImplementedException");
    drop(dir);
}

#[tokio::test]
async fn offline_service_returns_503_on_create() {
    let dir = tempdir();
    let mut cfg = default_cfg();
    cfg.db_path = format!("{dir}/db.sqlite3");
    cfg.status.online = false;
    let db = db::init_db(&cfg).unwrap();
    let app = build_router(Arc::new(AppState { db, config: cfg }));
    let (status, body) = call(
        app,
        "POST",
        "/bookmarks",
        Some(r#"{"bookmarks":"x"}"#.into()),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body: {body}");
    assert_eq!(body["code"], "ServiceNotAvailableException", "body: {body}");
    drop(dir);
}

#[tokio::test]
async fn stale_last_accessed_syncs_are_purged() {
    let dir = tempdir();
    let state = test_state(&dir);
    let app = build_router(state.clone());

    let (_, body) = call(
        app,
        "POST",
        "/bookmarks",
        Some(r#"{"bookmarks":"b"}"#.into()),
    )
    .await;
    let id = body["id"].as_str().unwrap().to_string();

    // manually backdate last_accessed beyond purge threshold (21 days)
    state
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE bookmarks SET last_accessed = '1999-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    let purged = xbrowsersync_api_rs::services::bookmarks::purge_stale(&state, 21)
        .await
        .unwrap();
    assert_eq!(purged, 1);
    let (status, _) = call(
        build_router(state),
        "GET",
        &format!("/bookmarks/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    drop(dir);
}

fn uuid_like_hex32() -> String {
    // valid v4 uuid hex (no dashes), not inserted in db
    Uuid::new_v4().simple().to_string()
}

fn tempdir() -> String {
    let dir = std::env::temp_dir().join(format!(
        "xbs-test-{}-{}",
        std::process::id(),
        Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().to_string()
}

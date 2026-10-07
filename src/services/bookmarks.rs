use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::db::SharedState;
use crate::error::ApiError;
use crate::services::new_sync_logs as logs;
use crate::uuid;

#[derive(Debug, Serialize)]
pub struct CreateBookmarksResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GetBookmarksResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bookmarks: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
}

#[derive(Debug, Serialize)]
pub struct LastUpdatedResponse {
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

pub async fn create_bookmarks(
    state: &SharedState,
    bookmarks: Option<String>,
    version: Option<String>,
    ip: Option<String>,
) -> Result<CreateBookmarksResponse, ApiError> {
    // Service availability + new-sync policy
    if !state.config.status.online {
        return Err(ApiError::ServiceNotAvailable);
    }
    if !state.config.status.allow_new_syncs {
        return Err(ApiError::NewSyncsForbidden);
    }
    if state.config.max_syncs > 0 {
        let count: i64 = state
            .conn()
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0))?;
        if count >= state.config.max_syncs as i64 {
            return Err(ApiError::NewSyncsForbidden);
        }
    }
    if state.config.daily_new_syncs_limit > 0
        && logs::limit_hit(state, ip.as_deref(), state.config.daily_new_syncs_limit).await?
    {
        return Err(ApiError::NewSyncsLimitExceeded);
    }

    let id = uuid::new_sync_id();
    let now = uuid::now_iso();
    // must drop the lock before create_log below re-acquires it
    state.conn().execute(
        "INSERT INTO bookmarks (id, bookmarks, version, last_accessed, last_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, bookmarks, version, now, now],
    )?;

    if state.config.daily_new_syncs_limit > 0 {
        logs::create_log(state, ip).await?;
    }
    tracing::info!("New bookmarks sync created");

    Ok(CreateBookmarksResponse {
        id: Some(id),
        last_updated: Some(now),
        version,
    })
}

async fn touch_and_fetch(
    state: &SharedState,
    id: &str,
) -> Result<(Option<String>, Option<String>, String), ApiError> {
    let now = uuid::now_iso();
    let row = state
        .conn()
        .query_row(
            "UPDATE bookmarks SET last_accessed = ?2 WHERE id = ?1
             RETURNING bookmarks, version, last_updated",
            rusqlite::params![id, now],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ApiError::SyncNotFound,
            other => other.into(),
        })?;
    Ok(row)
}

pub async fn get_bookmarks(
    state: &SharedState,
    id: &str,
) -> Result<GetBookmarksResponse, ApiError> {
    let (bookmarks, version, last_updated) = touch_and_fetch(state, id).await?;
    Ok(GetBookmarksResponse {
        bookmarks,
        version,
        last_updated,
    })
}

pub async fn get_last_updated(
    state: &SharedState,
    id: &str,
) -> Result<LastUpdatedResponse, ApiError> {
    let (_, _, last_updated) = touch_and_fetch(state, id).await?;
    Ok(LastUpdatedResponse {
        last_updated: Some(last_updated),
    })
}

pub async fn get_version(state: &SharedState, id: &str) -> Result<VersionResponse, ApiError> {
    let (_, version, _) = touch_and_fetch(state, id).await?;
    Ok(VersionResponse { version })
}

pub async fn update_bookmarks(
    state: &SharedState,
    id: &str,
    bookmarks: Option<String>,
    client_last_updated: Option<String>,
    new_version: Option<String>,
) -> Result<LastUpdatedResponse, ApiError> {
    let now_dt = OffsetDateTime::now_utc();
    let now = uuid::format_rfc3339_millis(now_dt);

    // Conflict check + row existence (only when client sends lastUpdated, i.e. v2)
    let conn = state.conn();
    let existing: String = conn
        .query_row(
            "SELECT last_updated FROM bookmarks WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ApiError::SyncNotFound,
            other => other.into(),
        })?;

    if let Some(client_ts) = client_last_updated {
        if !uuid::timestamps_equal(&client_ts, &existing) {
            return Err(ApiError::SyncConflict);
        }
    }

    conn.execute(
        "UPDATE bookmarks SET bookmarks = COALESCE(?2, bookmarks),
                version = COALESCE(?3, version),
                last_accessed = ?4,
                last_updated = ?5
         WHERE id = ?1",
        rusqlite::params![id, bookmarks, new_version, now, now],
    )?;

    Ok(LastUpdatedResponse {
        last_updated: Some(now),
    })
}

pub async fn purge_stale(state: &SharedState, max_age_days: u32) -> Result<usize, ApiError> {
    if max_age_days == 0 {
        return Ok(0);
    }
    let cutoff = (OffsetDateTime::now_utc() - time::Duration::days(max_age_days as i64))
        .format(&Rfc3339)
        .map_err(crate::error::internal)?;
    let n = state.conn().execute(
        "DELETE FROM bookmarks WHERE last_accessed < ?1",
        rusqlite::params![cutoff],
    )?;
    Ok(n)
}

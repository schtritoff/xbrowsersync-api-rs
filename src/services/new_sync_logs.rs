use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::db::SharedState;
use crate::error::ApiError;
use crate::uuid;

/// Log one new-sync creation for IP-based throttling.
pub async fn create_log(state: &SharedState, ip: Option<String>) -> Result<(), ApiError> {
    let Some(ip) = ip else {
        tracing::info!("Unable to determine client IP address");
        return Ok(());
    };
    // Expire at next local-midnight UTC boundary, same spirit as upstream.
    let now = OffsetDateTime::now_utc();
    let tomorrow = (now + time::Duration::days(1)).replace_time(time::Time::MIDNIGHT);
    state.conn().execute(
        "INSERT INTO newsynclogs (id, ip_address, expires_at, sync_created) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            uuid::new_sync_id(),
            ip,
            tomorrow.format(&Rfc3339).map_err(crate::error::internal)?,
            uuid::now_iso(),
        ],
    )?;
    Ok(())
}

/// True when this IP has already created >= limit new syncs today.
pub async fn limit_hit(
    state: &SharedState,
    ip: Option<&str>,
    limit: u32,
) -> Result<bool, ApiError> {
    let Some(ip) = ip else {
        return Ok(false);
    };
    if limit == 0 {
        return Ok(false);
    }
    let midnight = OffsetDateTime::now_utc().replace_time(time::Time::MIDNIGHT);
    let since = midnight.format(&Rfc3339).map_err(crate::error::internal)?;
    let n: i64 = state.conn().query_row(
        "SELECT COUNT(*) FROM newsynclogs WHERE ip_address = ?1 AND sync_created >= ?2",
        rusqlite::params![ip, since],
        |r| r.get(0),
    )?;
    Ok(n >= limit as i64)
}

/// Remove expired newsynclogs rows; return number deleted.
pub async fn purge_expired(state: &SharedState) -> Result<usize, ApiError> {
    let now = uuid::now_iso();
    let n = state.conn().execute(
        "DELETE FROM newsynclogs WHERE expires_at < ?1",
        rusqlite::params![now],
    )?;
    Ok(n)
}

pub fn client_ip(
    behind_proxy: bool,
    connect_info: Option<std::net::SocketAddr>,
    forwarded_for: Option<&str>,
) -> Option<String> {
    if behind_proxy {
        if let Some(fwd) = forwarded_for {
            if let Some(first) = fwd.split(',').next() {
                let trimmed = first.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        return None;
    }
    connect_info.map(|a| a.ip().to_string())
}

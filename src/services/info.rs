use serde::Serialize;

use crate::config::API_VERSION;
use crate::db::SharedState;
use crate::error::ApiError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub location: Option<String>,
    pub max_sync_size: usize,
    pub message: String,
    pub status: i32,
    pub version: String,
}

pub const STATUS_ONLINE: i32 = 0;
pub const STATUS_NO_NEW_SYNCS: i32 = 1;
pub const STATUS_OFFLINE: i32 = 2;

pub async fn get_info(state: SharedState) -> Result<ServiceInfo, ApiError> {
    let cfg = &state.config;

    let mut status = if cfg.status.online {
        STATUS_ONLINE
    } else {
        STATUS_OFFLINE
    };
    if cfg.status.online {
        let accepting = if !cfg.status.allow_new_syncs {
            false
        } else if cfg.max_syncs == 0 {
            true
        } else {
            let count: i64 = state
                .conn()
                .query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0))?;
            count < cfg.max_syncs as i64
        };
        if !accepting {
            status = STATUS_NO_NEW_SYNCS;
        }
    }

    Ok(ServiceInfo {
        location: if cfg.location.is_empty() {
            None
        } else {
            Some(cfg.location.to_uppercase())
        },
        max_sync_size: cfg.max_sync_size,
        message: crate::config::strip_scripts(&cfg.status.message),
        status,
        version: API_VERSION.to_string(),
    })
}

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Wrap any displayable error into a 500 UnspecifiedException.
pub fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("invalid sync id")]
    InvalidSyncId,
    #[error("sync does not exist")]
    SyncNotFound,
    #[error("unable to find required data")]
    RequiredDataNotFound,
    #[error("the service is not accepting new syncs")]
    NewSyncsForbidden,
    #[error("client has exceeded the daily new syncs limit")]
    NewSyncsLimitExceeded,
    #[error("a sync conflict was detected")]
    SyncConflict,
    #[error("the service is currently offline")]
    ServiceNotAvailable,
    #[error("the requested route has not been implemented")]
    NotImplemented,
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::InvalidSyncId | Self::SyncNotFound => StatusCode::UNAUTHORIZED,
            Self::RequiredDataNotFound => StatusCode::BAD_REQUEST,
            Self::NewSyncsForbidden => StatusCode::METHOD_NOT_ALLOWED,
            Self::NewSyncsLimitExceeded => StatusCode::NOT_ACCEPTABLE,
            Self::SyncConflict => StatusCode::CONFLICT,
            Self::ServiceNotAvailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::NotImplemented => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidSyncId => "InvalidSyncIdException",
            Self::SyncNotFound => "SyncNotFoundException",
            Self::RequiredDataNotFound => "RequiredDataNotFoundException",
            Self::NewSyncsForbidden => "NewSyncsForbiddenException",
            Self::NewSyncsLimitExceeded => "NewSyncsLimitExceededException",
            Self::SyncConflict => "SyncConflictException",
            Self::ServiceNotAvailable => "ServiceNotAvailableException",
            Self::NotImplemented => "NotImplementedException",
            Self::Internal(_) => "UnspecifiedException",
        }
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!(error = %e, "sqlite error");
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.code();
        let message = match &self {
            Self::Internal(e) => e.clone(),
            _ => self.to_string(),
        };
        let body = json!({ "code": code, "message": message });
        (self.status(), axum::Json(body)).into_response()
    }
}

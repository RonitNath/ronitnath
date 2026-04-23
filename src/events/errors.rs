use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub(crate) enum EventError {
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid token")]
    InvalidToken,
    #[error("signup is closed")]
    SignupClosed,
    #[error("capacity reached")]
    CapacityReached,
    #[error("rsvp edits are closed")]
    RsvpClosed,
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl EventError {
    pub(crate) fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound | Self::InvalidToken => StatusCode::NOT_FOUND,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::SignupClosed | Self::CapacityReached | Self::RsvpClosed => StatusCode::CONFLICT,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(crate) fn public_message(&self) -> &'static str {
        match self {
            Self::NotFound | Self::InvalidToken => "not found",
            Self::Forbidden => "forbidden",
            Self::InvalidInput(_) => "invalid input",
            Self::SignupClosed => "signup is closed",
            Self::CapacityReached => "capacity reached",
            Self::RsvpClosed => "rsvp edits are closed",
            Self::Database(_) | Self::Internal(_) => "internal error",
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, EventError>;

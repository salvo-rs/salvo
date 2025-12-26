use salvo_core::http::StatusCode;

pub type TusResult<T> = Result<T, TusError>;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("missing tus-resumable")]
    MissingTusResumable,
    #[error("unsupported tus version: {0}")]
    UnsupportedTusVersion(String),
    #[error("missing header: {0}")]
    MissingHeader(&'static str),
    #[error("invalid integer header: {0}")]
    InvalidInt(&'static str),
    #[error("invalid upload-metadata")]
    InvalidMetadata,
    #[error("invalid content-type")]
    InvalidContentType,
}

#[derive(Debug, thiserror::Error)]
pub enum TusError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    #[error("upload not found")]
    NotFound,

    #[error("offset mismatch: expected {expected}, got {got}")]
    OffsetMismatch { expected: u64, got: u64 },

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("internal: {0}")]
    Internal(String),
}

impl TusError {
    pub fn status(&self) -> StatusCode {
        match self {
            TusError::Protocol(ProtocolError::MissingTusResumable) => StatusCode::PRECONDITION_FAILED, // 412
            TusError::Protocol(ProtocolError::UnsupportedTusVersion(_)) => StatusCode::PRECONDITION_FAILED,
            TusError::Protocol(_) => StatusCode::BAD_REQUEST,
            TusError::NotFound => StatusCode::NOT_FOUND,
            TusError::OffsetMismatch { .. } => StatusCode::CONFLICT, // 409
            TusError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE, // 413
            TusError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
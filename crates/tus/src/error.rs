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
    #[error("invalid content-type")]
    InvalidContentType,

    #[error("Concatenation extension is not (yet) supported. Disable parallel uploads in the tus client.")]
    UnsupportedConcatenationExtension,
    #[error("creation-defer-length extension is not (yet) supported.")]
    UnsupportedCreationDeferLengthExtension,
    #[error("Upload-Length or Upload-Defer-Length header required.")]
    InvalidLength,
    #[error("Upload-Metadata is invalid. It MUST consist of one or more comma-separated key-value pairs. The key and value MUST be separated by a space. The key MUST NOT contain spaces and commas and MUST NOT be empty. The key SHOULD be ASCII encoded and the value MUST be Base64 encoded. All keys MUST be unique")]
    InvalidMetadata,
    #[error("Maximum size exceeded")]
    ErrMaxSizeExceeded,
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

    #[error("failed to generate upload id")]
    GenerateIdError,

    #[error("failed to generate upload url, check your generate url function")]
    GenerateUploadURLError,

    #[error("internal: {0}")]
    Internal(String),
}

impl TusError {
    pub fn status(&self) -> StatusCode {
        match self {
            TusError::Protocol(ProtocolError::MissingTusResumable) => StatusCode::PRECONDITION_FAILED, // 412
            TusError::Protocol(ProtocolError::UnsupportedTusVersion(_)) => StatusCode::PRECONDITION_FAILED, // 412

            TusError::Protocol(ProtocolError::UnsupportedConcatenationExtension) => StatusCode::NOT_IMPLEMENTED, // 501
            TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension) => StatusCode::NOT_IMPLEMENTED, // 501
            TusError::Protocol(ProtocolError::InvalidLength) => StatusCode::BAD_REQUEST, // 400
            TusError::Protocol(ProtocolError::InvalidMetadata) => StatusCode::BAD_REQUEST, // 400
            TusError::Protocol(ProtocolError::ErrMaxSizeExceeded) => StatusCode::PAYLOAD_TOO_LARGE, // 413

            TusError::Protocol(_) => StatusCode::BAD_REQUEST, // 400
            TusError::NotFound => StatusCode::NOT_FOUND,
            TusError::OffsetMismatch { .. } => StatusCode::CONFLICT, // 409
            TusError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE, // 413
            TusError::GenerateIdError => StatusCode::INTERNAL_SERVER_ERROR, // 500
            TusError::GenerateUploadURLError => StatusCode::INTERNAL_SERVER_ERROR, //500
            TusError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

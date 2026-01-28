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

    #[error(
        "Concatenation extension is not (yet) supported. Disable parallel uploads in the tus client."
    )]
    UnsupportedConcatenationExtension,
    #[error("creation-defer-length extension is not (yet) supported.")]
    UnsupportedCreationDeferLengthExtension,
    #[error("creation-with-upload extension is not (yet) supported.")]
    UnsupportedCreationWithUploadExtension,
    #[error("termination extension is not (yet) supported.")]
    UnsupportedTerminationExtension,
    #[error("Upload-Length or Upload-Defer-Length header required.")]
    InvalidLength,
    #[error(
        "Upload-Metadata is invalid. It MUST consist of one or more comma-separated key-value pairs. The key and value MUST be separated by a space. The key MUST NOT contain spaces and commas and MUST NOT be empty. The key SHOULD be ASCII encoded and the value MUST be Base64 encoded. All keys MUST be unique"
    )]
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

    #[error("Upload-Offset conflict")]
    InvalidOffset,

    #[error("offset mismatch: expected {expected}, got {got}")]
    OffsetMismatch { expected: u64, got: u64 },

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("failed to generate upload id")]
    GenerateIdError,

    #[error("failed to generate upload url, check your generate url function")]
    GenerateUploadURLError,

    #[error("failed to get file id")]
    FileIdError,

    #[error("file no longer exists")]
    FileNoLongerExists,

    #[error("internal: {0}")]
    Internal(String),
}

impl TusError {
    pub fn status(&self) -> StatusCode {
        match self {
            TusError::Protocol(ProtocolError::MissingTusResumable) => {
                StatusCode::PRECONDITION_FAILED
            } // 412
            TusError::Protocol(ProtocolError::UnsupportedTusVersion(_)) => {
                StatusCode::PRECONDITION_FAILED
            } // 412

            TusError::Protocol(ProtocolError::UnsupportedConcatenationExtension) => {
                StatusCode::NOT_IMPLEMENTED
            } // 501
            TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension) => {
                StatusCode::NOT_IMPLEMENTED
            } // 501
            TusError::Protocol(ProtocolError::UnsupportedCreationWithUploadExtension) => {
                StatusCode::NOT_IMPLEMENTED
            } // 501
            TusError::Protocol(ProtocolError::UnsupportedTerminationExtension) => {
                StatusCode::NOT_IMPLEMENTED
            } // 501
            TusError::Protocol(ProtocolError::InvalidLength) => StatusCode::BAD_REQUEST, // 400
            TusError::Protocol(ProtocolError::InvalidMetadata) => StatusCode::BAD_REQUEST, // 400
            TusError::Protocol(ProtocolError::ErrMaxSizeExceeded) => StatusCode::PAYLOAD_TOO_LARGE, /* 413 */
            TusError::Protocol(ProtocolError::InvalidContentType) => {
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            } /* 415 */
            TusError::Protocol(_) => StatusCode::BAD_REQUEST, // 400

            TusError::FileNoLongerExists => StatusCode::GONE, // 410
            TusError::FileIdError => StatusCode::BAD_REQUEST, // 400
            TusError::NotFound => StatusCode::NOT_FOUND,
            TusError::OffsetMismatch { .. } => StatusCode::CONFLICT, // 409
            TusError::InvalidOffset => StatusCode::CONFLICT,         // 409
            TusError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE, // 413
            TusError::GenerateIdError => StatusCode::INTERNAL_SERVER_ERROR, // 500
            TusError::GenerateUploadURLError => StatusCode::INTERNAL_SERVER_ERROR, // 500
            TusError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_error_display() {
        assert_eq!(
            ProtocolError::MissingTusResumable.to_string(),
            "missing tus-resumable"
        );
        assert_eq!(
            ProtocolError::UnsupportedTusVersion("2.0.0".to_string()).to_string(),
            "unsupported tus version: 2.0.0"
        );
        assert_eq!(
            ProtocolError::MissingHeader("Upload-Length").to_string(),
            "missing header: Upload-Length"
        );
        assert_eq!(
            ProtocolError::InvalidInt("Upload-Offset").to_string(),
            "invalid integer header: Upload-Offset"
        );
        assert_eq!(
            ProtocolError::InvalidContentType.to_string(),
            "invalid content-type"
        );
        assert_eq!(
            ProtocolError::InvalidLength.to_string(),
            "Upload-Length or Upload-Defer-Length header required."
        );
        assert_eq!(
            ProtocolError::ErrMaxSizeExceeded.to_string(),
            "Maximum size exceeded"
        );
    }

    #[test]
    fn test_protocol_error_unsupported_extensions() {
        assert!(
            ProtocolError::UnsupportedConcatenationExtension
                .to_string()
                .contains("Concatenation extension")
        );
        assert!(
            ProtocolError::UnsupportedCreationDeferLengthExtension
                .to_string()
                .contains("creation-defer-length")
        );
        assert!(
            ProtocolError::UnsupportedCreationWithUploadExtension
                .to_string()
                .contains("creation-with-upload")
        );
        assert!(
            ProtocolError::UnsupportedTerminationExtension
                .to_string()
                .contains("termination")
        );
    }

    #[test]
    fn test_tus_error_display() {
        assert_eq!(TusError::NotFound.to_string(), "upload not found");
        assert_eq!(
            TusError::InvalidOffset.to_string(),
            "Upload-Offset conflict"
        );
        assert_eq!(
            TusError::OffsetMismatch {
                expected: 100,
                got: 50
            }
            .to_string(),
            "offset mismatch: expected 100, got 50"
        );
        assert_eq!(TusError::PayloadTooLarge.to_string(), "payload too large");
        assert_eq!(
            TusError::GenerateIdError.to_string(),
            "failed to generate upload id"
        );
        assert_eq!(
            TusError::GenerateUploadURLError.to_string(),
            "failed to generate upload url, check your generate url function"
        );
        assert_eq!(TusError::FileIdError.to_string(), "failed to get file id");
        assert_eq!(
            TusError::FileNoLongerExists.to_string(),
            "file no longer exists"
        );
        assert_eq!(
            TusError::Internal("test error".to_string()).to_string(),
            "internal: test error"
        );
    }

    #[test]
    fn test_tus_error_from_protocol_error() {
        let protocol_error = ProtocolError::MissingTusResumable;
        let tus_error: TusError = protocol_error.into();
        assert!(matches!(
            tus_error,
            TusError::Protocol(ProtocolError::MissingTusResumable)
        ));
    }

    #[test]
    fn test_protocol_error_status_codes() {
        // 412 Precondition Failed
        assert_eq!(
            TusError::Protocol(ProtocolError::MissingTusResumable).status(),
            StatusCode::PRECONDITION_FAILED
        );
        assert_eq!(
            TusError::Protocol(ProtocolError::UnsupportedTusVersion("2.0".into())).status(),
            StatusCode::PRECONDITION_FAILED
        );

        // 501 Not Implemented
        assert_eq!(
            TusError::Protocol(ProtocolError::UnsupportedConcatenationExtension).status(),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            TusError::Protocol(ProtocolError::UnsupportedCreationDeferLengthExtension).status(),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            TusError::Protocol(ProtocolError::UnsupportedCreationWithUploadExtension).status(),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            TusError::Protocol(ProtocolError::UnsupportedTerminationExtension).status(),
            StatusCode::NOT_IMPLEMENTED
        );

        // 400 Bad Request
        assert_eq!(
            TusError::Protocol(ProtocolError::InvalidLength).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            TusError::Protocol(ProtocolError::InvalidMetadata).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            TusError::Protocol(ProtocolError::MissingHeader("test")).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            TusError::Protocol(ProtocolError::InvalidInt("test")).status(),
            StatusCode::BAD_REQUEST
        );

        // 413 Payload Too Large
        assert_eq!(
            TusError::Protocol(ProtocolError::ErrMaxSizeExceeded).status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );

        // 415 Unsupported Media Type
        assert_eq!(
            TusError::Protocol(ProtocolError::InvalidContentType).status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }

    #[test]
    fn test_tus_error_status_codes() {
        // 404 Not Found
        assert_eq!(TusError::NotFound.status(), StatusCode::NOT_FOUND);

        // 409 Conflict
        assert_eq!(TusError::InvalidOffset.status(), StatusCode::CONFLICT);
        assert_eq!(
            TusError::OffsetMismatch {
                expected: 10,
                got: 5
            }
            .status(),
            StatusCode::CONFLICT
        );

        // 410 Gone
        assert_eq!(TusError::FileNoLongerExists.status(), StatusCode::GONE);

        // 400 Bad Request
        assert_eq!(TusError::FileIdError.status(), StatusCode::BAD_REQUEST);

        // 413 Payload Too Large
        assert_eq!(
            TusError::PayloadTooLarge.status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );

        // 500 Internal Server Error
        assert_eq!(
            TusError::GenerateIdError.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            TusError::GenerateUploadURLError.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            TusError::Internal("error".to_string()).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_tus_result_type() {
        let success: TusResult<i32> = Ok(42);
        assert_eq!(success.unwrap(), 42);

        let failure: TusResult<i32> = Err(TusError::NotFound);
        assert!(failure.is_err());
    }
}

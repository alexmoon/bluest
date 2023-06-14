use objc_foundation::INSString;
use objc_id::ShareId;

use super::types::CBError;
pub use super::types::NSError;
use crate::error::{AttError, ErrorKind};

impl crate::Error {
    pub(super) fn from_recv_error(err: tokio::sync::broadcast::error::RecvError) -> Self {
        crate::Error::new(
            ErrorKind::Internal,
            Some(Box::new(err)),
            "receiving delegate event".to_string(),
        )
    }

    pub(super) fn from_stream_recv_error(err: tokio_stream::wrappers::errors::BroadcastStreamRecvError) -> Self {
        crate::Error::new(
            ErrorKind::Internal,
            Some(Box::new(err)),
            "receiving delegate event".to_string(),
        )
    }

    pub(super) fn from_nserror(err: ShareId<NSError>) -> Self {
        crate::Error::new(
            kind_from_nserror(&err),
            Some(Box::new(NSErrorError(err))),
            String::new(),
        )
    }

    pub(super) fn from_kind_and_nserror(kind: ErrorKind, err: Option<ShareId<NSError>>) -> Self {
        match err {
            Some(err) => crate::Error::new(kind, Some(Box::new(NSErrorError(err))), String::new()),
            None => kind.into(),
        }
    }
}

fn kind_from_nserror(value: &NSError) -> ErrorKind {
    if value.domain().as_str() == "CBErrorDomain" {
        match CBError(value.code()) {
            CBError::OPERATION_NOT_SUPPORTED => ErrorKind::NotSupported,
            CBError::NOT_CONNECTED | CBError::PERIPHERAL_DISCONNECTED => ErrorKind::NotConnected,
            CBError::CONNECTION_TIMEOUT | CBError::ENCRYPTION_TIMED_OUT => ErrorKind::Timeout,
            CBError::INVALID_PARAMETERS
            | CBError::INVALID_HANDLE
            | CBError::UUID_NOT_ALLOWED
            | CBError::UNKOWN_DEVICE => ErrorKind::InvalidParameter,
            CBError::CONNECTION_FAILED
            | CBError::PEER_REMOVED_PAIRING_INFORMATION
            | CBError::CONNECTION_LIMIT_REACHED
            | CBError::TOO_MANY_LE_PAIRED_DEVICES => ErrorKind::ConnectionFailed,
            CBError::UNKNOWN | CBError::OUT_OF_SPACE | CBError::OPERATION_CANCELLED | CBError::ALREADY_ADVERTISING => {
                ErrorKind::Other
            }
            _ => ErrorKind::Other,
        }
    } else if value.domain().as_str() == "CBATTErrorDomain" {
        let n = value.code();
        if let Ok(n) = u8::try_from(n) {
            ErrorKind::Protocol(AttError::from(n))
        } else {
            ErrorKind::Other
        }
    } else {
        ErrorKind::Other
    }
}

struct NSErrorError(ShareId<NSError>);

impl std::fmt::Debug for NSErrorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl std::fmt::Display for NSErrorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.localized_description().as_str())
    }
}

impl std::error::Error for NSErrorError {}

impl std::ops::Deref for NSErrorError {
    type Target = NSError;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

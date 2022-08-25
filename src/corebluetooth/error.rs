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
            kind_from_nserror(&*err),
            Some(Box::new(OsError { inner: err })),
            String::new(),
        )
    }
}

fn kind_from_nserror(value: &NSError) -> ErrorKind {
    if value.domain().as_str() == "CBErrorDomain" {
        match CBError::from(value.code()) {
            CBError::OperationNotSupported => ErrorKind::NotSupported,
            CBError::NotConnected | CBError::PeripheralDisconnected => ErrorKind::NotConnected,
            CBError::ConnectionTimeout | CBError::EncryptionTimedOut => ErrorKind::Timeout,
            CBError::InvalidParameters | CBError::InvalidHandle | CBError::UuidNotAllowed | CBError::UnkownDevice => {
                ErrorKind::InvalidParameter
            }
            CBError::ConnectionFailed
            | CBError::PeerRemovedPairingInformation
            | CBError::ConnectionLimitReached
            | CBError::TooManyLEPairedDevices => ErrorKind::ConnectionFailed,
            CBError::Unknown | CBError::OutOfSpace | CBError::OperationCancelled | CBError::AlreadyAdvertising => {
                ErrorKind::Other
            }
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

pub struct OsError {
    inner: ShareId<NSError>,
}

impl std::fmt::Debug for OsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.inner, f)
    }
}

impl std::fmt::Display for OsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.localized_description().as_str())
    }
}

impl std::error::Error for OsError {}

impl std::ops::Deref for OsError {
    type Target = NSError;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

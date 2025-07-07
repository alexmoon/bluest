use corebluetooth::error::{CBATTError, CBError};

use crate::error::{AttError, ErrorKind};

impl From<async_broadcast::RecvError> for crate::Error {
    fn from(err: async_broadcast::RecvError) -> Self {
        crate::Error::new(ErrorKind::Internal, Some(Box::new(err)), "receiving delegate event")
    }
}

impl From<corebluetooth::Error> for crate::Error {
    fn from(err: corebluetooth::Error) -> Self {
        crate::Error::new(err.kind().into(), Some(Box::new(err)), String::new())
    }
}

impl From<corebluetooth::error::ErrorKind> for ErrorKind {
    fn from(value: corebluetooth::error::ErrorKind) -> Self {
        match value {
            corebluetooth::error::ErrorKind::Bluetooth(code) => match code {
                CBError::OperationNotSupported => ErrorKind::NotSupported,
                CBError::NotConnected | CBError::PeripheralDisconnected => ErrorKind::NotConnected,
                CBError::ConnectionTimeout | CBError::EncryptionTimedOut => ErrorKind::Timeout,
                CBError::InvalidParameters
                | CBError::InvalidHandle
                | CBError::UUIDNotAllowed
                | CBError::UnknownDevice => ErrorKind::InvalidParameter,
                CBError::ConnectionFailed
                | CBError::PeerRemovedPairingInformation
                | CBError::ConnectionLimitReached
                | CBError::TooManyLEPairedDevices => ErrorKind::ConnectionFailed,
                CBError::Unknown | CBError::OutOfSpace | CBError::OperationCancelled | CBError::AlreadyAdvertising => {
                    ErrorKind::Other
                }
                _ => ErrorKind::Other,
            },
            corebluetooth::error::ErrorKind::ATT(CBATTError(code)) => {
                if let Ok(code) = u8::try_from(code) {
                    ErrorKind::Protocol(AttError::from(code))
                } else {
                    ErrorKind::Other
                }
            }
            corebluetooth::error::ErrorKind::Other => ErrorKind::Other,
        }
    }
}

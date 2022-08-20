use windows::{Devices::Bluetooth::GenericAttributeProfile::GattCommunicationStatus, Foundation::IReference};

use crate::error::ErrorKind;
use crate::Result;

/// Platform specific error type
pub type OsError = windows::core::Error;

impl TryFrom<&OsError> for ErrorKind {
    type Error = ();

    fn try_from(_value: &OsError) -> Result<Self, Self::Error> {
        // No conversions from windows::core::Error to bluest::Error are currently known
        Err(())
    }
}

impl From<OsError> for crate::Error {
    fn from(err: OsError) -> Self {
        crate::Error::new(
            (&err).try_into().unwrap_or(ErrorKind::Other),
            Some(Box::new(err)),
            String::new(),
        )
    }
}

pub(super) fn check_communication_status(
    status: GattCommunicationStatus,
    protocol_error: windows::core::Result<IReference<u8>>,
    message: &str,
) -> Result<()> {
    use crate::Error;
    match status {
        GattCommunicationStatus::Success => Ok(()),
        GattCommunicationStatus::AccessDenied => Err(Error::new(ErrorKind::NotAuthorized, None, message.to_string())),
        GattCommunicationStatus::Unreachable => Err(Error::new(ErrorKind::ConnectionFailed, None, message.to_string())),
        GattCommunicationStatus::ProtocolError => {
            let code = protocol_error?.Value()?;
            Err(Error::new(ErrorKind::Protocol(code.into()), None, message.to_string()))
        }
        _ => Err(Error::new(ErrorKind::Other, None, message.to_string())),
    }
}

use windows::Devices::Bluetooth::GenericAttributeProfile::GattCommunicationStatus;
use windows::Foundation::IReference;

use crate::error::ErrorKind;
use crate::Result;

impl From<windows::core::Error> for crate::Error {
    fn from(err: windows::core::Error) -> Self {
        crate::Error::new(ErrorKind::Other, Some(Box::new(err)), String::new())
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

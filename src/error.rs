//! Bluest errors

use num_enum::TryFromPrimitive;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "{}", &self.kind)
        } else {
            write!(f, "{}: {}", &self.kind, &self.message)
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, displaydoc::Display, Clone, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// an unknown error occured
    Unknown,
    /// invalid parameters for Bluetooth operation
    InvalidParameters,
    /// invalid handle for Bluetooth operation
    InvalidHandle,
    /// the Bluetooth device isn't connected
    NotConnected,
    /// Bluetooth out of space
    OutOfSpace,
    /// Bluetooth operation was cancelled
    OperationCancelled,
    /// Bluetooth connection timed out
    ConnectionTimeout,
    /// Bluetooth device disconnected
    PeripheralDisconnected,
    /// the provided UUID is not allowed
    UuidNotAllowed,
    /// the Bluetooth device is already advertising
    AlreadyAdvertising,
    /// the Bluetooth connection failed
    ConnectionFailed,
    /// the Bluetooth device has reached the maximum number of connections
    ConnectionLimitReached,
    /// the Bluetooth device is unknown
    UnkownDevice,
    /// the Bluetooth operation is unsupported
    OperationNotSupported,
    /// the Bluetooth device has removed pairing information
    PeerRemovedPairingInformation,
    /// Bluetooth encryption timed out
    EncryptionTimedOut,
    /// too many Bluetooth LE devices have been paired
    TooManyLEPairedDevices,
    /// Bluetooth adapter not available
    AdapterUnavailable,
    /// the Bluetooth adapter is already scanning
    AlreadyScanning,
    /// internal error
    InternalError,
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Error {
            kind,
            message: String::new(),
        }
    }
}

/// Bluetooth Attribute Protocol error codes. See the Bluetooth Core Specification, Vol 3, Part F, §3.4.1.1
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
pub enum AttErrorCode {
    /// The operation completed successfully.
    Success = 0x00,
    /// The attribute handle given was not valid on this server.
    InvalidHandle = 0x01,
    /// The attribute cannot be read.
    ReadNotPermitted = 0x02,
    /// The attribute cannot be written.
    WriteNotPermitted = 0x03,
    /// The attribute PDU was invalid.
    InvalidPdu = 0x04,
    /// The attribute requires authentication before it can be read or written.
    InsufficientAuthentication = 0x05,
    /// Attribute server does not support the request received from the client.
    RequestNotSupported = 0x06,
    /// Offset specified was past the end of the attribute.
    InvalidOffset = 0x07,
    /// The attribute requires authorization before it can be read or written.
    InsufficientAuthorization = 0x08,
    /// Too many prepare writes have been queued.
    PrepareQueueFull = 0x09,
    /// No attribute found within the given attribute handle range.
    AttributeNotFound = 0x0a,
    /// The attribute cannot be read or written using the Read Blob Request.
    AttributeNotLong = 0x0b,
    /// The Encryption Key Size used for encrypting this link is insufficient.
    InsufficientEncryptionKeySize = 0x0c,
    /// The attribute value length is invalid for the operation.
    InvalidAttributeValueLength = 0x0d,
    /// The attribute request that was requested has encountered an error that was unlikely, and therefore could not
    /// be completed as requested.
    UnlikelyError = 0x0e,
    /// The attribute requires encryption before it can be read or written.
    InsufficientEncryption = 0x0f,
    /// The attribute type is not a supported grouping attribute as defined by a higher layer specification.
    UnsupportedGroupType = 0x10,
    /// Insufficient Resources to complete the request.
    InsufficientResources = 0x11,
    /// The server requests the client to rediscover the database.
    DatabaseOutOfSync = 0x12,
    /// The attribute parameter value was not allowed.
    ValueNotAllowed = 0x13,
    /// Write Request Rejected
    WriteRequestRejected = 0xfc,
    /// Client Characteristic Configuration Descriptor Improperly Configured
    CccdImproperlyConfigured = 0xfd,
    /// Procedure Already in Progress
    ProcedureAlreadyInProgress = 0xfe,
    /// Out of Range
    OutOfRange = 0xff,
}

/// Bluetooth Attribute Protocol error. See the Bluetooth Core Specification, Vol 3, Part F, §3.4.1.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttError {
    /// Known error codes defined by the Bluetooth specification.
    Known(AttErrorCode),
    /// Application error code defined by a higher layer specification. Values range from 0x80-0x9f.
    Application(u8),
    /// Reserved or unknown error code.
    Reserved(u8),
}

impl From<u8> for AttError {
    fn from(number: u8) -> Self {
        match AttErrorCode::try_from(number) {
            Ok(code) => AttError::Known(code),
            Err(_) => {
                if (0x80..0xa0).contains(&number) {
                    AttError::Application(number)
                } else {
                    AttError::Reserved(number)
                }
            }
        }
    }
}

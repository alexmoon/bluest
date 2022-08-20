//! Bluest errors

use num_enum::TryFromPrimitive;

/// The error type for Bluetooth operations
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    message: String,
}

impl Error {
    pub(crate) fn new(
        kind: ErrorKind,
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
        message: String,
    ) -> Self {
        Error { kind, source, message }
    }

    /// Returns the corresponding [ErrorKind] for this error.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the message for this error.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.message.is_empty(), &self.source) {
            (true, None) => write!(f, "{}", &self.kind),
            (false, None) => write!(f, "{}: {}", &self.kind, &self.message),
            (true, Some(err)) => write!(f, "{}: {} ({})", &self.kind, &self.message, err),
            (false, Some(err)) => write!(f, "{}: {}", &self.kind, err),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|x| {
            let x: &(dyn std::error::Error + 'static) = &**x;
            x
        })
    }
}

/// A list of general categories of Bluetooth error.
#[non_exhaustive]
#[derive(Debug, displaydoc::Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorKind {
    /// the Bluetooth adapter is not available
    AdapterUnavailable,
    /// the Bluetooth adapter is already scanning
    AlreadyScanning,
    /// connection failed
    ConnectionFailed,
    /// the Bluetooth device isn't connected
    NotConnected,
    /// the Bluetooth operation is unsupported
    NotSupported,
    /// permission denied
    NotAuthorized,
    /// not ready
    NotReady,
    /// not found
    NotFound,
    /// invalid paramter
    InvalidParameter,
    /// timed out
    Timeout,
    /// protocol error: {0}
    Protocol(AttError),
    /// an internal error has occured
    Internal,
    /// error
    Other,
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Error {
            kind,
            source: None,
            message: String::new(),
        }
    }
}

/// Bluetooth Attribute Protocol error codes. See the Bluetooth Core Specification, Vol 3, Part F, §3.4.1.1
#[repr(u8)]
#[derive(Debug, displaydoc::Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, TryFromPrimitive)]
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
    /// The attribute request that was requested has encountered an error that was unlikely, and therefore could not be completed as requested.
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
#[derive(Debug, displaydoc::Display, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttError {
    /// {0}
    Known(AttErrorCode),
    /// application specific error: {0}
    Application(u8),
    /// unknown error: {0}
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

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ATTError {
    Success,
    InvalidHandle,
    ReadNotPermitted,
    WriteNotPermitted,
    InvalidPdu,
    InsufficientAuthentication,
    RequestNotSupported,
    InvalidOffset,
    InsufficientAuthorization,
    PrepareQueueFull,
    AttributeNotFound,
    AttributeNotLong,
    InsufficientEncryptionKeySize,
    InvalidAttributeValueLength,
    UnlikelyError,
    InsufficientEncryption,
    UnsupportedGroupType,
    InsufficientResources,
    DatabaseOutOfSync,
    ValueNotAllowed,
    Application(u8),
    Common(u8),
    Reserved(u8),
}

impl From<u8> for ATTError {
    fn from(number: u8) -> Self {
        const SUCCESS: u8 = 0x00;
        const INVALID_HANDLE: u8 = 0x01;
        const READ_NOT_PERMITTED: u8 = 0x02;
        const WRITE_NOT_PERMITTED: u8 = 0x03;
        const INVALID_PDU: u8 = 0x04;
        const INSUFFICIENT_AUTHENTICATION: u8 = 0x05;
        const REQUEST_NOT_SUPPORTED: u8 = 0x06;
        const INVALID_OFFSET: u8 = 0x07;
        const INSUFFICIENT_AUTHORIZATION: u8 = 0x08;
        const PREPARE_QUEUE_FULL: u8 = 0x09;
        const ATTRIBUTE_NOT_FOUND: u8 = 0x0A;
        const ATTRIBUTE_NOT_LONG: u8 = 0x0B;
        const INSUFFICIENT_ENCRYPTION_KEY_SIZE: u8 = 0x0C;
        const INVALID_ATTRIBUTE_VALUE_LENGTH: u8 = 0x0D;
        const UNLIKELY_ERROR: u8 = 0x0E;
        const INSUFFICIENT_ENCRYPTION: u8 = 0x0F;
        const UNSUPPORTED_GROUP_TYPE: u8 = 0x10;
        const INSUFFICIENT_RESOURCES: u8 = 0x11;
        const DATABASE_OUT_OF_SYNC: u8 = 0x12;
        const VALUE_NOT_ALLOWED: u8 = 0x13;

        #[deny(unreachable_patterns)]
        match number {
            SUCCESS => Self::Success,
            INVALID_HANDLE => Self::InvalidHandle,
            READ_NOT_PERMITTED => Self::ReadNotPermitted,
            WRITE_NOT_PERMITTED => Self::WriteNotPermitted,
            INVALID_PDU => Self::InvalidPdu,
            INSUFFICIENT_AUTHENTICATION => Self::InsufficientAuthentication,
            REQUEST_NOT_SUPPORTED => Self::RequestNotSupported,
            INVALID_OFFSET => Self::InvalidOffset,
            INSUFFICIENT_AUTHORIZATION => Self::InsufficientAuthorization,
            PREPARE_QUEUE_FULL => Self::PrepareQueueFull,
            ATTRIBUTE_NOT_FOUND => Self::AttributeNotFound,
            ATTRIBUTE_NOT_LONG => Self::AttributeNotLong,
            INSUFFICIENT_ENCRYPTION_KEY_SIZE => Self::InsufficientEncryptionKeySize,
            INVALID_ATTRIBUTE_VALUE_LENGTH => Self::InvalidAttributeValueLength,
            UNLIKELY_ERROR => Self::UnlikelyError,
            INSUFFICIENT_ENCRYPTION => Self::InsufficientEncryption,
            UNSUPPORTED_GROUP_TYPE => Self::UnsupportedGroupType,
            INSUFFICIENT_RESOURCES => Self::InsufficientResources,
            DATABASE_OUT_OF_SYNC => Self::DatabaseOutOfSync,
            VALUE_NOT_ALLOWED => Self::ValueNotAllowed,
            0x80..=0x9f => Self::Application(number),
            0xe0..=0xff => Self::Common(number),
            _ => Self::Reserved(number),
        }
    }
}

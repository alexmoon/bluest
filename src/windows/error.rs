use crate::{error::ErrorKind, Error};

impl From<windows::core::Error> for Error {
    fn from(_: windows::core::Error) -> Self {
        Error {
            kind: ErrorKind::Unknown,
            message: String::new(),
        }
    }
}

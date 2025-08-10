use java_spaghetti::{CastError, Local};

use self::bindings::java::lang::Throwable;
use crate::error::{AttError, ErrorKind};

pub mod adapter;
pub mod characteristic;
pub mod descriptor;
pub mod device;
pub mod l2cap_channel;
pub mod service;

// **NOTE**: it is important to use `jni_get_vm` or `jni_with_env` instead of `Global::vm`
// so that a few bugs in `java-spaghetti` 0.2.0 may be avoided.
pub(crate) mod async_util;
pub(crate) mod bindings;
pub(crate) mod callback;
pub(crate) mod event_receiver;
pub(crate) mod gatt_tree;
pub(crate) mod jni;
pub(crate) mod vm_context;

/// A platform-specific device identifier.
/// On android it contains the Bluetooth address in the format `AB:CD:EF:01:23:45`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(pub(crate) String);

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

trait UuidExt {
    fn from_java(value: java_spaghetti::Ref<'_, bindings::java::util::UUID>) -> Result<uuid::Uuid, crate::Error>;
}

impl UuidExt for uuid::Uuid {
    fn from_java(value: java_spaghetti::Ref<'_, bindings::java::util::UUID>) -> Result<Self, crate::Error> {
        uuid::Uuid::parse_str(value.toString()?.non_null()?.to_string_lossy().trim())
            .map_err(|e| crate::Error::new(ErrorKind::Internal, Some(Box::new(e)), "Uuid::parse_str failed"))
    }
}

impl From<Local<'_, Throwable>> for crate::Error {
    fn from(e: Local<'_, Throwable>) -> Self {
        Self::new(ErrorKind::Internal, None, format!("{e:?}"))
    }
}

impl From<CastError> for crate::Error {
    fn from(e: CastError) -> Self {
        Self::new(ErrorKind::Internal, None, format!("{e:?}"))
    }
}

impl From<AttError> for crate::Error {
    fn from(e: AttError) -> Self {
        ErrorKind::Protocol(e).into()
    }
}

struct JavaIterator<'env>(Local<'env, bindings::java::util::Iterator>);

impl<'env> Iterator for JavaIterator<'env> {
    type Item = Local<'env, bindings::java::lang::Object>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.hasNext().unwrap() {
            let obj = self.0.next().unwrap().unwrap();
            // upgrade lifetime to the original env.
            let obj = unsafe { Local::from_raw(self.0.env(), obj.into_raw()) };
            Some(obj)
        } else {
            None
        }
    }
}

// TODO: make use of the caller information in these track caller methods.

trait OptionExt<T> {
    fn non_null(self) -> Result<T, crate::Error>;
    fn ok_or_check_conn(self, dev_id: &DeviceId) -> Result<T, crate::Error>;
}

impl<T> OptionExt<T> for Option<T> {
    #[track_caller]
    fn non_null(self) -> Result<T, crate::Error> {
        self.ok_or_else(|| crate::Error::new(ErrorKind::Internal, None, "Java call unexpectedly returned null."))
    }

    #[track_caller]
    fn ok_or_check_conn(self, dev_id: &DeviceId) -> Result<T, crate::Error> {
        self.ok_or_else(|| {
            if gatt_tree::GattTree::find_connection(dev_id).is_none() {
                ErrorKind::NotConnected.into()
            } else {
                ErrorKind::ServiceChanged.into()
            }
        })
    }
}

trait BoolExt {
    fn non_false(self) -> Result<(), crate::Error>;
}

impl BoolExt for bool {
    #[track_caller]
    fn non_false(self) -> Result<(), crate::Error> {
        self.then_some(()).ok_or_else(|| {
            crate::Error::new(
                ErrorKind::Internal,
                None,
                "Java call returned false (please ensure that you have the required permission).",
            )
        })
    }
}

// See <https://developer.android.com/reference/android/bluetooth/BluetoothStatusCodes>.
trait IntExt {
    fn check_status_code(self) -> Result<(), crate::Error>;
}

impl IntExt for i32 {
    #[track_caller]
    fn check_status_code(self) -> Result<(), crate::Error> {
        use bindings::android::bluetooth::BluetoothStatusCodes;

        use crate::Error;
        if self == BluetoothStatusCodes::SUCCESS {
            return Ok(());
        }
        Err(match self {
            BluetoothStatusCodes::ERROR_BLUETOOTH_NOT_ALLOWED => Error::new(
                ErrorKind::NotAuthorized,
                None,
                "BluetoothStatusCodes.ERROR_BLUETOOTH_NOT_ALLOWED",
            ),
            BluetoothStatusCodes::ERROR_BLUETOOTH_NOT_ENABLED => {
                Error::new(ErrorKind::AdapterUnavailable, None, "bluetooth is disabled")
            }
            BluetoothStatusCodes::ERROR_DEVICE_NOT_BONDED => {
                Error::new(ErrorKind::NotAuthorized, None, "please pair with the device")
            }
            BluetoothStatusCodes::ERROR_GATT_WRITE_NOT_ALLOWED => {
                ErrorKind::Protocol(AttError::WRITE_NOT_PERMITTED).into()
            }
            BluetoothStatusCodes::ERROR_GATT_WRITE_REQUEST_BUSY => Error::new(
                ErrorKind::NotReady,
                None,
                "BluetoothStatusCodes.ERROR_GATT_WRITE_REQUEST_BUSY",
            ),
            BluetoothStatusCodes::ERROR_MISSING_BLUETOOTH_CONNECT_PERMISSION => Error::new(
                ErrorKind::NotAuthorized,
                None,
                "please request the permission for bluetooth connections",
            ),
            BluetoothStatusCodes::ERROR_PROFILE_SERVICE_NOT_BOUND => Error::new(
                ErrorKind::Other,
                None,
                "BluetoothStatusCodes.ERROR_PROFILE_SERVICE_NOT_BOUND",
            ),
            _ => Error::new(ErrorKind::Other, None, format!("status code {self}")),
        })
    }
}

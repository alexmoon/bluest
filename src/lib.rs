pub mod btuuid;
pub mod error;
pub mod sys;
pub mod types;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod corebluetooth;
#[cfg(target_os = "windows")]
mod windows;

// Dependency re-exports
pub use smallvec;
pub use uuid;

pub use error::Error;
pub type Result<T, E = Error> = core::result::Result<T, E>;

pub use btuuid::BluetoothUuidExt;

pub use sys::adapter::Adapter;
pub use sys::characteristic::Characteristic;
pub use sys::descriptor::Descriptor;
pub use sys::device::Device;
pub use sys::service::Service;
pub use sys::session::Session;
pub use types::*;

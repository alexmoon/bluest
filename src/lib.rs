pub mod btuuid;
pub mod error;
pub mod sys;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod corebluetooth;

// Dependency re-exports
pub use smallvec;
pub use uuid;

pub use error::Error;
pub type Result<T, E = Error> = core::result::Result<T, E>;

pub use btuuid::BluetoothUuidExt;

pub use sys::adapter::{Adapter, AdvertisementData, DiscoveredDevice};
pub use sys::characteristic::Characteristic;
pub use sys::descriptor::Descriptor;
pub use sys::device::Device;
pub use sys::service::Service;
pub use sys::session::Session;

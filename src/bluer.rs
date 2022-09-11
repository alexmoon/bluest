pub mod adapter;
pub mod characteristic;
pub mod descriptor;
pub mod device;
pub mod service;

mod error;

/// A platform-specific device identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(bluer::Address);

pub mod adapter;
pub mod characteristic;
pub mod descriptor;
pub mod device;
pub mod l2cap_channel;
pub mod service;
pub mod advertisement;

mod error;

/// A platform-specific device identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(bluer::Address);

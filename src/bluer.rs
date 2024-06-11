pub mod adapter;
pub mod characteristic;
pub mod descriptor;
pub mod device;
pub mod service;

#[cfg(feature = "l2cap")]
pub mod l2cap_channel;

mod error;

/// A platform-specific device identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(bluer::Address);

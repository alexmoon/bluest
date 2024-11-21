use crate::Uuid;

pub mod adapter;
pub mod characteristic;
pub mod descriptor;
pub mod device;
pub mod error;
pub mod l2cap_channel;
pub mod service;
pub mod advertisement;

mod delegates;
mod types;

/// A platform-specific device identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(Uuid);

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

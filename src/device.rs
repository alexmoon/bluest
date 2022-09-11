#![allow(clippy::let_unit_value)]

use crate::pairing::PairingAgent;
use crate::{sys, DeviceId, Result, Service, Uuid};

/// A Bluetooth LE device
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Device(pub(crate) sys::device::DeviceImpl);

impl std::fmt::Display for Device {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl Device {
    /// This device's unique identifier
    #[inline]
    pub fn id(&self) -> DeviceId {
        self.0.id()
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    ///
    /// # Panics
    ///
    /// On Linux, this method will panic if there is a current Tokio runtime and it is single-threaded or if there is
    /// no current Tokio runtime and creating one fails.
    #[inline]
    pub fn name(&self) -> Result<String> {
        self.0.name()
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    #[inline]
    pub async fn name_async(&self) -> Result<String> {
        self.0.name_async().await
    }

    /// The connection status for this device
    #[inline]
    pub async fn is_connected(&self) -> bool {
        self.0.is_connected().await
    }

    /// The pairing status for this device
    #[inline]
    pub async fn is_paired(&self) -> Result<bool> {
        self.0.is_paired().await
    }

    /// Attempt to pair this device using the system default pairing UI
    ///
    /// # Platform specific
    ///
    /// ## MacOS/iOS
    ///
    /// Device pairing is performed automatically by the OS when a characteristic requiring security is accessed. This
    /// method is a no-op.
    ///
    /// ## Windows
    ///
    /// This will fail unless it is called from a UWP application.
    #[inline]
    pub async fn pair(&self) -> Result<()> {
        self.0.pair().await
    }

    /// Attempt to pair this device using the system default pairing UI
    ///
    /// # Platform specific
    ///
    /// On MacOS/iOS, device pairing is performed automatically by the OS when a characteristic requiring security is
    /// accessed. This method is a no-op.
    #[inline]
    pub async fn pair_with_agent<T: PairingAgent + 'static>(&self, agent: &T) -> Result<()> {
        self.0.pair_with_agent(agent).await
    }

    /// Discover the primary services of this device.
    #[inline]
    pub async fn discover_services(&self) -> Result<Vec<Service>> {
        self.0.discover_services().await
    }

    /// Discover the primary service(s) of this device with the given [`Uuid`].
    #[inline]
    pub async fn discover_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        self.0.discover_services_with_uuid(uuid).await
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this method may either perform service discovery or return an error.
    #[inline]
    pub async fn services(&self) -> Result<Vec<Service>> {
        self.0.services().await
    }

    /// Asynchronously blocks until a GATT services changed packet is received
    #[inline]
    pub async fn services_changed(&self) -> Result<()> {
        self.0.services_changed().await
    }

    /// Get the current signal strength from the device in dBm.
    ///
    /// # Platform specific
    ///
    /// Returns [ErrorKind::NotSupported] on Windows and Linux.
    #[inline]
    pub async fn rssi(&self) -> Result<i16> {
        self.0.rssi().await
    }
}

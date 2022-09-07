use tracing::error;
use windows::core::{GUID, HSTRING};
use windows::Devices::Bluetooth::{
    BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
};
use windows::Foundation::TypedEventHandler;

use super::error::check_communication_status;
use super::service::Service;
use crate::error::ErrorKind;
use crate::util::defer;
use crate::{Result, Uuid};

/// A platform-specific device identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(pub(super) std::ffi::OsString);

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0.to_string_lossy(), f)
    }
}

/// A Bluetooth LE device
#[derive(Clone)]
pub struct Device {
    inner: BluetoothLEDevice,
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.inner.DeviceId() == other.inner.DeviceId()
    }
}

impl Eq for Device {}

impl std::hash::Hash for Device {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.DeviceId().unwrap().to_os_string().hash(state);
    }
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("Device");
        f.field("id", &self.id());
        if let Some(name) = self.name() {
            f.field("name", &name);
        }
        f.finish()
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name().as_deref().unwrap_or("(Unknown)"))
    }
}

impl Device {
    pub(super) async fn from_addr(addr: u64, kind: BluetoothAddressType) -> windows::core::Result<Self> {
        let inner = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind)?.await?;
        Ok(Device { inner })
    }

    pub(super) async fn from_id(id: &HSTRING) -> windows::core::Result<Self> {
        let inner = BluetoothLEDevice::FromIdAsync(id)?.await?;
        Ok(Device { inner })
    }

    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        DeviceId(
            self.inner
                .DeviceId()
                .expect("error getting DeviceId for BluetoothLEDevice")
                .to_os_string(),
        )
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    ///
    /// # Panics
    ///
    /// On Linux, this method will panic if there is a current Tokio runtime and it is single-threaded or if there is
    /// no current Tokio runtime and creating one fails.
    pub fn name(&self) -> Option<String> {
        self.inner
            .Name()
            .ok()
            .and_then(|x| (!x.is_empty()).then(|| x.to_string_lossy()))
    }

    /// The connection status for this device
    ///
    /// # Panics
    ///
    /// On Linux, this method will panic if there is a current Tokio runtime and it is single-threaded or if there is
    /// no current Tokio runtime and creating one fails.
    pub fn is_connected(&self) -> bool {
        self.inner.ConnectionStatus() == Ok(BluetoothConnectionStatus::Connected)
    }

    /// Discover the primary services of this device.
    pub async fn discover_services(&self) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Discover the primary service(s) of this device with the given [`Uuid`].
    pub async fn discover_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetGattServicesForUuidWithCacheModeAsync(GUID::from_u128(uuid.as_u128()), BluetoothCacheMode::Uncached)?
            .await?;

        check_communication_status(res.Status()?, res.ProtocolError(), "discovering services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this method may either perform service discovery or return an empty
    /// set.
    pub async fn services(&self) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Cached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Asynchronously blocks until a GATT services changed packet is received
    pub async fn services_changed(&self) -> Result<()> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let mut sender = Some(sender);
        let token = self.inner.GattServicesChanged(&TypedEventHandler::new(move |_, _| {
            if let Some(sender) = sender.take() {
                let _ = sender.send(());
            }
            Ok(())
        }))?;

        let _guard = defer(move || {
            if let Err(err) = self.inner.RemoveGattServicesChanged(token) {
                error!("Error removing state changed handler: {:?}", err);
            }
        });

        receiver.await.unwrap();
        Ok(())
    }

    /// Get the current signal strength from the device in dBm.
    ///
    /// # Platform specific
    ///
    /// Returns [ErrorKind::NotSupported] on Windows.
    pub async fn rssi(&self) -> Result<i16> {
        Err(ErrorKind::NotSupported.into())
    }
}

use smallvec::SmallVec;
use tokio::sync::Mutex;
use uuid::Uuid;
use windows::{
    core::GUID,
    Devices::Bluetooth::{
        BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothDeviceId, BluetoothLEDevice,
        GenericAttributeProfile::{GattCommunicationStatus, GattSession},
    },
};

use crate::{error::ErrorKind, Error, Result};

use super::service::Service;

/// A platform-specific device identifier.
pub type DeviceId = BluetoothDeviceId;

/// A Bluetooth LE device
pub struct Device {
    device: BluetoothLEDevice,
    session: Mutex<Option<GattSession>>,
}

impl Device {
    pub(super) async fn from_addr(addr: u64, kind: BluetoothAddressType) -> windows::core::Result<Self> {
        let device = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind)?.await?;
        Ok(Device {
            device,
            session: Mutex::new(None),
        })
    }

    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        self.device.BluetoothDeviceId().expect("error getting BluetoothDeviceId for BluetoothLEDevice")
    }

    /// The local name for this device, if available
    pub async fn name(&self) -> Option<String> {
        self.device
            .Name()
            .ok()
            .and_then(|x| (!x.is_empty()).then(|| x.to_string_lossy()))
    }

    /// The connection status for this device
    pub async fn is_connected(&self) -> bool {
        self.device.ConnectionStatus() == Ok(BluetoothConnectionStatus::Connected)
    }

    /// Discover the primary services of this device.
    ///
    /// If a [Uuid] is provided, only services with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// services will be discovered.
    pub async fn discover_services(&self, service: Option<Uuid>) -> Result<SmallVec<[Service; 2]>> {
        self.get_services(service, BluetoothCacheMode::Uncached).await
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this function may either perform service discovery or return an empty
    /// set.
    pub async fn services(&self) -> Result<SmallVec<[Service; 2]>> {
        self.get_services(None, BluetoothCacheMode::Cached).await
    }

    async fn get_services(
        &self,
        service: Option<Uuid>,
        cachemode: BluetoothCacheMode,
    ) -> Result<SmallVec<[Service; 2]>> {
        let res = if let Some(service) = service {
            self.device
                .GetGattServicesForUuidWithCacheModeAsync(GUID::from_u128(service.as_u128()), cachemode)?
                .await
        } else {
            self.device.GetGattServicesWithCacheModeAsync(cachemode)?.await
        }?;

        if res.Status()? == GattCommunicationStatus::Success {
            let services = res.Services()?;
            Ok(services.into_iter().map(Service::new).collect())
        } else {
            Err(Error {
                kind: ErrorKind::AdapterUnavailable,
                message: String::new(),
            })
        }
    }

    pub(super) async fn connect(&self) -> Result<()> {
        let mut guard = self.session.lock().await;

        if guard.is_none() {
            let session = GattSession::FromDeviceIdAsync(&self.device.BluetoothDeviceId()?)?.await?;
            session.SetMaintainConnection(true)?;
            *guard = Some(session);
        }

        Ok(())
    }

    pub(super) async fn disconnect(&self) -> Result<()> {
        let mut guard = self.session.lock().await;

        if let Some(session) = guard.take() {
            session.Close()?;
        }

        Ok(())
    }
}

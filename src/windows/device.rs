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

pub struct Device {
    device: BluetoothLEDevice,
    session: Mutex<Option<GattSession>>,
}

impl Device {
    pub(crate) async fn from_addr(addr: u64, kind: BluetoothAddressType) -> windows::core::Result<Self> {
        let device = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind)?.await?;
        Ok(Device {
            device,
            session: Mutex::new(None),
        })
    }

    pub fn id(&self) -> Result<BluetoothDeviceId> {
        self.device.BluetoothDeviceId().map_err(Into::into)
    }

    pub async fn name(&self) -> Option<String> {
        self.device.Name().ok().map(|x| x.to_string_lossy())
    }

    pub async fn is_connected(&self) -> bool {
        self.device.ConnectionStatus() == Ok(BluetoothConnectionStatus::Connected)
    }

    pub async fn discover_services(&self, service: Option<Uuid>) -> Result<SmallVec<[Service; 2]>> {
        self.get_services(service, BluetoothCacheMode::Uncached).await
    }

    pub async fn services(&self) -> Result<SmallVec<[Service; 2]>> {
        self.get_services(None, BluetoothCacheMode::Cached).await
    }

    pub(crate) async fn get_services(
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

    pub(crate) async fn connect(&self) -> Result<()> {
        let mut guard = self.session.lock().await;

        if guard.is_none() {
            let session = GattSession::FromDeviceIdAsync(&self.device.BluetoothDeviceId()?)?.await?;
            session.SetMaintainConnection(true)?;
            *guard = Some(session);
        }

        Ok(())
    }

    pub(crate) async fn disconnect(&self) -> Result<()> {
        let mut guard = self.session.lock().await;

        if let Some(session) = guard.take() {
            session.Close()?;
        }

        Ok(())
    }
}

use smallvec::SmallVec;
use tokio::sync::Mutex;
use tracing::error;
use uuid::Uuid;
use windows::{
    core::GUID,
    Devices::Bluetooth::{
        BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
        GenericAttributeProfile::GattSession,
    },
    Foundation::TypedEventHandler,
};

use crate::Result;

use super::{error::check_communication_status, service::Service};

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
pub struct Device {
    device: BluetoothLEDevice,
    session: Mutex<Option<GattSession>>,
}

impl Clone for Device {
    fn clone(&self) -> Self {
        Self {
            device: self.device.clone(),
            session: Mutex::new(None),
        }
    }
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.device.DeviceId() == other.device.DeviceId()
    }
}

impl Eq for Device {}

impl std::hash::Hash for Device {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.device.DeviceId().unwrap().to_os_string().hash(state);
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
        if let Some(name) = self.name() {
            f.write_str(&name)
        } else {
            f.write_str("(Unknown)")
        }
    }
}

impl Device {
    pub(super) async fn from_addr(addr: u64, kind: BluetoothAddressType) -> windows::core::Result<Self> {
        let device = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind)?.await?;
        Ok(Device {
            device,
            session: Mutex::new(None),
        })
    }

    pub(super) async fn from_id(id: DeviceId) -> windows::core::Result<Self> {
        let device = BluetoothLEDevice::FromIdAsync(&(&id.0).into())?.await?;
        Ok(Device {
            device,
            session: Mutex::new(None),
        })
    }

    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        DeviceId(
            self.device
                .DeviceId()
                .expect("error getting DeviceId for BluetoothLEDevice")
                .to_os_string(),
        )
    }

    /// The local name for this device, if available
    pub fn name(&self) -> Option<String> {
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

        check_communication_status(res.Status()?, res.ProtocolError()?, "discovering services")?;

        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Asynchronously blocks until a GATT services changed packet is received
    pub async fn services_changed(&self) -> Result<()> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut sender = Some(sender);
        let token = self.device.GattServicesChanged(&TypedEventHandler::new(move |_, _| {
            if let Some(sender) = sender.take() {
                let _ = sender.send(());
            }
            Ok(())
        }))?;

        let _guard = scopeguard::guard((), move |_| {
            if let Err(err) = self.device.RemoveGattServicesChanged(token) {
                error!("Error removing state changed handler: {:?}", err);
            }
        });

        receiver.await.unwrap();
        Ok(())
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

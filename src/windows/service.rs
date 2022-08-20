use smallvec::SmallVec;
use uuid::Uuid;
use windows::{
    core::GUID,
    Devices::Bluetooth::{BluetoothCacheMode, GenericAttributeProfile::GattDeviceService},
};

use crate::Result;

use super::{characteristic::Characteristic, error::check_communication_status};

/// A Bluetooth GATT service
#[derive(Clone)]
pub struct Service {
    inner: GattDeviceService,
}

impl PartialEq for Service {
    fn eq(&self, other: &Self) -> bool {
        self.inner.DeviceId() == other.inner.DeviceId()
    }
}

impl Eq for Service {}

impl std::hash::Hash for Service {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner
            .DeviceId()
            .expect("DeviceId missing on GattDeviceService")
            .to_os_string()
            .hash(state);
    }
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field(
                "device_id",
                &self.inner.DeviceId().expect("DeviceId missing on GattDeviceService"),
            )
            .field("uuid", &self.inner.Uuid().expect("UUID missing on GattDeviceService"))
            .field(
                "handle",
                &self
                    .inner
                    .AttributeHandle()
                    .expect("AttributeHandle missing on GattDeviceService"),
            )
            .finish()
    }
}

impl Service {
    pub(super) fn new(service: GattDeviceService) -> Self {
        Service { inner: service }
    }

    /// The [UUID] identifying the type of service
    pub fn uuid(&self) -> Result<Uuid> {
        Ok(Uuid::from_u128(self.inner.Uuid()?.to_u128()))
    }

    /// Discover the characteristics associated with this service.
    ///
    /// If a [Uuid] is provided, only characteristics with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// characteristics will be discovered.
    pub async fn discover_characteristics(
        &self,
        characteristic: Option<Uuid>,
    ) -> Result<SmallVec<[Characteristic; 2]>> {
        self.get_characteristics(characteristic, BluetoothCacheMode::Uncached)
            .await
    }

    /// Get previously discovered characteristics.
    ///
    /// If no characteristics have been discovered yet, this function may either perform characteristic discovery or
    /// return an empty set.
    pub async fn characteristics(&self) -> Result<SmallVec<[Characteristic; 2]>> {
        self.get_characteristics(None, BluetoothCacheMode::Cached).await
    }

    async fn get_characteristics(
        &self,
        characteristic: Option<Uuid>,
        cachemode: BluetoothCacheMode,
    ) -> Result<SmallVec<[Characteristic; 2]>> {
        let res = if let Some(characteristic) = characteristic {
            self.inner
                .GetCharacteristicsForUuidWithCacheModeAsync(GUID::from_u128(characteristic.as_u128()), cachemode)?
                .await
        } else {
            self.inner.GetCharacteristicsWithCacheModeAsync(cachemode)?.await
        }?;

        check_communication_status(res.Status()?, res.ProtocolError(), "discovering characteristics")?;

        let characteristics = res.Characteristics()?;
        Ok(characteristics.into_iter().map(Characteristic::new).collect())
    }

    /// Discover the included services of this service.
    ///
    /// If a [Uuid] is provided, only services with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// services will be discovered.
    pub async fn discover_included_services(&self, service: Option<Uuid>) -> Result<SmallVec<[Service; 2]>> {
        self.get_included_services(service, BluetoothCacheMode::Uncached).await
    }

    /// Get previously discovered included services.
    ///
    /// If no included services have been discovered yet, this function may either perform included service discovery
    /// or return an empty set.
    pub async fn included_services(&self) -> Result<SmallVec<[Service; 2]>> {
        self.get_included_services(None, BluetoothCacheMode::Cached).await
    }

    async fn get_included_services(
        &self,
        service: Option<Uuid>,
        cachemode: BluetoothCacheMode,
    ) -> Result<SmallVec<[Service; 2]>> {
        let res = if let Some(service) = service {
            self.inner
                .GetIncludedServicesForUuidWithCacheModeAsync(GUID::from_u128(service.as_u128()), cachemode)?
                .await
        } else {
            self.inner.GetIncludedServicesWithCacheModeAsync(cachemode)?.await
        }?;

        check_communication_status(res.Status()?, res.ProtocolError(), "discovering included services")?;

        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }
}

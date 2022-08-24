use windows::core::GUID;
use windows::Devices::Bluetooth::BluetoothCacheMode;
use windows::Devices::Bluetooth::GenericAttributeProfile::GattDeviceService;

use super::characteristic::Characteristic;
use super::error::check_communication_status;
use crate::{Result, SmallVec, Uuid};

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
    pub async fn discover_characteristics(&self) -> Result<SmallVec<[Characteristic; 2]>> {
        let res = self
            .inner
            .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering characteristics")?;
        let characteristics = res.Characteristics()?;
        Ok(characteristics.into_iter().map(Characteristic::new).collect())
    }

    /// Discover the characteristics(s) of this service with the given [Uuid].
    pub async fn discover_characteristics_with_uuid(&self, uuid: Uuid) -> Result<SmallVec<[Characteristic; 2]>> {
        let res = self
            .inner
            .GetCharacteristicsForUuidWithCacheModeAsync(GUID::from_u128(uuid.as_u128()), BluetoothCacheMode::Uncached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering characteristics")?;
        let characteristics = res.Characteristics()?;
        Ok(characteristics.into_iter().map(Characteristic::new).collect())
    }

    /// Get previously discovered characteristics.
    ///
    /// If no characteristics have been discovered yet, this function may either perform characteristic discovery or
    /// return an empty set.
    pub async fn characteristics(&self) -> Result<SmallVec<[Characteristic; 2]>> {
        let res = self
            .inner
            .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Cached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering characteristics")?;
        let characteristics = res.Characteristics()?;
        Ok(characteristics.into_iter().map(Characteristic::new).collect())
    }

    /// Discover the included services of this service.
    pub async fn discover_included_services(&self) -> Result<SmallVec<[Service; 2]>> {
        let res = self
            .inner
            .GetIncludedServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering included services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Discover the included service(s) of this service with the given [Uuid].
    pub async fn discover_included_services_with_uuid(&self, uuid: Uuid) -> Result<SmallVec<[Service; 2]>> {
        let res = self
            .inner
            .GetIncludedServicesForUuidWithCacheModeAsync(
                GUID::from_u128(uuid.as_u128()),
                BluetoothCacheMode::Uncached,
            )?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering included services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Get previously discovered included services.
    ///
    /// If no included services have been discovered yet, this function may either perform included service discovery
    /// or return an empty set.
    pub async fn included_services(&self) -> Result<SmallVec<[Service; 2]>> {
        let res = self
            .inner
            .GetIncludedServicesWithCacheModeAsync(BluetoothCacheMode::Cached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering included services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }
}

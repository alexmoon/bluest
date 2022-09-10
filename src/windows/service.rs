use windows::core::GUID;
use windows::Devices::Bluetooth::BluetoothCacheMode;
use windows::Devices::Bluetooth::GenericAttributeProfile::GattDeviceService;

use super::characteristic::Characteristic;
use super::error::check_communication_status;
use crate::error::ErrorKind;
use crate::{Result, Uuid};

/// A Bluetooth GATT service
#[derive(Clone)]
pub struct Service {
    inner: GattDeviceService,
}

impl PartialEq for Service {
    fn eq(&self, other: &Self) -> bool {
        self.inner.Session().unwrap() == other.inner.Session().unwrap()
            && self.inner.AttributeHandle().unwrap() == other.inner.AttributeHandle().unwrap()
    }
}

impl Eq for Service {}

impl std::hash::Hash for Service {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner
            .Session()
            .unwrap()
            .DeviceId()
            .unwrap()
            .Id()
            .unwrap()
            .to_os_string()
            .hash(state);
        self.inner.AttributeHandle().unwrap().hash(state);
    }
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field(
                "device_id",
                &self
                    .inner
                    .Session()
                    .expect("Session missing on GattDeviceService")
                    .DeviceId()
                    .expect("DeviceId missing on GattSession")
                    .Id()
                    .expect("Id missing on BluetoothDeviceId")
                    .to_os_string(),
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

    /// The [`Uuid`] identifying the type of service
    ///
    /// # Panics
    ///
    /// On Linux, this method will panic if there is a current Tokio runtime and it is single-threaded, if there is no
    /// current Tokio runtime and creating one fails, or if the underlying [`Service::uuid_async()`] method
    /// fails.
    pub fn uuid(&self) -> Uuid {
        Uuid::from_u128(self.inner.Uuid().expect("UUID missing on GattDeviceService").to_u128())
    }

    /// The [`Uuid`] identifying the type of this GATT service
    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(Uuid::from_u128(self.inner.Uuid()?.to_u128()))
    }

    /// Whether this is a primary service of the device.
    ///
    /// # Platform specific
    ///
    /// Returns [ErrorKind::NotSupported] on Windows.
    pub async fn is_primary(&self) -> Result<bool> {
        Err(ErrorKind::NotSupported.into())
    }

    /// Discover all characteristics associated with this service.
    pub async fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        let res = self
            .inner
            .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering characteristics")?;
        let characteristics = res.Characteristics()?;
        Ok(characteristics.into_iter().map(Characteristic::new).collect())
    }

    /// Discover the characteristic(s) with the given [`Uuid`].
    pub async fn discover_characteristics_with_uuid(&self, uuid: Uuid) -> Result<Vec<Characteristic>> {
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
    /// If no characteristics have been discovered yet, this method may either perform characteristic discovery or
    /// return an empty set.
    pub async fn characteristics(&self) -> Result<Vec<Characteristic>> {
        let res = self
            .inner
            .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Cached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering characteristics")?;
        let characteristics = res.Characteristics()?;
        Ok(characteristics.into_iter().map(Characteristic::new).collect())
    }

    /// Discover the included services of this service.
    pub async fn discover_included_services(&self) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetIncludedServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering included services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Discover the included service(s) with the given [`Uuid`].
    pub async fn discover_included_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
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
    /// If no included services have been discovered yet, this method may either perform included service discovery
    /// or return an empty set.
    pub async fn included_services(&self) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetIncludedServicesWithCacheModeAsync(BluetoothCacheMode::Cached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering included services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }
}

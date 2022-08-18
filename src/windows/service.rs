use smallvec::SmallVec;
use uuid::Uuid;
use windows::{
    core::GUID,
    Devices::Bluetooth::{
        BluetoothCacheMode,
        GenericAttributeProfile::{GattCommunicationStatus, GattDeviceService},
    },
};

use crate::{error::ErrorKind, Error, Result};

use super::characteristic::Characteristic;

/// A Bluetooth GATT service
pub struct Service {
    service: GattDeviceService,
}

impl Service {
    pub(super) fn new(service: GattDeviceService) -> Self {
        Service { service }
    }

    /// The [UUID] identifying the type of service
    pub fn uuid(&self) -> Result<Uuid> {
        Ok(Uuid::from_u128(self.service.Uuid()?.to_u128()))
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
            self.service
                .GetCharacteristicsForUuidWithCacheModeAsync(GUID::from_u128(characteristic.as_u128()), cachemode)?
                .await
        } else {
            self.service.GetCharacteristicsWithCacheModeAsync(cachemode)?.await
        }?;

        if let Ok(GattCommunicationStatus::Success) = res.Status() {
            let characteristics = res.Characteristics()?;
            Ok(characteristics.into_iter().map(Characteristic::new).collect())
        } else {
            Err(Error {
                kind: ErrorKind::AdapterUnavailable,
                message: String::new(),
            })
        }
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
            self.service
                .GetIncludedServicesForUuidWithCacheModeAsync(GUID::from_u128(service.as_u128()), cachemode)?
                .await
        } else {
            self.service.GetIncludedServicesWithCacheModeAsync(cachemode)?.await
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
}

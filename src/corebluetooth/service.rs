use btuuid::BluetoothUuid;
use corebluetooth::CBPeripheralState;
use dispatch_executor::Handle;

use super::delegates::{subscribe_peripheral, PeripheralEvent};
use crate::error::ErrorKind;
use crate::{Characteristic, Error, Result, Service, Uuid};

/// A Bluetooth GATT service
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceImpl {
    pub(super) inner: Handle<corebluetooth::Service>,
}

impl Service {
    pub(super) fn new(inner: Handle<corebluetooth::Service>) -> Self {
        Service(ServiceImpl { inner })
    }
}

impl ServiceImpl {
    /// The [`Uuid`] identifying the type of this GATT service
    pub fn uuid(&self) -> Uuid {
        self.inner.lock(|service, _| service.uuid().into())
    }

    /// The [`Uuid`] identifying the type of this GATT service
    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(self.uuid())
    }

    /// Whether this is a primary service of the device.
    pub async fn is_primary(&self) -> Result<bool> {
        self.inner.lock(|service, _| Ok(service.is_primary()))
    }

    /// Discover all characteristics associated with this service.
    pub async fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        self.discover_characteristics_inner(None).await
    }

    /// Discover the characteristic(s) with the given [`Uuid`].
    pub async fn discover_characteristics_with_uuid(&self, uuid: Uuid) -> Result<Vec<Characteristic>> {
        let characteristics = self.discover_characteristics_inner(Some(uuid)).await?;
        Ok(characteristics.into_iter().filter(|x| x.uuid() == uuid).collect())
    }

    async fn discover_characteristics_inner(&self, uuid: Option<Uuid>) -> Result<Vec<Characteristic>> {
        let mut receiver = self.inner.lock(|service, _| {
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.discover_characteristics(
                service,
                uuid.map(BluetoothUuid::from).as_ref().map(std::slice::from_ref),
            );

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok(receiver)
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DiscoveredCharacteristics { service, result } if service == self.inner => {
                    result?;
                    break;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(error.into());
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&self.inner) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }

        self.characteristics_inner()
    }

    /// Get previously discovered characteristics.
    ///
    /// If no characteristics have been discovered yet, this method will perform characteristic discovery.
    pub async fn characteristics(&self) -> Result<Vec<Characteristic>> {
        match self.characteristics_inner() {
            Ok(characteristics) => Ok(characteristics),
            Err(_) => self.discover_characteristics().await,
        }
    }

    fn characteristics_inner(&self) -> Result<Vec<Characteristic>> {
        self.inner.lock(|service, executor| {
            service
                .characteristics()
                .map(|s| {
                    s.iter()
                        .map(|x| Characteristic::new(executor.handle(x.clone())))
                        .collect()
                })
                .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "no characteristics have been discovered"))
        })
    }

    /// Discover the included services of this service.
    pub async fn discover_included_services(&self) -> Result<Vec<Service>> {
        self.discover_included_services_inner(None).await
    }

    /// Discover the included service(s) with the given [`Uuid`].
    pub async fn discover_included_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        let services = self.discover_included_services_inner(Some(uuid)).await?;
        Ok(services.into_iter().filter(|x| x.uuid() == uuid).collect())
    }

    async fn discover_included_services_inner(&self, uuid: Option<Uuid>) -> Result<Vec<Service>> {
        let mut receiver = self.inner.lock(|service, _| {
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.discover_included_services(
                service,
                uuid.map(BluetoothUuid::from).as_ref().map(std::slice::from_ref),
            );

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok(receiver)
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DiscoveredIncludedServices { service, result } if service == self.inner => {
                    result?;
                    break;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(error.into());
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&self.inner) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }

        self.included_services_inner()
    }

    /// Get previously discovered included services.
    ///
    /// If no included services have been discovered yet, this method will perform included service discovery.
    pub async fn included_services(&self) -> Result<Vec<Service>> {
        match self.included_services_inner() {
            Ok(services) => Ok(services),
            Err(_) => self.discover_included_services().await,
        }
    }

    fn included_services_inner(&self) -> Result<Vec<Service>> {
        self.inner.lock(|service, executor| {
            service
                .included_services()
                .map(|s| s.iter().map(|x| Service::new(executor.handle(x.clone()))).collect())
                .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "no included services have been discovered"))
        })
    }
}

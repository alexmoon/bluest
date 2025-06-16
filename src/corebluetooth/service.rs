use objc2::Message;
use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSData};

use super::delegates::{PeripheralDelegate, PeripheralEvent};
use crate::error::ErrorKind;
use crate::{Characteristic, Error, Result, Service, Uuid};
use objc2_core_bluetooth::{CBPeripheralState, CBService, CBUUID};

/// A Bluetooth GATT service
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceImpl {
    pub(super) inner: Retained<CBService>,
    delegate: Retained<PeripheralDelegate>,
}

impl Service {
    pub(super) fn new(service: &CBService, delegate: Retained<PeripheralDelegate>) -> Self {
        Service(ServiceImpl {
            inner: service.retain(),
            delegate,
        })
    }
}

impl ServiceImpl {
    /// The [`Uuid`] identifying the type of this GATT service
    pub fn uuid(&self) -> Uuid {
        unsafe { Uuid::from_slice(self.inner.UUID().data().as_bytes_unchecked()).unwrap() }
    }

    /// The [`Uuid`] identifying the type of this GATT service
    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(self.uuid())
    }

    /// Whether this is a primary service of the device.
    pub async fn is_primary(&self) -> Result<bool> {
        unsafe { Ok(self.inner.isPrimary()) }
    }

    /// Discover all characteristics associated with this service.
    pub async fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        self.discover_characteristics_inner(None).await
    }

    /// Discover the characteristic(s) with the given [`Uuid`].
    pub async fn discover_characteristics_with_uuid(
        &self,
        uuid: Uuid,
    ) -> Result<Vec<Characteristic>> {
        let uuids = unsafe {
            NSArray::from_retained_slice(&[CBUUID::UUIDWithData(&NSData::with_bytes(
                &uuid.as_bytes()[..],
            ))])
        };

        let characteristics = self.discover_characteristics_inner(Some(&uuids)).await?;
        Ok(characteristics
            .into_iter()
            .filter(|x| x.uuid() == uuid)
            .collect())
    }

    async fn discover_characteristics_inner(
        &self,
        uuids: Option<&NSArray<CBUUID>>,
    ) -> Result<Vec<Characteristic>> {
        let peripheral = unsafe {
            self.inner.peripheral().ok_or(Error::new(
                ErrorKind::NotFound,
                None,
                "peripheral not found",
            ))?
        };

        if unsafe { peripheral.state() } != CBPeripheralState::Connected {
            return Err(ErrorKind::NotConnected.into());
        }

        let mut receiver = self.delegate.sender().new_receiver();
        unsafe { peripheral.discoverCharacteristics_forService(uuids, &self.inner) };

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredCharacteristics { service, error }
                    if service == self.inner =>
                {
                    match error {
                        Some(err) => return Err(Error::from_nserror(err)),
                        None => break,
                    }
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                }
                PeripheralEvent::ServicesChanged {
                    invalidated_services,
                } if invalidated_services.contains(&self.inner) => {
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
        unsafe { self.inner.characteristics() }
            .map(|s| {
                s.iter()
                    .map(|x| Characteristic::new(&x, self.delegate.clone()))
                    .collect()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "no characteristics have been discovered",
                )
            })
    }

    /// Discover the included services of this service.
    pub async fn discover_included_services(&self) -> Result<Vec<Service>> {
        self.discover_included_services_inner(None).await
    }

    /// Discover the included service(s) with the given [`Uuid`].
    pub async fn discover_included_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        let uuids = unsafe {
            NSArray::from_retained_slice(&[CBUUID::UUIDWithData(&NSData::with_bytes(
                &uuid.as_bytes()[..],
            ))])
        };

        let services = self.discover_included_services_inner(Some(&uuids)).await?;
        Ok(services.into_iter().filter(|x| x.uuid() == uuid).collect())
    }

    async fn discover_included_services_inner(
        &self,
        uuids: Option<&NSArray<CBUUID>>,
    ) -> Result<Vec<Service>> {
        let peripheral = unsafe {
            self.inner.peripheral().ok_or(Error::new(
                ErrorKind::NotFound,
                None,
                "peripheral not found",
            ))?
        };

        if unsafe { peripheral.state() } != CBPeripheralState::Connected {
            return Err(ErrorKind::NotConnected.into());
        }

        let mut receiver = self.delegate.sender().new_receiver();
        unsafe { peripheral.discoverIncludedServices_forService(uuids, &self.inner) };

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredIncludedServices { service, error }
                    if service == self.inner =>
                {
                    match error {
                        Some(err) => return Err(Error::from_nserror(err)),
                        None => break,
                    }
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                }
                PeripheralEvent::ServicesChanged {
                    invalidated_services,
                } if invalidated_services.contains(&self.inner) => {
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
        unsafe { self.inner.includedServices() }
            .map(|s| {
                s.iter()
                    .map(|x| Service::new(&x, self.delegate.clone()))
                    .collect()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "no included services have been discovered",
                )
            })
    }
}

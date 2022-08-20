use objc_foundation::{INSArray, INSFastEnumeration, NSArray};
use objc_id::ShareId;
use smallvec::SmallVec;
use uuid::Uuid;

use super::delegates::PeripheralEvent;
use super::types::CBUUID;
use super::{characteristic::Characteristic, types::CBService};

use crate::error::ErrorKind;
use crate::{Error, Result};

/// A Bluetooth GATT service
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Service {
    inner: ShareId<CBService>,
}

impl Service {
    pub(super) fn new(service: &CBService) -> Self {
        Service {
            inner: unsafe { ShareId::from_ptr(service as *const _ as *mut _) },
        }
    }

    /// The [Uuid] identifying the type of this GATT service
    pub fn uuid(&self) -> Uuid {
        self.inner.uuid().to_uuid()
    }

    /// Whether this is a primary service of the device.
    ///
    /// # Platform specific
    ///
    /// This function is available on MacOS/iOS only.
    pub fn is_primary(&self) -> bool {
        self.inner.is_primary()
    }

    /// Discover the characteristics associated with this service.
    ///
    /// If a [Uuid] is provided, only characteristics with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// characteristics will be discovered.
    pub async fn discover_characteristics(&self, uuid: Option<Uuid>) -> Result<SmallVec<[Characteristic; 2]>> {
        let uuids = uuid.map(|x| {
            let vec = vec![CBUUID::from_uuid(x)];
            NSArray::from_vec(vec)
        });

        let peripheral = self.inner.peripheral();
        let mut receiver = peripheral.subscribe()?;
        peripheral.discover_characteristics(&self.inner, uuids);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredCharacteristics { service, error } if service == self.inner => match error {
                    Some(err) => Err(Error::from_nserror(err))?,
                    None => break,
                },
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&self.inner) =>
                {
                    Err(ErrorKind::ServiceChanged)?
                }
                _ => (),
            }
        }

        self.characteristics().await
    }

    /// Get previously discovered characteristics.
    ///
    /// If no characteristics have been discovered yet, this function may either perform characteristic discovery or
    /// return an error.
    pub async fn characteristics(&self) -> Result<SmallVec<[Characteristic; 2]>> {
        self.inner
            .characteristics()
            .map(|s| s.enumerator().map(Characteristic::new).collect())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "no characteristics have been discovered".to_string(),
                )
            })
    }

    /// Discover the included services of this service.
    ///
    /// If a [Uuid] is provided, only services with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// services will be discovered.
    pub async fn discover_included_services(&self, uuid: Option<Uuid>) -> Result<SmallVec<[Service; 2]>> {
        let uuids = uuid.map(|x| {
            let vec = vec![CBUUID::from_uuid(x)];
            NSArray::from_vec(vec)
        });

        let peripheral = self.inner.peripheral();
        let mut receiver = peripheral.subscribe()?;
        peripheral.discover_included_services(&self.inner, uuids);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredIncludedServices { service, error } if service == self.inner => {
                    match error {
                        Some(err) => Err(Error::from_nserror(err))?,
                        None => break,
                    }
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&self.inner) =>
                {
                    Err(ErrorKind::ServiceChanged)?
                }
                _ => (),
            }
        }

        self.included_services().await
    }

    /// Get previously discovered included services.
    ///
    /// If no included services have been discovered yet, this function may either perform included service discovery
    /// or return an error.
    pub async fn included_services(&self) -> Result<SmallVec<[Service; 2]>> {
        self.inner
            .included_services()
            .map(|s| s.enumerator().map(Service::new).collect())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "no included services have been discovered".to_string(),
                )
            })
    }
}

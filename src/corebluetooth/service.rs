use objc_foundation::{INSArray, INSFastEnumeration, NSArray};
use objc_id::ShareId;
use smallvec::SmallVec;
use uuid::Uuid;

use super::delegates::PeripheralEvent;
use super::types::CBUUID;
use super::{characteristic::Characteristic, types::CBService};

use crate::error::ErrorKind;
use crate::Result;

/// A Bluetooth GATT service
#[derive(Debug)]
pub struct Service {
    service: ShareId<CBService>,
}

impl Service {
    pub(super) fn new(service: &CBService) -> Self {
        Service {
            service: unsafe { ShareId::from_ptr(service as *const _ as *mut _) },
        }
    }

    /// The [Uuid] identifying the type of this GATT service
    pub fn uuid(&self) -> Uuid {
        self.service.uuid().to_uuid()
    }

    /// Whether this is a primary service of the device.
    ///
    /// # Platform specific
    ///
    /// This function is available on MacOS/iOS only.
    pub fn is_primary(&self) -> bool {
        self.service.is_primary()
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

        let peripheral = self.service.peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;
        peripheral.discover_characteristics(&self.service, uuids);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::DiscoveredCharacteristics { service, error }) if service == self.service => {
                    match error {
                        Some(err) => Err(&*err)?,
                        None => break,
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }

        Ok(self.characteristics().await)
    }

    /// Get previously discovered characteristics.
    ///
    /// If no characteristics have been discovered yet, this function may either perform characteristic discovery or
    /// return an empty set.
    pub async fn characteristics(&self) -> SmallVec<[Characteristic; 2]> {
        match self.service.characteristics() {
            Some(c) => c.enumerator().map(Characteristic::new).collect(),
            None => SmallVec::new(),
        }
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

        let peripheral = self.service.peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;
        peripheral.discover_included_services(&self.service, uuids);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::DiscoveredIncludedServices { service, error }) if service == self.service => {
                    match error {
                        Some(err) => Err(&*err)?,
                        None => break,
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }

        Ok(self.included_services().await)
    }

    /// Get previously discovered included services.
    ///
    /// If no included services have been discovered yet, this function may either perform included service discovery
    /// or return an empty set.
    pub async fn included_services(&self) -> SmallVec<[Service; 2]> {
        match self.service.included_services() {
            Some(s) => s.enumerator().map(Service::new).collect(),
            None => SmallVec::new(),
        }
    }
}

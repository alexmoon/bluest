use objc_foundation::{INSArray, INSFastEnumeration, NSArray};
use objc_id::ShareId;
use smallvec::SmallVec;
use uuid::Uuid;

use super::delegates::PeripheralEvent;
use super::types::CBUUID;
use super::{characteristic::Characteristic, types::CBService};

use crate::error::ErrorKind;
use crate::Result;

#[derive(Debug)]
pub struct Service {
    service: ShareId<CBService>,
}

impl Service {
    pub(crate) fn new(service: &CBService) -> Self {
        Service {
            service: unsafe { ShareId::from_ptr(service as *const _ as *mut _) },
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.service.uuid().to_uuid()
    }

    pub fn is_primary(&self) -> bool {
        self.service.is_primary()
    }

    pub async fn discover_characteristics(
        &self,
        characteristics: Option<&[Uuid]>,
    ) -> Result<SmallVec<[Characteristic; 2]>> {
        let characteristics = characteristics.map(|x| {
            let vec = x.iter().map(CBUUID::from_uuid).collect::<Vec<_>>();
            NSArray::from_vec(vec)
        });

        let peripheral = self.service.peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;
        peripheral.discover_characteristics(&self.service, characteristics);

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

    pub async fn characteristics(&self) -> SmallVec<[Characteristic; 2]> {
        match self.service.characteristics() {
            Some(c) => c.enumerator().map(Characteristic::new).collect(),
            None => SmallVec::new(),
        }
    }

    pub async fn discover_included_services(&self, services: Option<&[Uuid]>) -> Result<SmallVec<[Service; 2]>> {
        let services = services.map(|x| {
            let vec = x.iter().map(CBUUID::from_uuid).collect::<Vec<_>>();
            NSArray::from_vec(vec)
        });

        let peripheral = self.service.peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;
        peripheral.discover_included_services(&self.service, services);

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

    pub async fn included_services(&self) -> SmallVec<[Service; 2]> {
        match self.service.included_services() {
            Some(s) => s.enumerator().map(Service::new).collect(),
            None => SmallVec::new(),
        }
    }
}

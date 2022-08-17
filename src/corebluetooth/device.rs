#![allow(clippy::let_unit_value)]

use objc_foundation::{INSArray, INSFastEnumeration, INSString, NSArray};
use objc_id::ShareId;
use smallvec::SmallVec;
use uuid::Uuid;

use super::delegates::{self, PeripheralDelegate, PeripheralEvent};
use super::service::Service;
use super::types::{CBPeripheral, CBPeripheralState, CBUUID};

use crate::error::ErrorKind;
use crate::Result;

pub struct Device {
    pub(super) peripheral: ShareId<CBPeripheral>,
    sender: tokio::sync::broadcast::Sender<delegates::PeripheralEvent>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device").field("peripheral", &self.peripheral).finish()
    }
}

impl Device {
    pub(crate) fn new(peripheral: ShareId<CBPeripheral>) -> Self {
        let sender = peripheral
            .delegate()
            .and_then(|x| x.sender().cloned())
            .unwrap_or_else(|| {
                // Create a new delegate and attach it to the peripheral
                let (sender, _) = tokio::sync::broadcast::channel(16);
                let delegate = PeripheralDelegate::with_sender(sender.clone());
                peripheral.set_delegate(delegate);
                sender
            });

        Device { peripheral, sender }
    }

    pub fn id(&self) -> Uuid {
        self.peripheral.identifier().to_uuid()
    }

    pub async fn name(&self) -> Option<String> {
        self.peripheral.name().map(|x| x.as_str().to_owned())
    }

    pub async fn is_connected(&self) -> bool {
        self.peripheral.state() == CBPeripheralState::Connected
    }

    pub async fn rssi(&self) -> Result<i16> {
        let mut receiver = self.sender.subscribe();
        self.peripheral.read_rssi();

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::ReadRssi { rssi, error: None }) => return Ok(rssi),
                Ok(PeripheralEvent::ReadRssi { error: Some(err), .. }) => Err(&*err)?,
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }
    }

    pub async fn discover_services(&self, services: Option<&[Uuid]>) -> Result<SmallVec<[Service; 2]>> {
        let services = services.map(|x| {
            let vec = x.iter().map(CBUUID::from_uuid).collect::<Vec<_>>();
            NSArray::from_vec(vec)
        });

        let mut receiver = self.sender.subscribe();
        self.peripheral.discover_services(services);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::DiscoveredServices { error: None }) => break,
                Ok(PeripheralEvent::DiscoveredServices { error: Some(err) }) => Err(&*err)?,
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }

        Ok(self.services().await)
    }

    pub async fn services(&self) -> SmallVec<[Service; 2]> {
        match self.peripheral.services() {
            Some(s) => s.enumerator().map(Service::new).collect(),
            None => SmallVec::new(),
        }
    }
}

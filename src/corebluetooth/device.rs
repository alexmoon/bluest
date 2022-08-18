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

/// A platform-specific device identifier.
pub type DeviceId = Uuid;

/// A Bluetooth LE device
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
    pub(super) fn new(peripheral: ShareId<CBPeripheral>) -> Self {
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

    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        self.peripheral.identifier().to_uuid()
    }

    /// The local name for this device, if available
    pub async fn name(&self) -> Option<String> {
        self.peripheral.name().map(|x| x.as_str().to_owned())
    }

    /// The connection status for this device
    pub async fn is_connected(&self) -> bool {
        self.peripheral.state() == CBPeripheralState::Connected
    }

    /// Discover the primary services of this device.
    ///
    /// If a [Uuid] is provided, only services with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// services will be discovered.
    pub async fn discover_services(&self, uuid: Option<Uuid>) -> Result<SmallVec<[Service; 2]>> {
        let uuids = uuid.map(|x| {
            let vec = vec![CBUUID::from_uuid(x)];
            NSArray::from_vec(vec)
        });

        let mut receiver = self.sender.subscribe();
        self.peripheral.discover_services(uuids);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::DiscoveredServices { error: None }) => break,
                Ok(PeripheralEvent::DiscoveredServices { error: Some(err) }) => Err(&*err)?,
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }

        self.services().await
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this function may either perform service discovery or return an empty
    /// set.
    pub async fn services(&self) -> Result<SmallVec<[Service; 2]>> {
        Ok(match self.peripheral.services() {
            Some(s) => s.enumerator().map(Service::new).collect(),
            None => SmallVec::new(),
        })
    }
}

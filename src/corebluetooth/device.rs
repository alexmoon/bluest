#![allow(clippy::let_unit_value)]

use objc_foundation::{INSArray, INSFastEnumeration, INSString, NSArray};
use objc_id::{Id, ShareId};

use super::delegates::{self, PeripheralDelegate, PeripheralEvent};
use super::service::Service;
use super::types::{CBPeripheral, CBPeripheralState, CBUUID};
use crate::error::ErrorKind;
use crate::{Error, Result, SmallVec, Uuid};

/// A platform-specific device identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(pub(super) Uuid);

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// A Bluetooth LE device
#[derive(Clone)]
pub struct Device {
    pub(super) peripheral: ShareId<CBPeripheral>,
    sender: tokio::sync::broadcast::Sender<delegates::PeripheralEvent>,
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.peripheral == other.peripheral
    }
}

impl Eq for Device {}

impl std::hash::Hash for Device {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.peripheral.hash(state);
    }
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Device").field(&self.peripheral).finish()
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.name() {
            f.write_str(&name)
        } else {
            f.write_str("(Unknown)")
        }
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
        DeviceId(self.peripheral.identifier().to_uuid())
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
    pub async fn discover_services(&self) -> Result<SmallVec<[Service; 2]>> {
        self.discover_services_inner(None).await
    }

    /// Discover the primary service(s) of this device with the given [Uuid].
    pub async fn discover_services_with_uuid(&self, uuid: Uuid) -> Result<SmallVec<[Service; 2]>> {
        let uuids = {
            let vec = vec![CBUUID::from_uuid(uuid)];
            NSArray::from_vec(vec)
        };

        self.discover_services_inner(Some(uuids)).await
    }

    async fn discover_services_inner(&self, uuids: Option<Id<NSArray<CBUUID>>>) -> Result<SmallVec<[Service; 2]>> {
        let mut receiver = self.sender.subscribe();
        self.peripheral.discover_services(uuids);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredServices { error: None } => break,
                PeripheralEvent::DiscoveredServices { error: Some(err) } => Err(Error::from_nserror(err))?,
                _ => (),
            }
        }

        self.services().await
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this function may either perform service discovery or return an error.
    pub async fn services(&self) -> Result<SmallVec<[Service; 2]>> {
        self.peripheral
            .services()
            .map(|s| s.enumerator().map(Service::new).collect())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "no services have been discovered".to_string(),
                )
            })
    }

    /// Asynchronously blocks until a GATT services changed packet is received
    pub async fn services_changed(&self) -> Result<()> {
        let mut receiver = self.sender.subscribe();
        while !matches!(
            receiver.recv().await.map_err(Error::from_recv_error)?,
            PeripheralEvent::ServicesChanged { .. }
        ) {}

        Ok(())
    }

    /// Get the current signal strength from the device in dBm.
    ///
    /// # Platform specific
    ///
    /// This function is available on MacOS/iOS only.
    pub async fn rssi(&self) -> Result<i16> {
        let mut receiver = self.sender.subscribe();
        self.peripheral.read_rssi();

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::ReadRssi { rssi, error: None }) => return Ok(rssi),
                Ok(PeripheralEvent::ReadRssi { error: Some(err), .. }) => Err(Error::from_nserror(err))?,
                Err(err) => Err(Error::from_recv_error(err))?,
                _ => (),
            }
        }
    }
}

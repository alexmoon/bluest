#![allow(clippy::let_unit_value)]

use objc_foundation::{INSArray, INSFastEnumeration, INSString, NSArray};
use objc_id::ShareId;

use super::delegates::{PeripheralDelegate, PeripheralEvent};
use super::types::{CBPeripheral, CBPeripheralState, CBUUID};
use crate::error::ErrorKind;
use crate::pairing::PairingAgent;
use crate::{Device, DeviceId, Error, Result, Service, Uuid};

/// A Bluetooth LE device
#[derive(Clone)]
pub struct DeviceImpl {
    pub(super) peripheral: ShareId<CBPeripheral>,
    delegate: ShareId<PeripheralDelegate>,
}

impl PartialEq for DeviceImpl {
    fn eq(&self, other: &Self) -> bool {
        self.peripheral == other.peripheral
    }
}

impl Eq for DeviceImpl {}

impl std::hash::Hash for DeviceImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.peripheral.hash(state);
    }
}

impl std::fmt::Debug for DeviceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Device").field(&self.peripheral).finish()
    }
}

impl std::fmt::Display for DeviceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name().as_deref().unwrap_or("(Unknown)"))
    }
}

impl Device {
    pub(super) fn new(peripheral: ShareId<CBPeripheral>) -> Self {
        let delegate = peripheral.delegate().unwrap_or_else(|| {
            // Create a new delegate and attach it to the peripheral
            let (sender, _) = tokio::sync::broadcast::channel(16);
            let delegate = PeripheralDelegate::with_sender(sender).share();
            peripheral.set_delegate(&delegate);
            delegate
        });

        Device(DeviceImpl { peripheral, delegate })
    }
}

impl DeviceImpl {
    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        super::DeviceId(self.peripheral.identifier().to_uuid())
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub fn name(&self) -> Result<String> {
        match self.peripheral.name() {
            Some(name) => Ok(name.as_str().to_owned()),
            None => Err(ErrorKind::NotFound.into()),
        }
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub async fn name_async(&self) -> Result<String> {
        self.name()
    }

    /// The connection status for this device
    pub async fn is_connected(&self) -> bool {
        self.peripheral.state() == CBPeripheralState::CONNECTED
    }

    /// The pairing status for this device
    pub async fn is_paired(&self) -> Result<bool> {
        Err(ErrorKind::NotSupported.into())
    }

    /// Attempt to pair this device using the system default pairing UI
    ///
    /// Device pairing is performed automatically by the OS when a characteristic requiring security is accessed. This
    /// method is a no-op.
    pub async fn pair(&self) -> Result<()> {
        Ok(())
    }

    /// Attempt to pair this device using the system default pairing UI
    ///
    /// Device pairing is performed automatically by the OS when a characteristic requiring security is accessed. This
    /// method is a no-op.
    pub async fn pair_with_agent<T: PairingAgent>(&self, _agent: &T) -> Result<()> {
        Ok(())
    }

    /// Disconnect and unpair this device from the system
    ///
    /// # Platform specific
    ///
    /// Not supported on MacOS/iOS.
    pub async fn unpair(&self) -> Result<()> {
        Err(ErrorKind::NotSupported.into())
    }

    /// Discover the primary services of this device.
    pub async fn discover_services(&self) -> Result<Vec<Service>> {
        self.discover_services_inner(None).await
    }

    /// Discover the primary service(s) of this device with the given [`Uuid`].
    pub async fn discover_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        let uuids = {
            let vec = vec![CBUUID::from_uuid(uuid)];
            NSArray::from_vec(vec)
        };

        let services = self.discover_services_inner(Some(&uuids)).await?;
        Ok(services.into_iter().filter(|x| x.uuid() == uuid).collect())
    }

    async fn discover_services_inner(&self, uuids: Option<&NSArray<CBUUID>>) -> Result<Vec<Service>> {
        let mut receiver = self.delegate.sender().subscribe();

        if !self.is_connected().await {
            return Err(ErrorKind::NotConnected.into());
        }

        self.peripheral.discover_services(uuids);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredServices { error: None } => break,
                PeripheralEvent::DiscoveredServices { error: Some(err) } => return Err(Error::from_nserror(err)),
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                }
                _ => (),
            }
        }

        self.services().await
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this method may either perform service discovery or return an error.
    pub async fn services(&self) -> Result<Vec<Service>> {
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
        let mut receiver = self.delegate.sender().subscribe();

        if !self.is_connected().await {
            return Err(ErrorKind::NotConnected.into());
        }

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::ServicesChanged { .. } => break,
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error))
                }
                _ => (),
            }
        }

        Ok(())
    }

    /// Get the current signal strength from the device in dBm.
    pub async fn rssi(&self) -> Result<i16> {
        let mut receiver = self.delegate.sender().subscribe();
        self.peripheral.read_rssi();

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::ReadRssi { rssi, error: None }) => return Ok(rssi),
                Ok(PeripheralEvent::ReadRssi { error: Some(err), .. }) => return Err(Error::from_nserror(err)),
                Err(err) => return Err(Error::from_recv_error(err)),
                _ => (),
            }
        }
    }
}

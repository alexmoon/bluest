#![allow(clippy::let_unit_value)]

use btuuid::BluetoothUuid;
use corebluetooth::CBPeripheralState;
use dispatch_executor::Handle;
use futures_core::Stream;
use futures_lite::StreamExt;

use super::delegates::{subscribe_peripheral, PeripheralEvent};
#[cfg(feature = "l2cap")]
use super::l2cap_channel::{L2capChannelReader, L2capChannelWriter};
use crate::device::ServicesChanged;
use crate::error::ErrorKind;
use crate::pairing::PairingAgent;
use crate::{Device, DeviceId, Error, Result, Service, Uuid};

/// A Bluetooth LE device
#[derive(Clone)]
pub struct DeviceImpl {
    pub(super) peripheral: Handle<corebluetooth::Peripheral>,
}

impl PartialEq for DeviceImpl {
    fn eq(&self, other: &Self) -> bool {
        self.peripheral == other.peripheral
    }
}

impl Eq for DeviceImpl {}

impl std::hash::Hash for DeviceImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.peripheral.hash(state)
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
    pub(super) fn new(peripheral: Handle<corebluetooth::Peripheral>) -> Self {
        Device(DeviceImpl { peripheral })
    }
}

impl DeviceImpl {
    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        super::DeviceId(self.peripheral.lock(|peripheral, _| peripheral.identifier()))
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub fn name(&self) -> Result<String> {
        self.peripheral
            .lock(|peripheral, _| peripheral.name())
            .ok_or(ErrorKind::NotFound.into())
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub async fn name_async(&self) -> Result<String> {
        self.name()
    }

    /// The connection status for this device
    pub async fn is_connected(&self) -> bool {
        self.peripheral.lock(|peripheral, _| peripheral.state()) == CBPeripheralState::Connected
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
        let services = self.discover_services_inner(Some(uuid)).await?;
        Ok(services.into_iter().filter(|x| x.uuid() == uuid).collect())
    }

    async fn discover_services_inner(&self, uuid: Option<Uuid>) -> Result<Vec<Service>> {
        let mut receiver = self.peripheral.lock(|peripheral, _| {
            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.discover_services(uuid.map(BluetoothUuid::from).as_ref().map(std::slice::from_ref));

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok(receiver)
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DiscoveredServices { result } => {
                    result?;
                    break;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(error.into());
                }
                _ => (),
            }
        }

        self.services_inner()
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this method will perform service discovery.
    pub async fn services(&self) -> Result<Vec<Service>> {
        match self.services_inner() {
            Ok(services) => Ok(services),
            Err(_) => self.discover_services().await,
        }
    }

    fn services_inner(&self) -> Result<Vec<Service>> {
        self.peripheral.lock(|peripheral, executor| {
            peripheral
                .services()
                .map(|s| s.into_iter().map(|x| Service::new(executor.handle(x))).collect())
                .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "no services have been discovered"))
        })
    }

    /// Monitors the device for services changed events.
    pub async fn service_changed_indications(
        &self,
    ) -> Result<impl Stream<Item = Result<ServicesChanged>> + Send + Unpin + '_> {
        let receiver = self.peripheral.lock(|peripheral, _| {
            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            Ok(subscribe_peripheral(peripheral.delegate()))
        })?;

        Ok(receiver.filter_map(|ev| match ev {
            PeripheralEvent::ServicesChanged { invalidated_services } => {
                Some(Ok(ServicesChanged(ServicesChangedImpl(invalidated_services))))
            }
            PeripheralEvent::Disconnected { error } => Some(Err(error.into())),
            _ => None,
        }))
    }

    /// Get the current signal strength from the device in dBm.
    pub async fn rssi(&self) -> Result<i16> {
        let mut receiver = self.peripheral.lock(|peripheral, _| {
            peripheral.read_rssi();
            subscribe_peripheral(peripheral.delegate())
        });

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::ReadRssi { rssi }) => return rssi.map_err(Into::into),
                Err(err) => return Err(Error::from(err)),
                _ => (),
            }
        }
    }

    /// Open L2CAP channel given PSM
    #[cfg(feature = "l2cap")]
    pub async fn open_l2cap_channel(
        &self,
        psm: u16,
        _secure: bool,
    ) -> Result<(L2capChannelReader, L2capChannelWriter)> {
        use tracing::{debug, info};

        let mut receiver = self.peripheral.lock(|peripheral, _| {
            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            info!("starting open_l2cap_channel on {}", psm);
            peripheral.open_l2cap_channel(psm);

            Ok(subscribe_peripheral(peripheral.delegate()))
        })?;

        let l2capchannel;
        loop {
            match receiver.recv().await? {
                PeripheralEvent::L2CAPChannelOpened { result } => {
                    l2capchannel = result?;
                    break;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from(error));
                }
                o => {
                    info!("Other event: {:?}", o);
                }
            }
        }
        debug!("open_l2cap_channel success {:?}", self.peripheral);

        let reader = l2capchannel.0;
        let writer = l2capchannel.1;

        Ok((reader, writer))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServicesChangedImpl(Vec<Handle<corebluetooth::Service>>);

impl ServicesChangedImpl {
    pub fn was_invalidated(&self, service: &Service) -> bool {
        self.0.contains(&service.0.inner)
    }
}

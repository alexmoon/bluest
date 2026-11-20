#![allow(clippy::let_unit_value)]

use std::fmt;

use btuuid::BluetoothUuid;
use corebluetooth::CBPeripheralState;
use dispatch_executor::{DispatchCell, Ref};
use futures_core::Stream;
use futures_lite::StreamExt;

use super::delegates::PeripheralEvent;
use crate::corebluetooth::delegates::peripheral_delegate;
use crate::device::ServicesChanged;
use crate::error::ErrorKind;
use crate::pairing::PairingAgent;
use crate::{Device, DeviceId, Error, Result, Service, Uuid};

/// A Bluetooth LE device
#[derive(Clone)]
pub struct DeviceImpl {
    pub(super) peripheral: DispatchCell<corebluetooth::Peripheral>,
    identifier: Uuid,
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

impl fmt::Debug for DeviceImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.identifier.fmt(f)
    }
}

impl fmt::Display for DeviceImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name().as_deref().unwrap_or("(Unknown)"))
    }
}

impl Device {
    pub(super) fn new(peripheral: Ref<'_, corebluetooth::Peripheral>) -> Self {
        let identifier = peripheral.identifier();
        Device(DeviceImpl {
            identifier,
            peripheral: peripheral.retain(),
        })
    }
}

impl DeviceImpl {
    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        super::DeviceId(self.identifier)
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub fn name(&self) -> Result<String> {
        self.peripheral
            .barrier_sync(|peripheral| peripheral.name())
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
        self.peripheral.barrier_sync(|peripheral| peripheral.state()) == CBPeripheralState::Connected
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

    /// Attempt to pair this device using the provided agent
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
        let mut receiver = self.peripheral.barrier_sync(|peripheral| {
            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.discover_services(uuid.map(BluetoothUuid::from).as_ref().map(std::slice::from_ref));

            let receiver = peripheral_delegate(&peripheral).subscribe();
            Ok(receiver)
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DiscoveredServices { result } => {
                    result?;
                    break;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_error(ErrorKind::NotConnected, error));
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
        self.peripheral.barrier_sync(|peripheral| {
            peripheral
                .services()
                .map(|s| {
                    s.into_iter()
                        .map(|x| Service::new(x.borrow(peripheral.scope()), peripheral.retain()))
                        .collect()
                })
                .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "no services have been discovered"))
        })
    }

    /// Monitors the device for services changed events.
    pub async fn service_changed_indications(
        &self,
    ) -> Result<impl Stream<Item = Result<ServicesChanged>> + Send + Unpin + '_> {
        let receiver = self.peripheral.barrier_sync(|peripheral| {
            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            Ok(peripheral_delegate(&peripheral).subscribe())
        })?;

        Ok(receiver.filter_map(|ev| match ev {
            PeripheralEvent::ServicesChanged { invalidated_services } => {
                Some(Ok(ServicesChanged(ServicesChangedImpl(invalidated_services))))
            }
            PeripheralEvent::Disconnected { error } => {
                Some(Err(Error::from_kind_and_error(ErrorKind::NotConnected, error)))
            }
            _ => None,
        }))
    }

    /// Get the current signal strength from the device in dBm.
    pub async fn rssi(&self) -> Result<i16> {
        let mut receiver = self.peripheral.barrier_sync(|peripheral| {
            peripheral.read_rssi();
            peripheral_delegate(&peripheral).subscribe()
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
    pub async fn open_l2cap_channel(&self, psm: u16, _secure: bool) -> Result<super::l2cap_channel::L2capChannel> {
        use tracing::{debug, info};

        use crate::corebluetooth::delegates::peripheral_delegate;

        let receiver = self.peripheral.barrier_sync(|peripheral| {
            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            let delegate = peripheral_delegate(&peripheral);
            let receiver = {
                let mut pending = delegate.pending_l2cap_connection.lock().unwrap();
                if pending.is_some() {
                    return Err(Error::new(
                        ErrorKind::NotReady,
                        None,
                        "L2CAP connection already in progress",
                    ));
                } else {
                    let (sender, receiver) = oneshot::channel();
                    *pending = Some(sender);
                    receiver
                }
            };

            info!("starting open_l2cap_channel on {}", psm);
            peripheral.open_l2cap_channel(psm);

            Ok(receiver)
        })?;

        let l2capchannel = receiver.await??;
        debug!("open_l2cap_channel success {:?}", self.identifier);

        Ok(l2capchannel)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ServicesChangedImpl(Vec<DispatchCell<corebluetooth::Service>>);

impl fmt::Debug for ServicesChangedImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} services changed", self.0.len())
    }
}

impl ServicesChangedImpl {
    pub fn was_invalidated(&self, service: &Service) -> bool {
        self.0.contains(&service.0.inner)
    }
}

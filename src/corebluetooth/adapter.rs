#![allow(clippy::let_unit_value)]

use btuuid::BluetoothUuid;
use corebluetooth::dispatch::DispatchQoS;
use corebluetooth::{CBManagerAuthorization, CBManagerState, CentralManager};
use dispatch_executor::DispatchCell;
use futures_core::Stream;
use futures_lite::{StreamExt, stream};
use tracing::{debug, error, info, warn};

use super::delegates::{self, CentralDelegate, CentralEvent};
use crate::corebluetooth::delegates::central_delegate;
use crate::error::ErrorKind;
use crate::util::defer;
use crate::{AdapterEvent, AdvertisingDevice, ConnectionEvent, Device, DeviceId, Error, Result, Uuid};

#[derive(Default)]
pub struct AdapterConfig {
    /// Enable/disable the power alert dialog when using the adapter.
    pub show_power_alert: bool,
}

/// The system's Bluetooth adapter interface.
///
/// The default adapter for the system may be accessed with the [`Adapter::default()`] method.
#[derive(Clone)]
pub struct AdapterImpl {
    central: DispatchCell<corebluetooth::CentralManager>,
    #[cfg(not(target_os = "macos"))]
    registered_connection_events: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<Uuid, usize>>>,
}

impl PartialEq for AdapterImpl {
    fn eq(&self, other: &Self) -> bool {
        self.central == other.central
    }
}

impl Eq for AdapterImpl {}

impl std::hash::Hash for AdapterImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.central.hash(state);
    }
}

impl std::fmt::Debug for AdapterImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CoreBluetoothAdapter")
    }
}

impl AdapterImpl {
    /// Creates an interface to a Bluetooth adapter using the provided config.
    pub async fn with_config(config: AdapterConfig) -> Result<Self> {
        match CentralManager::authorization() {
            CBManagerAuthorization::AllowedAlways => info!("Bluetooth authorization is allowed"),
            CBManagerAuthorization::Denied => error!("Bluetooth authorization is denied"),
            CBManagerAuthorization::NotDetermined => {
                warn!("Bluetooth authorization is undetermined")
            }
            CBManagerAuthorization::Restricted => warn!("Bluetooth authorization is restricted"),
            val => error!("Bluetooth authorization returned unknown value {:?}", val),
        }

        let central = CentralManager::background(
            DispatchQoS::new(dispatch_executor::DispatchQoS::Default, 0),
            Box::new(CentralDelegate::new()),
            config.show_power_alert,
            None,
        );

        Ok(AdapterImpl {
            central,
            #[cfg(not(target_os = "macos"))]
            registered_connection_events: Default::default(),
        })
    }

    /// A stream of [`AdapterEvent`] which allows the application to identify when the adapter is enabled or disabled.
    pub async fn events(&self) -> Result<impl Stream<Item = Result<AdapterEvent>> + Send + Unpin + '_> {
        let receiver = self
            .central
            .barrier_sync(|central| central_delegate(&central).subscribe());
        Ok(receiver.filter_map(|x| {
            match x {
                delegates::CentralEvent::StateChanged(state) => {
                    // TODO: Check CBCentralManager::authorization()?
                    debug!("Central state is now {:?}", state);
                    match state {
                        CBManagerState::PoweredOn => Some(Ok(AdapterEvent::Available)),
                        _ => Some(Ok(AdapterEvent::Unavailable)),
                    }
                }
                _ => None,
            }
        }))
    }

    /// Asynchronously blocks until the adapter is available
    pub async fn wait_available(&self) -> Result<()> {
        let receiver = self.central.barrier_sync(|central| {
            if central.state() != CBManagerState::PoweredOn {
                Some(central_delegate(&central).subscribe())
            } else {
                None
            }
        });

        if let Some(receiver) = receiver {
            receiver
                .filter(|x| matches!(x, CentralEvent::StateChanged(CBManagerState::PoweredOn)))
                .next()
                .await;
        }

        Ok(())
    }

    /// Check if the adapter is available
    pub async fn is_available(&self) -> Result<bool> {
        Ok(self
            .central
            .barrier_sync(|central| central.state() == CBManagerState::PoweredOn))
    }

    /// Attempts to create the device identified by `id`
    pub async fn open_device(&self, id: &DeviceId) -> Result<Device> {
        self.central.barrier_sync(|central| {
            let peripherals = central.retrieve_peripherals(&[id.0]);

            peripherals
                .into_iter()
                .next()
                .map(|peripheral| Device::new(peripheral.borrow(central.scope())))
                .ok_or_else(|| Error::new(ErrorKind::NotFound, None, "opening device"))
        })
    }

    /// Finds all connected Bluetooth LE devices
    pub async fn connected_devices(&self) -> Result<Vec<Device>> {
        self.connected_devices_with_services(&[crate::btuuid::services::GENERIC_ATTRIBUTE])
            .await
    }

    /// Finds all connected devices providing any service in `services`
    ///
    /// # Panics
    ///
    /// Panics if `services` is empty.
    pub async fn connected_devices_with_services(&self, services: &[Uuid]) -> Result<Vec<Device>> {
        assert!(!services.is_empty());

        let services = services.iter().map(|x| BluetoothUuid::from(*x)).collect::<Vec<_>>();
        self.central.barrier_sync(|central| {
            let peripherals = central.retrieve_connected_peripherals(&services);
            Ok(peripherals
                .into_iter()
                .map(|peripheral| Device::new(peripheral.borrow(central.scope())))
                .collect())
        })
    }

    /// Starts scanning for Bluetooth advertising packets.
    ///
    /// Returns a stream of [`AdvertisingDevice`] structs which contain the data from the advertising packet and the
    /// [`Device`] which sent it. Scanning is automatically stopped when the stream is dropped. Inclusion of duplicate
    /// packets is a platform-specific implementation detail.
    ///
    /// If `services` is not empty, returns advertisements including at least one GATT service with a UUID in
    /// `services`. Otherwise returns all advertisements.
    pub async fn scan<'a>(
        &'a self,
        services: &'a [Uuid],
    ) -> Result<impl Stream<Item = AdvertisingDevice> + Send + Unpin + 'a> {
        let receiver = self.central.barrier_sync(|central| {
            if central.state() != CBManagerState::PoweredOn {
                return Err(Error::from(ErrorKind::AdapterUnavailable));
            }

            if central.is_scanning() {
                return Err(ErrorKind::AlreadyScanning.into());
            }

            let services = services.iter().copied().map(BluetoothUuid::from).collect::<Vec<_>>();
            let services = (!services.is_empty()).then_some(services.as_slice());
            central.scan(services, false, None);

            Ok(central_delegate(&central).subscribe())
        })?;

        let mut guard = Some(defer(|| {
            self.central.barrier_sync(|central| central.stop_scan());
        }));

        let events = receiver
            .take_while(move |x| match x {
                CentralEvent::StateChanged(state) if state != &CBManagerState::PoweredOn => {
                    if let Some(guard) = guard.take() {
                        guard.defuse();
                    }
                    false
                }
                _ => true,
            })
            .filter_map(|x| match x {
                delegates::CentralEvent::Discovered {
                    peripheral,
                    advertisement_data,
                    rssi,
                } => peripheral.barrier_sync(|peripheral| {
                    Some(AdvertisingDevice {
                        device: Device::new(peripheral),
                        adv_data: advertisement_data,
                        rssi: Some(rssi),
                    })
                }),
                _ => None,
            });

        Ok(events)
    }

    /// Finds Bluetooth devices providing any service in `services`.
    ///
    /// Returns a stream of [`Device`] structs with matching connected devices returned first. If the stream is not
    /// dropped before all matching connected devices are consumed then scanning will begin for devices advertising any
    /// of the `services`. Scanning will continue until the stream is dropped. Inclusion of duplicate devices is a
    /// platform-specific implementation detail.
    pub async fn discover_devices<'a>(
        &'a self,
        services: &'a [Uuid],
    ) -> Result<impl Stream<Item = Result<Device>> + Send + Unpin + 'a> {
        let connected = stream::iter(self.connected_devices_with_services(services).await?).map(Ok);

        // try_unfold is used to ensure we do not start scanning until the connected devices have been consumed
        let advertising = Box::pin(stream::try_unfold(None, |state| async {
            let mut stream = match state {
                Some(stream) => stream,
                None => self.scan(services).await?,
            };
            Ok(stream.next().await.map(|x| (x.device, Some(stream))))
        }));

        Ok(connected.chain(advertising))
    }

    /// Connects to the [`Device`]
    ///
    /// This method must be called before any methods on the [`Device`] which require a connection are called. After a
    /// successful return from this method, a connection has been established with the device (if one did not already
    /// exist) and the application can then interact with the device. This connection will be maintained until either
    /// [`disconnect_device`][Self::disconnect_device] is called or the `Adapter` is dropped.
    pub async fn connect_device(&self, device: &Device) -> Result<()> {
        let mut receiver = self.central.barrier_sync(|central| {
            let peripheral = device.0.peripheral.borrow(central.scope());
            if central.state() != CBManagerState::PoweredOn {
                return Err(Error::from(ErrorKind::AdapterUnavailable));
            }

            debug!("Connecting to {:?}", device);
            central.connect(&peripheral);

            Ok(central_delegate(&central).subscribe())
        })?;

        let drop = defer(|| {
            self.central.barrier_sync(|central| {
                central.cancel_peripheral_connection(&device.0.peripheral.borrow(central.scope()));
            })
        });

        while let Some(event) = receiver.next().await {
            match event {
                delegates::CentralEvent::StateChanged(state) if state != CBManagerState::PoweredOn => {
                    drop.defuse();
                    return Err(ErrorKind::AdapterUnavailable.into());
                }
                delegates::CentralEvent::Connect { peripheral_id } if peripheral_id == device.id().0 => {
                    drop.defuse();
                    return Ok(());
                }
                delegates::CentralEvent::ConnectFailed { peripheral_id, error } if peripheral_id == device.id().0 => {
                    drop.defuse();
                    return Err(Error::from(error));
                }
                _ => (),
            }
        }

        unreachable!()
    }

    /// Disconnects from the [`Device`]
    ///
    /// Once this method is called, the application will no longer have access to the [`Device`] and any methods
    /// which would require a connection will fail. If no other application has a connection to the same device,
    /// the underlying Bluetooth connection will be closed.
    pub async fn disconnect_device(&self, device: &Device) -> Result<()> {
        let mut events = self.central.barrier_sync(|central| {
            if central.state() != CBManagerState::PoweredOn {
                return Err(Error::from(ErrorKind::AdapterUnavailable));
            }

            debug!("Disconnecting from {:?}", device);
            let peripheral = device.0.peripheral.borrow(central.scope());
            central.cancel_peripheral_connection(&peripheral);

            Ok(central_delegate(&central).subscribe())
        })?;

        while let Some(event) = events.next().await {
            match event {
                CentralEvent::StateChanged(state) if state != CBManagerState::PoweredOn => {
                    return Err(ErrorKind::AdapterUnavailable.into());
                }
                CentralEvent::Disconnect {
                    peripheral_id,
                    error: None,
                } if peripheral_id == device.id().0 => return Ok(()),
                CentralEvent::Disconnect {
                    peripheral_id,
                    error: Some(err),
                } if peripheral_id == device.id().0 => return Err(Error::from(err)),
                _ => (),
            }
        }

        unreachable!()
    }

    #[cfg(not(target_os = "macos"))]
    fn register_connection_events(&self, central: &CentralManager, identifier: Uuid) -> impl Drop + use<'_> {
        {
            let mut registrations = self.registered_connection_events.lock().unwrap();
            match registrations.entry(identifier) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    *entry.get_mut() += 1;
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(1);
                    let peripherals = registrations.keys().copied().collect::<Vec<_>>();
                    central.register_for_connection_events(Some(&peripherals), None);
                }
            }
        }

        defer(move || {
            self.central.barrier_sync(|central| {
                let mut registrations = self.registered_connection_events.lock().unwrap();
                match registrations.entry(identifier) {
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        *entry.get_mut() -= 1;
                        if *entry.get() == 0 {
                            entry.remove();
                            let peripherals = registrations.keys().copied().collect::<Vec<_>>();
                            central.register_for_connection_events(Some(&peripherals), None);
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(_) => unreachable!(),
                }
            })
        })
    }

    /// Monitors a device for connection/disconnection events.
    ///
    /// # Platform specifics
    ///
    /// ## MacOS/iOS
    ///
    /// Available on iOS/iPadOS only. On MacOS no events will be generated.
    #[cfg(not(target_os = "macos"))]
    pub async fn device_connection_events<'a>(
        &'a self,
        device: &'a Device,
    ) -> Result<impl Stream<Item = ConnectionEvent> + Send + Unpin + 'a> {
        let (guard, events) = self.central.barrier_sync(|central| {
            if central.state() != CBManagerState::PoweredOn {
                return Err(Error::from(ErrorKind::AdapterUnavailable));
            }
            let guard = self.register_connection_events(&central, device.id().0);
            Ok((guard, central_delegate(&central).subscribe()))
        })?;

        Ok(events
            .take_while(|x| !matches!(x, CentralEvent::StateChanged(state) if state != &CBManagerState::PoweredOn))
            .filter_map(move |x| {
                let _guard = &guard;
                match x {
                    delegates::CentralEvent::Connect { peripheral_id } if peripheral_id == device.id().0 => {
                        Some(ConnectionEvent::Connected)
                    }
                    delegates::CentralEvent::Disconnect { peripheral_id, .. } if peripheral_id == device.id().0 => {
                        Some(ConnectionEvent::Disconnected)
                    }
                    delegates::CentralEvent::ConnectionEvent { peripheral_id, event }
                        if peripheral_id == device.id().0 =>
                    {
                        Some(event)
                    }
                    _ => None,
                }
            }))
    }

    /// Monitors a device for connection/disconnection events.
    ///
    /// # Platform specifics
    ///
    /// ## MacOS/iOS
    ///
    /// Available on iOS/iPadOS only. On MacOS no events will be generated.
    #[cfg(target_os = "macos")]
    pub async fn device_connection_events<'a>(
        &'a self,
        device: &'a Device,
    ) -> Result<impl Stream<Item = ConnectionEvent> + Send + Unpin + 'a> {
        let events = self.central.barrier_sync(|central| {
            if central.state() != CBManagerState::PoweredOn {
                return Err(Error::from(ErrorKind::AdapterUnavailable));
            }
            Ok(central_delegate(&central).subscribe())
        })?;

        Ok(events
            .take_while(|x| !matches!(x, CentralEvent::StateChanged(state) if state != &CBManagerState::PoweredOn))
            .filter_map(move |x| match x {
                delegates::CentralEvent::Connect { peripheral_id } if peripheral_id == device.id().0 => {
                    Some(ConnectionEvent::Connected)
                }
                delegates::CentralEvent::Disconnect { peripheral_id, .. } if peripheral_id == device.id().0 => {
                    Some(ConnectionEvent::Disconnected)
                }
                _ => None,
            }))
    }
}

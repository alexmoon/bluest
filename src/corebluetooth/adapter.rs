#![allow(clippy::let_unit_value)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::future::ready;

use futures_core::Stream;
use futures_lite::{stream, StreamExt};
use objc_foundation::{INSArray, INSFastEnumeration, NSArray};
use objc_id::ShareId;
use tracing::{debug, error, info, warn};

use super::delegates::{self, CentralDelegate};
use super::types::{CBCentralManager, CBManagerAuthorization, CBManagerState, CBUUID, NSUUID};
use crate::corebluetooth::types::{dispatch_get_global_queue, QOS_CLASS_UTILITY};
use crate::error::ErrorKind;
use crate::util::defer;
use crate::{
    AdapterEvent, AdvertisementData, AdvertisingDevice, ConnectionEvent, Device, DeviceId, Error, Result, Uuid,
};

/// The system's Bluetooth adapter interface.
///
/// The default adapter for the system may be accessed with the [`Adapter::default()`] method.
#[derive(Clone)]
pub struct AdapterImpl {
    central: ShareId<CBCentralManager>,
    delegate: ShareId<CentralDelegate>,
    scanning: Arc<AtomicBool>,
    #[cfg(not(target_os = "macos"))]
    registered_connection_events: Arc<std::sync::Mutex<std::collections::HashMap<DeviceId, usize>>>,
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
        f.debug_tuple("Adapter").field(&self.central).finish()
    }
}

impl AdapterImpl {
    /// Creates an interface to the default Bluetooth adapter for the system
    pub async fn default() -> Option<Self> {
        match CBCentralManager::authorization() {
            CBManagerAuthorization::ALLOWED_ALWAYS => info!("Bluetooth authorization is allowed"),
            CBManagerAuthorization::DENIED => error!("Bluetooth authorization is denied"),
            CBManagerAuthorization::NOT_DETERMINED => warn!("Bluetooth authorization is undetermined"),
            CBManagerAuthorization::RESTRICTED => warn!("Bluetooth authorization is restricted"),
            val => error!("Bluetooth authorization returned unknown value {:?}", val),
        }

        let delegate = CentralDelegate::new()?.share();
        let central = unsafe {
            let queue = dispatch_get_global_queue(QOS_CLASS_UTILITY, 0);
            if queue.is_null() {
                return None;
            }
            CBCentralManager::with_delegate(&delegate, queue).share()
        };

        Some(AdapterImpl {
            central,
            delegate,
            scanning: Default::default(),
            #[cfg(not(target_os = "macos"))]
            registered_connection_events: Default::default(),
        })
    }

    /// A stream of [`AdapterEvent`] which allows the application to identify when the adapter is enabled or disabled.
    pub async fn events(&self) -> Result<impl Stream<Item = Result<AdapterEvent>> + Send + Unpin + '_> {
        let receiver = self.delegate.sender().new_receiver();
        Ok(receiver.filter_map(|x| {
            match x {
                delegates::CentralEvent::StateChanged => {
                    // TODO: Check CBCentralManager::authorization()?
                    let state = self.central.state();
                    debug!("Central state is now {:?}", state);
                    match state {
                        CBManagerState::POWERED_ON => Some(Ok(AdapterEvent::Available)),
                        _ => Some(Ok(AdapterEvent::Unavailable)),
                    }
                }
                _ => None,
            }
        }))
    }

    /// Asynchronously blocks until the adapter is available
    pub async fn wait_available(&self) -> Result<()> {
        let events = self.events();
        if self.central.state() != CBManagerState::POWERED_ON {
            events
                .await?
                .skip_while(|x| x.is_ok() && !matches!(x, Ok(AdapterEvent::Available)))
                .next()
                .await
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::Internal,
                        None,
                        "adapter event stream closed unexpectedly".to_string(),
                    )
                })??;
        }
        Ok(())
    }

    /// Attempts to create the device identified by `id`
    pub async fn open_device(&self, id: &DeviceId) -> Result<Device> {
        let identifiers = NSArray::from_vec(vec![NSUUID::from_uuid(id.0)]);
        let peripherals = self.central.retrieve_peripherals_with_identifiers(&identifiers);
        peripherals
            .first_object()
            .map(|x| Device::new(unsafe { ShareId::from_ptr(x as *const _ as *mut _) }))
            .ok_or_else(|| Error::new(ErrorKind::NotFound, None, "opening device".to_string()))
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

        let services = {
            let vec = services.iter().copied().map(CBUUID::from_uuid).collect::<Vec<_>>();
            NSArray::from_vec(vec)
        };
        let peripherals = self.central.retrieve_connected_peripherals_with_services(&services);
        Ok(peripherals
            .enumerator()
            .map(|x| Device::new(unsafe { ShareId::from_ptr(x as *const _ as *mut _) }))
            .collect())
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
        if self.central.state() != CBManagerState::POWERED_ON {
            return Err(ErrorKind::AdapterUnavailable.into());
        }

        if self.scanning.swap(true, Ordering::Acquire) {
            return Err(ErrorKind::AlreadyScanning.into());
        }

        let services = (!services.is_empty()).then(|| {
            let vec = services.iter().copied().map(CBUUID::from_uuid).collect::<Vec<_>>();
            NSArray::from_vec(vec)
        });

        let guard = defer(|| {
            self.central.stop_scan();
            self.scanning.store(false, Ordering::Release);
        });

        let events = self
            .delegate
            .sender()
            .new_receiver()
            .take_while(|_| self.central.state() == CBManagerState::POWERED_ON)
            .filter_map(move |x| {
                let _guard = &guard;
                match x {
                    delegates::CentralEvent::Discovered {
                        peripheral,
                        adv_data,
                        rssi,
                    } => Some(AdvertisingDevice {
                        device: Device::new(peripheral),
                        adv_data: AdvertisementData::from_nsdictionary(&adv_data),
                        rssi: Some(rssi),
                    }),
                    _ => None,
                }
            });

        self.central
            .scan_for_peripherals_with_services(services.as_deref(), None);

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
        if self.central.state() != CBManagerState::POWERED_ON {
            return Err(ErrorKind::AdapterUnavailable.into());
        }

        let mut events = self.delegate.sender().new_receiver();
        debug!("Connecting to {:?}", device);
        self.central.connect_peripheral(&device.0.peripheral, None);
        while let Some(event) = events.next().await {
            if self.central.state() != CBManagerState::POWERED_ON {
                return Err(ErrorKind::AdapterUnavailable.into());
            }
            match event {
                delegates::CentralEvent::Connect { peripheral } if peripheral == device.0.peripheral => return Ok(()),
                delegates::CentralEvent::ConnectFailed { peripheral, error } if peripheral == device.0.peripheral => {
                    return Err(error.map_or(ErrorKind::ConnectionFailed.into(), Error::from_nserror));
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
        if self.central.state() != CBManagerState::POWERED_ON {
            return Err(ErrorKind::AdapterUnavailable.into());
        }

        let mut events = self.delegate.sender().new_receiver();
        debug!("Disconnecting from {:?}", device);
        self.central.cancel_peripheral_connection(&device.0.peripheral);
        while let Some(event) = events.next().await {
            if self.central.state() != CBManagerState::POWERED_ON {
                return Err(ErrorKind::AdapterUnavailable.into());
            }
            match event {
                delegates::CentralEvent::Disconnect {
                    peripheral,
                    error: None,
                } if peripheral == device.0.peripheral => return Ok(()),
                delegates::CentralEvent::Disconnect {
                    peripheral,
                    error: Some(err),
                } if peripheral == device.0.peripheral => return Err(Error::from_nserror(err)),
                _ => (),
            }
        }

        unreachable!()
    }

    #[cfg(not(target_os = "macos"))]
    fn register_connection_events(&self, device: DeviceId) -> impl Drop + '_ {
        use std::collections::HashMap;

        use objc_foundation::{INSDictionary, NSDictionary, NSString};
        use objc_id::Id;

        use crate::corebluetooth::types::connection_event_matching_option_peripheral_uuids;

        let mut guard = self.registered_connection_events.lock().unwrap();

        fn options(devices: &HashMap<DeviceId, usize>) -> Id<NSDictionary<NSString, NSArray<CBUUID>>> {
            let ids = devices.keys().map(|x| CBUUID::from_uuid(x.0)).collect();
            NSDictionary::from_keys_and_objects(
                &[connection_event_matching_option_peripheral_uuids()],
                vec![NSArray::from_vec(ids)],
            )
        }

        match guard.entry(device.clone()) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                *e.get_mut() += 1;
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(1);
                self.central
                    .register_for_connection_events_with_options(Some(&*options(&guard)));
            }
        }

        defer(move || {
            let mut guard = self.registered_connection_events.lock().unwrap();
            match guard.entry(device) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    *e.get_mut() -= 1;
                    if *e.get() == 0 {
                        e.remove();
                        self.central
                            .register_for_connection_events_with_options(Some(&*options(&guard)))
                    }
                }
                std::collections::hash_map::Entry::Vacant(_) => unreachable!(),
            }
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
        let events = BroadcastStream::new(self.delegate.sender().subscribe());
        let guard = self.register_connection_events(device.id());

        Ok(events
            .take_while(|_| ready(self.central.state() == CBManagerState::POWERED_ON))
            .filter_map(move |x| {
                let _guard = &guard;
                ready(match x {
                    Ok(delegates::CentralEvent::ConnectionEvent { peripheral, event })
                        if peripheral.identifier() == device.0.peripheral.identifier() =>
                    {
                        Some(event.into())
                    }
                    _ => None,
                })
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
        let events = self.delegate.sender().new_receiver();
        Ok(events
            .take_while(|_| self.central.state() == CBManagerState::POWERED_ON)
            .filter_map(move |x| match x {
                delegates::CentralEvent::Connect { peripheral } if peripheral == device.0.peripheral => {
                    Some(ConnectionEvent::Connected)
                }
                delegates::CentralEvent::Disconnect { peripheral, .. } if peripheral == device.0.peripheral => {
                    Some(ConnectionEvent::Disconnected)
                }
                _ => None,
            }))
    }
}

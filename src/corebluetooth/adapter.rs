#![allow(clippy::let_unit_value)]

use std::ffi::CStr;
use std::future::ready;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use objc_foundation::{INSArray, INSFastEnumeration, NSArray};
use objc_id::ShareId;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, error, info, warn};

use super::delegates::{self, CentralDelegate};
use super::device::Device;
use super::types::{
    dispatch_queue_create, dispatch_release, nil, CBCentralManager, CBManagerAuthorization, CBManagerState, CBUUID,
    NSUUID,
};
use crate::error::ErrorKind;
use crate::util::defer;
use crate::{AdapterEvent, AdvertisementData, AdvertisingDevice, DeviceId, Error, Result, Uuid};

/// The system's Bluetooth adapter interface.
///
/// The default adapter for the system may be accessed with the [`Adapter::default()`] method.
#[derive(Clone)]
pub struct Adapter {
    central: ShareId<CBCentralManager>,
    sender: tokio::sync::broadcast::Sender<delegates::CentralEvent>,
    scanning: Arc<AtomicBool>,
}

impl PartialEq for Adapter {
    fn eq(&self, other: &Self) -> bool {
        self.central == other.central
    }
}

impl Eq for Adapter {}

impl std::hash::Hash for Adapter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.central.hash(state);
    }
}

impl std::fmt::Debug for Adapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Adapter").field(&self.central).finish()
    }
}

impl Adapter {
    /// Creates an interface to the default Bluetooth adapter for the system
    pub async fn default() -> Option<Self> {
        match CBCentralManager::authorization() {
            CBManagerAuthorization::ALLOWED_ALWAYS => info!("Bluetooth authorization is allowed"),
            CBManagerAuthorization::DENIED => error!("Bluetooth authorization is denied"),
            CBManagerAuthorization::NOT_DETERMINED => warn!("Bluetooth authorization is undetermined"),
            CBManagerAuthorization::RESTRICTED => warn!("Bluetooth authorization is restricted"),
            val => error!("Bluetooth authorization returned unknown value {:?}", val),
        }

        let (sender, _) = tokio::sync::broadcast::channel(16);
        let delegate = CentralDelegate::with_sender(sender.clone())?;
        let central = unsafe {
            let queue = dispatch_queue_create(CStr::from_bytes_with_nul(b"BluetoothQueue\0").unwrap().as_ptr(), nil);
            if queue.is_null() {
                return None;
            }
            let central = CBCentralManager::with_delegate(delegate, queue);
            dispatch_release(queue);
            central.share()
        };

        Some(Adapter {
            central,
            sender,
            scanning: Arc::new(AtomicBool::new(false)),
        })
    }

    /// A stream of [`AdapterEvent`] which allows the application to identify when the adapter is enabled or disabled.
    pub async fn events(&self) -> Result<impl Stream<Item = Result<AdapterEvent>> + '_> {
        let receiver = self.sender.subscribe();
        Ok(BroadcastStream::new(receiver).filter_map(|x| {
            ready(match x {
                Ok(delegates::CentralEvent::StateChanged) => {
                    // TODO: Check CBCentralManager::authorization()?
                    let state = self.central.state();
                    debug!("Central state is now {:?}", state);
                    match state {
                        CBManagerState::POWERED_ON => Some(Ok(AdapterEvent::Available)),
                        _ => Some(Ok(AdapterEvent::Unavailable)),
                    }
                }
                Err(err) => Some(Err(Error::new(
                    ErrorKind::Internal,
                    Some(Box::new(err)),
                    "adapter event stream".to_string(),
                ))),
                _ => None,
            })
        }))
    }

    /// Asynchronously blocks until the adapter is available
    pub async fn wait_available(&self) -> Result<()> {
        let events = self.events();
        if self.central.state() != CBManagerState::POWERED_ON {
            events
                .await?
                .skip_while(|x| ready(x.is_ok() && !matches!(x, Ok(AdapterEvent::Available))))
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
        let peripherals = self.central.retrieve_peripherals_with_identifiers(identifiers);
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
        let peripherals = self.central.retrieve_connected_peripherals_with_services(services);
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
    pub async fn scan<'a>(&'a self, services: &'a [Uuid]) -> Result<impl Stream<Item = AdvertisingDevice> + 'a> {
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

        let events = BroadcastStream::new(self.sender.subscribe())
            .take_while(|_| ready(self.central.state() == CBManagerState::POWERED_ON))
            .filter_map(move |x| {
                let _guard = &guard;
                ready(match x {
                    Ok(delegates::CentralEvent::Discovered {
                        peripheral,
                        adv_data,
                        rssi,
                    }) => Some(AdvertisingDevice {
                        device: Device::new(peripheral),
                        adv_data: AdvertisementData::from_nsdictionary(&adv_data),
                        rssi: Some(rssi),
                    }),
                    _ => None,
                })
            });

        self.central.scan_for_peripherals_with_services(services, None);

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
    ) -> Result<impl Stream<Item = Result<Device>> + 'a> {
        use futures_util::TryFutureExt;

        let connected = self.connected_devices_with_services(services).await?;
        let advertising = Box::pin(async {
            match self.scan(services).await {
                Ok(stream) => Ok(stream.map(|x| Ok(x.device))),
                Err(err) => Err(err),
            }
        })
        .try_flatten_stream();

        Ok(futures_util::stream::iter(connected).map(Ok).chain(advertising))
    }

    /// Connects to the [`Device`]
    ///
    /// # Platform specifics
    ///
    /// ## MacOS/iOS
    ///
    /// This method must be called before any methods on the [`Device`] which require a connection are called. After a
    /// successful return from this method a connection has been established with the device (if one did not already
    /// exist) and the application can then interact with the device.
    ///
    /// ## Windows
    ///
    /// On Windows, device connections are automatically managed by the OS. This method has no effect. Instead, a
    /// connection will automatically be established, if necessary, when methods on the device requiring a connection
    /// are called.
    pub async fn connect_device(&self, device: &Device) -> Result<()> {
        if self.central.state() != CBManagerState::POWERED_ON {
            return Err(ErrorKind::AdapterUnavailable.into());
        }

        let mut events = BroadcastStream::new(self.sender.subscribe());
        debug!("Connecting to {:?}", device);
        self.central.connect_peripheral(&*device.peripheral, None);
        while let Some(event) = events.next().await {
            if self.central.state() != CBManagerState::POWERED_ON {
                return Err(ErrorKind::AdapterUnavailable.into());
            }
            match event {
                Ok(delegates::CentralEvent::Connect { peripheral }) if peripheral == device.peripheral => break,
                Ok(delegates::CentralEvent::ConnectFailed { peripheral, error }) if peripheral == device.peripheral => {
                    return Err(error.map_or(ErrorKind::ConnectionFailed.into(), Error::from_nserror));
                }
                _ => (),
            }
        }

        Ok(())
    }

    /// Disconnects from the [`Device`]
    ///
    /// # Platform specifics
    ///
    /// ## MacOS/iOS
    ///
    /// Once this method is called, the application will no longer have access to the [`Device`] and any methods
    /// which would require a connection will fail. If no other application has a connection to the same device,
    /// the underlying Bluetooth connection will be closed.
    ///
    /// ## Windows
    ///
    /// On Windows, device connections are automatically managed by the OS. This method has no effect. Instead, the
    /// connection will be closed only when the [`Device`] and all its child objects are dropped.
    pub async fn disconnect_device(&self, device: &Device) -> Result<()> {
        if self.central.state() != CBManagerState::POWERED_ON {
            return Err(ErrorKind::AdapterUnavailable.into());
        }

        let mut events = BroadcastStream::new(self.sender.subscribe());
        debug!("Disconnecting from {:?}", device);
        self.central.cancel_peripheral_connection(&*device.peripheral);
        while let Some(event) = events.next().await {
            if self.central.state() != CBManagerState::POWERED_ON {
                return Err(ErrorKind::AdapterUnavailable.into());
            }
            match event {
                Ok(delegates::CentralEvent::Disconnect {
                    peripheral,
                    error: None,
                }) if peripheral == device.peripheral => break,
                Ok(delegates::CentralEvent::Disconnect {
                    peripheral,
                    error: Some(err),
                }) if peripheral == device.peripheral => return Err(Error::from_nserror(err)),
                Err(err) => return Err(Error::from_stream_recv_error(err)),
                _ => (),
            }
        }

        Ok(())
    }
}

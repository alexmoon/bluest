#![allow(clippy::let_unit_value)]

use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::Stream;
use objc::runtime::{BOOL, NO};
use objc::{msg_send, sel, sel_impl};
use objc_foundation::{
    INSArray, INSData, INSDictionary, INSFastEnumeration, INSString, NSArray, NSData, NSDictionary, NSObject, NSString,
};
use objc_id::Id;
use smallvec::SmallVec;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::{debug, error};
use uuid::Uuid;

use super::delegates::{self, CentralDelegate};
use super::device::Device;
use super::types::{dispatch_queue_create, dispatch_release, id_or_nil, nil, CBCentralManager, CBManagerState, CBUUID};

use crate::error::ErrorKind;
use crate::{AdapterEvent, AdvertisementData, AdvertisingDevice, ManufacturerData, Result};

impl From<&NSDictionary<NSString, NSObject>> for AdvertisementData {
    fn from(adv_data: &NSDictionary<NSString, NSObject>) -> Self {
        let is_connectable = adv_data
            .object_for(&*INSString::from_str("kCBAdvDataIsConnectable"))
            .map(|val| unsafe {
                let n: BOOL = msg_send![val, boolValue];
                n != NO
            })
            .unwrap_or(false);

        let local_name = adv_data
            .object_for(&*INSString::from_str("kCBAdvDataLocalName"))
            .map(|val| unsafe { std::mem::transmute::<_, &NSString>(val).as_str().to_owned() });

        let manufacturer_data = adv_data
            .object_for(&*INSString::from_str("kCBAdvDataManufacturerData"))
            .map(|val| unsafe { std::mem::transmute::<_, &NSData>(val).bytes() })
            .and_then(|val| {
                (val.len() >= 2).then(|| ManufacturerData {
                    company_id: u16::from_le_bytes(val[0..2].try_into().unwrap()),
                    data: SmallVec::from_slice(&val[2..]),
                })
            });

        let tx_power_level: Option<i16> = adv_data
            .object_for(&*INSString::from_str("kCBAdvDataTxPowerLevel"))
            .map(|val| unsafe { msg_send![val, shortValue] });

        let service_data = if let Some(val) = adv_data.object_for(&*INSString::from_str("kCBAdvDataServiceData")) {
            unsafe {
                let val: &NSDictionary<CBUUID, NSData> = std::mem::transmute(val);
                let mut res = HashMap::with_capacity(val.count());
                for k in val.enumerator() {
                    res.insert(k.to_uuid(), SmallVec::from_slice(val.object_for(k).unwrap().bytes()));
                }
                res
            }
        } else {
            HashMap::new()
        };

        let services = adv_data
            .object_for(&*INSString::from_str("kCBAdvDataServiceUUIDs"))
            .into_iter()
            .chain(
                adv_data
                    .object_for(&*INSString::from_str("kCBAdvDataHashedServiceUUIDs"))
                    .into_iter(),
            )
            .flat_map(|x| {
                let val: &NSArray<CBUUID> = unsafe { std::mem::transmute(x) };
                val.enumerator()
            })
            .map(|x| x.to_uuid())
            .collect::<SmallVec<_>>();

        let solicited_services =
            if let Some(val) = adv_data.object_for(&*INSString::from_str("kCBAdvDataSolicitedServiceUUIDs")) {
                let val: &NSArray<CBUUID> = unsafe { std::mem::transmute(val) };
                val.enumerator().map(|x| x.to_uuid()).collect()
            } else {
                SmallVec::new()
            };

        AdvertisementData {
            local_name,
            manufacturer_data,
            service_data,
            services,
            solicited_services,
            tx_power_level,
            is_connectable,
        }
    }
}

/// The system's Bluetooth adapter interface.
///
/// The default adapter for the system may be accessed with the [Adapter::default()] method.
pub struct Adapter {
    central: Id<CBCentralManager>,
    sender: tokio::sync::broadcast::Sender<delegates::CentralEvent>,
    scanning: AtomicBool,
}

impl Adapter {
    /// Creates an interface to the default Bluetooth adapter for the system
    pub async fn default() -> Result<Self> {
        let (sender, _) = tokio::sync::broadcast::channel(16);
        let delegate = CentralDelegate::with_sender(sender.clone());
        let central = unsafe {
            let queue = dispatch_queue_create(CStr::from_bytes_with_nul(b"BluetoothQueue\0").unwrap().as_ptr(), nil);
            let central = CBCentralManager::with_delegate(delegate, queue);
            dispatch_release(queue);
            central
        };

        Ok(Adapter {
            central,
            sender,
            scanning: AtomicBool::new(false),
        })
    }

    /// A stream of [AdapterEvent] which allows the application to identify when the adapter is enabled or disabled.
    pub fn events(&self) -> impl Stream<Item = AdapterEvent> + '_ {
        let receiver = self.sender.subscribe();
        BroadcastStream::new(receiver).filter_map(|x| match x {
            Ok(delegates::CentralEvent::StateChanged) => {
                let state = self.central.state();
                debug!("Central state is now {:?}", state);
                match state {
                    CBManagerState::PoweredOn => Some(AdapterEvent::Available),
                    _ => Some(AdapterEvent::Unavailable),
                }
            }
            _ => None,
        })
    }

    /// Asynchronously blocks until the adapter is available
    pub async fn wait_available(&self) -> Result<()> {
        let events = self.events();
        if self.central.state() != CBManagerState::PoweredOn {
            let _ = events.skip_while(|x| *x != AdapterEvent::Available).next().await;
        }
        Ok(())
    }

    // pub async fn discover_devices(&self, services: Option<&[Uuid]>) -> Result<impl Stream<Item = DiscoveredDevice> + '_> {}

    // pub async fn known_devices(&self, services: Option<&[Uuid]>)

    /// Starts scanning for Bluetooth advertising packets.
    ///
    /// Returns a stream of [AdvertisingDevice] structs which contain the data from the advertising packet and the
    /// [Device] which sent it. Scanning is automatically stopped when the stream is dropped. Inclusion of duplicate
    /// packets is a platform-specific implementation detail.
    pub async fn scan<'a>(
        &'a self,
        services: Option<&'a [Uuid]>,
    ) -> Result<impl Stream<Item = AdvertisingDevice> + 'a> {
        unsafe {
            if self.central.state() != CBManagerState::PoweredOn {
                Err(ErrorKind::AdapterUnavailable)?
            }

            if self.scanning.swap(true, Ordering::Acquire) {
                Err(ErrorKind::AlreadyScanning)?;
            }

            let services = services.map(|x| {
                let vec = x.iter().copied().map(CBUUID::from_uuid).collect::<Vec<_>>();
                NSArray::from_vec(vec)
            });

            let guard = scopeguard::guard((), |_| {
                let _: () = msg_send![self.central, stopScan];
                self.scanning.store(false, Ordering::Release);
            });

            let events = BroadcastStream::new(self.sender.subscribe())
                .take_while(|_| self.central.state() == CBManagerState::PoweredOn)
                .filter_map(move |x| {
                    let _guard = &guard;
                    match x {
                        Ok(delegates::CentralEvent::Discovered {
                            peripheral,
                            adv_data,
                            rssi,
                        }) => Some(AdvertisingDevice {
                            device: Device::new(peripheral),
                            adv_data: AdvertisementData::from(&*adv_data),
                            rssi: Some(rssi),
                        }),
                        _ => None,
                    }
                });

            let _: () = msg_send![self.central, scanForPeripheralsWithServices: id_or_nil(&services) options: nil ];

            Ok(events)
        }
    }

    /// Connects to the [Device]
    pub async fn connect(&self, device: &Device) -> Result<()> {
        if self.central.state() != CBManagerState::PoweredOn {
            Err(ErrorKind::AdapterUnavailable)?
        }

        let mut events = BroadcastStream::new(self.sender.subscribe());
        debug!("Connecting to {:?}", device);
        self.central.connect_peripheral(&*device.peripheral, None);
        while let Some(event) = events.next().await {
            if self.central.state() != CBManagerState::PoweredOn {
                Err(ErrorKind::AdapterUnavailable)?
            }
            match event {
                Ok(delegates::CentralEvent::Connect { peripheral }) if peripheral == device.peripheral => return Ok(()),
                Ok(delegates::CentralEvent::ConnectFailed { peripheral, error }) if peripheral == device.peripheral => {
                    error!("Failed to connect to {:?}: {:?}", peripheral, error);
                    match error {
                        Some(err) => Err(&*err)?,
                        None => Err(ErrorKind::ConnectionFailed)?,
                    }
                }
                _ => (),
            }
        }

        unreachable!()
    }

    /// Disconnects from the [Device]
    pub async fn disconnect(&self, device: &Device) -> Result<()> {
        if self.central.state() != CBManagerState::PoweredOn {
            Err(ErrorKind::AdapterUnavailable)?
        }

        let mut events = BroadcastStream::new(self.sender.subscribe());
        debug!("Disconnecting from {:?}", device);
        self.central.cancel_peripheral_connection(&*device.peripheral);
        while let Some(event) = events.next().await {
            if self.central.state() != CBManagerState::PoweredOn {
                Err(ErrorKind::AdapterUnavailable)?
            }
            match event {
                Ok(delegates::CentralEvent::Disconnect { peripheral, error }) if peripheral == device.peripheral => {
                    match error {
                        Some(err) => {
                            error!("Failed to disconnect from {:?}: {:?}", peripheral, err);
                            Err(&*err)?
                        }
                        None => return Ok(()),
                    }
                }
                _ => (),
            }
        }

        unreachable!()
    }
}

#![allow(unused_variables)]
use objc2::rc::Retained;
use objc2::{AnyThread, DefinedClass, Message, define_class, msg_send};
use objc2_core_bluetooth::{
    CBCentralManager, CBCentralManagerDelegate, CBCharacteristic, CBConnectionEvent, CBDescriptor,
    CBL2CAPChannel, CBPeripheral, CBPeripheralDelegate, CBService,
};
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSString,
};
use tracing::{debug, error};

#[derive(Clone)]
pub enum CentralEvent {
    Connect {
        peripheral: Retained<CBPeripheral>,
    },
    Disconnect {
        peripheral: Retained<CBPeripheral>,
        error: Option<Retained<NSError>>,
    },
    ConnectFailed {
        peripheral: Retained<CBPeripheral>,
        error: Option<Retained<NSError>>,
    },
    ConnectionEvent {
        peripheral: Retained<CBPeripheral>,
        event: CBConnectionEvent,
    },
    Discovered {
        peripheral: Retained<CBPeripheral>,
        adv_data: Retained<NSDictionary<NSString>>,
        rssi: i16,
    },
    StateChanged,
}

impl std::fmt::Debug for CentralEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect { peripheral } => f
                .debug_struct("Connect")
                .field("peripheral", peripheral)
                .finish(),
            Self::Disconnect { peripheral, error } => f
                .debug_struct("Disconnect")
                .field("peripheral", peripheral)
                .field("error", error)
                .finish(),
            Self::ConnectFailed { peripheral, error } => f
                .debug_struct("ConnectFailed")
                .field("peripheral", peripheral)
                .field("error", error)
                .finish(),
            Self::ConnectionEvent { peripheral, event } => f
                .debug_struct("ConnectionEvent")
                .field("peripheral", peripheral)
                .field("event", event)
                .finish(),
            Self::Discovered {
                peripheral, rssi, ..
            } => f
                .debug_struct("Discovered")
                .field("peripheral", peripheral)
                .field("rssi", rssi)
                .finish(),
            Self::StateChanged => write!(f, "StateChanged"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PeripheralEvent {
    Connected,
    Disconnected {
        error: Option<Retained<NSError>>,
    },
    DiscoveredServices {
        error: Option<Retained<NSError>>,
    },
    DiscoveredIncludedServices {
        service: Retained<CBService>,
        error: Option<Retained<NSError>>,
    },
    DiscoveredCharacteristics {
        service: Retained<CBService>,
        error: Option<Retained<NSError>>,
    },
    DiscoveredDescriptors {
        characteristic: Retained<CBCharacteristic>,
        error: Option<Retained<NSError>>,
    },
    CharacteristicValueUpdate {
        characteristic: Retained<CBCharacteristic>,
        data: Option<Retained<NSData>>,
        error: Option<Retained<NSError>>,
    },
    DescriptorValueUpdate {
        descriptor: Retained<CBDescriptor>,
        error: Option<Retained<NSError>>,
    },
    CharacteristicValueWrite {
        characteristic: Retained<CBCharacteristic>,
        error: Option<Retained<NSError>>,
    },
    DescriptorValueWrite {
        descriptor: Retained<CBDescriptor>,
        error: Option<Retained<NSError>>,
    },
    ReadyToWrite,
    NotificationStateUpdate {
        characteristic: Retained<CBCharacteristic>,
        error: Option<Retained<NSError>>,
    },
    ReadRssi {
        rssi: i16,
        error: Option<Retained<NSError>>,
    },
    NameUpdate,
    ServicesChanged {
        invalidated_services: Vec<Retained<CBService>>,
    },
    #[allow(unused)]
    L2CAPChannelOpened {
        channel: Retained<CBL2CAPChannel>,
        error: Option<Retained<NSError>>,
    },
}

#[derive(Debug)]
pub(crate) struct CentralDelegateIvars {
    pub sender: async_broadcast::Sender<CentralEvent>,
    _receiver: async_broadcast::InactiveReceiver<CentralEvent>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = CentralDelegateIvars]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct CentralDelegate;

    unsafe impl NSObjectProtocol for CentralDelegate {}

    unsafe impl CBCentralManagerDelegate for CentralDelegate {
        #[unsafe(method(centralManagerDidUpdateState:))]
        fn did_update_state(&self, central: &CBCentralManager) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(CentralEvent::StateChanged);
        }

        #[unsafe(method(centralManager:didConnectPeripheral:))]
        fn did_connect_peripheral(&self, central: &CBCentralManager, peripheral: &CBPeripheral) {
            let sender = &self.ivars().sender;
            unsafe {
                if let Some(delegate) = peripheral
                    .delegate()
                    .map(|d| d.downcast::<PeripheralDelegate>().ok())
                    .flatten()
                {
                    let _res = delegate.sender().try_broadcast(PeripheralEvent::Connected);
                }
                let event = CentralEvent::Connect {
                    peripheral: peripheral.retain(),
                };
                debug!("CentralDelegate received {:?}", event);
                let _ = sender.try_broadcast(event);
            }
        }

        #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
        fn did_disconnect_peripheral_error(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            unsafe {
                let sender = &self.ivars().sender;
                if let Some(delegate) = peripheral
                    .delegate()
                    .map(|d| d.downcast::<PeripheralDelegate>().ok())
                    .flatten()
                {
                    let _res = delegate
                        .sender()
                        .try_broadcast(PeripheralEvent::Disconnected {
                            error: error.map(|e| e.retain()),
                        });
                }
                let event = CentralEvent::Disconnect {
                    peripheral: peripheral.retain(),
                    error: error.map(|e| e.retain()),
                };
                debug!("CentralDelegate received {:?}", event);
                let _res = sender.try_broadcast(event);
            }
        }

        #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
        fn did_discover_peripheral(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
            adv_data: &NSDictionary<NSString>,
            rssi: &NSNumber,
        ) {
            let sender = &self.ivars().sender;
            let rssi: i16 = rssi.shortValue();
            let event = CentralEvent::Discovered {
                peripheral: peripheral.retain(),
                adv_data: adv_data.retain(),
                rssi,
            };
            debug!("CentralDelegate received {:?}", event);
            let _res = sender.try_broadcast(event);
        }

        #[unsafe(method(centralManager:connectionEventDidOccur:forPeripheral:))]
        fn on_connection_event(
            &self,
            central: &CBCentralManager,
            event: CBConnectionEvent,
            peripheral: &CBPeripheral,
        ) {
            let sender = &self.ivars().sender;
            match event.try_into() {
                Ok(event) => {
                    let event = CentralEvent::ConnectionEvent {
                        peripheral: peripheral.retain(),
                        event,
                    };
                    debug!("CentralDelegate received {:?}", event);
                    let _res = sender.try_broadcast(event);
                }
                Err(err) => {
                    error!("Invalid value for CBConnectionEvent: {}", err);
                }
            }
        }

        #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
        fn did_fail_to_connect(
            &self,
            central: &CBCentralManager,
            peripheral: &CBPeripheral,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(CentralEvent::ConnectFailed {
                    peripheral: peripheral.retain(),
                    error: error.map(|e| e.retain()),
                });
        }
    }
);

impl CentralDelegate {
    pub fn new() -> Retained<Self> {
        let (mut sender, receiver) = async_broadcast::broadcast::<CentralEvent>(16);
        sender.set_overflow(true);
        let receiver = receiver.deactivate();

        let ivars = CentralDelegateIvars {
            sender,
            _receiver: receiver,
        };
        let this = CentralDelegate::alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    pub fn sender(&self) -> async_broadcast::Sender<CentralEvent> {
        self.ivars().sender.clone()
    }
}

#[derive(Debug)]
pub(crate) struct PeripheralDelegateIvars {
    pub sender: async_broadcast::Sender<PeripheralEvent>,
    _receiver: async_broadcast::InactiveReceiver<PeripheralEvent>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = PeripheralDelegateIvars]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct PeripheralDelegate;

    unsafe impl NSObjectProtocol for PeripheralDelegate {}

    unsafe impl CBPeripheralDelegate for PeripheralDelegate {
        #[unsafe(method(peripheral:didUpdateValueForCharacteristic:error:))]
        fn did_update_value_for_characteristic_error(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            unsafe {
                let sender = &self.ivars().sender;
                let data = characteristic.value();
                let event = PeripheralEvent::CharacteristicValueUpdate {
                    characteristic: characteristic.retain(),
                    data,
                    error: error.map(|e| e.retain()),
                };
                debug!("PeripheralDelegate received {:?}", event);
                let _res = sender.try_broadcast(event);
            }
        }

        #[unsafe(method(peripheral:didDiscoverServices:))]
        fn did_discover_services(&self, peripheral: &CBPeripheral, error: Option<&NSError>) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::DiscoveredServices {
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheral:didDiscoverIncludedServicesForService:error:))]
        fn did_discover_included_services(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            let _ =
                self.ivars()
                    .sender
                    .try_broadcast(PeripheralEvent::DiscoveredIncludedServices {
                        service: service.retain(),
                        error: error.map(|e| e.retain()),
                    });
        }

        #[unsafe(method(peripheralDidUpdateName:))]
        fn did_update_name(&self, peripheral: &CBPeripheral) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::NameUpdate);
        }

        #[unsafe(method(peripheral:didModifyServices:))]
        fn did_modify_services(
            &self,
            peripheral: &CBPeripheral,
            invalidated_services: &NSArray<CBService>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::ServicesChanged {
                    invalidated_services: invalidated_services.to_vec(),
                });
        }

        #[unsafe(method(peripheralDidUpdateRSSI:error:))]
        fn did_update_rssi(&self, peripheral: &CBPeripheral, error: Option<&NSError>) {}

        #[unsafe(method(peripheral:didReadRSSI:error:))]
        fn did_read_rssi(
            &self,
            peripheral: &CBPeripheral,
            rssi: &NSNumber,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::ReadRssi {
                    rssi: rssi.shortValue(),
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheral:didDiscoverCharacteristicsForService:error:))]
        fn did_discover_characteristics_for_service(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::DiscoveredCharacteristics {
                    service: service.retain(),
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheral:didWriteValueForCharacteristic:error:))]
        fn did_write_value_for_characteristic(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::CharacteristicValueWrite {
                    characteristic: characteristic.retain(),
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheral:didUpdateNotificationStateForCharacteristic:error:))]
        fn did_update_notification_state_for_characteristic(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::NotificationStateUpdate {
                    characteristic: characteristic.retain(),
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheral:didDiscoverDescriptorsForCharacteristic:error:))]
        fn did_discover_descriptors_for_characteristic(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &CBCharacteristic,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::DiscoveredDescriptors {
                    characteristic: characteristic.retain(),
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheral:didUpdateValueForDescriptor:error:))]
        fn did_update_value_for_descriptor(
            &self,
            peripheral: &CBPeripheral,
            descriptor: &CBDescriptor,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::DescriptorValueUpdate {
                    descriptor: descriptor.retain(),
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheral:didWriteValueForDescriptor:error:))]
        fn did_write_value_for_descriptor(
            &self,
            peripheral: &CBPeripheral,
            descriptor: &CBDescriptor,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::DescriptorValueWrite {
                    descriptor: descriptor.retain(),
                    error: error.map(|e| e.retain()),
                });
        }

        #[unsafe(method(peripheralIsReadyToSendWriteWithoutResponse:))]
        fn is_ready_to_write_without_response(&self, peripheral: &CBPeripheral) {
            let _ = self
                .ivars()
                .sender
                .try_broadcast(PeripheralEvent::ReadyToWrite);
        }

        #[unsafe(method(peripheral:didOpenL2CAPChannel:error:))]
        fn did_open_l2cap_channel(
            &self,
            peripheral: &CBPeripheral,
            channel: Option<&CBL2CAPChannel>,
            error: Option<&NSError>,
        ) {
            if let Some(channel) = channel {
                let _ = self
                    .ivars()
                    .sender
                    .try_broadcast(PeripheralEvent::L2CAPChannelOpened {
                        channel: channel.retain(),
                        error: error.map(|e| e.retain()),
                    });
            }
        }
    }
);

impl PeripheralDelegate {
    pub fn new() -> Retained<Self> {
        let (mut sender, receiver) = async_broadcast::broadcast::<PeripheralEvent>(16);
        sender.set_overflow(true);
        let receiver = receiver.deactivate();
        let ivars = PeripheralDelegateIvars {
            sender,
            _receiver: receiver,
        };
        let this = PeripheralDelegate::alloc().set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    pub fn sender(&self) -> async_broadcast::Sender<PeripheralEvent> {
        self.ivars().sender.clone()
    }
}

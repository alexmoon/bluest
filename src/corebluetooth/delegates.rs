use std::any::Any;

use corebluetooth::error::CBError;
use corebluetooth::{CBConnectionEvent, CBManagerState};
use dispatch_executor::{Executor, Handle};
use tracing::{debug, warn};

#[cfg(feature = "l2cap")]
use super::l2cap_channel::{L2capChannelReader, L2capChannelWriter};
use crate::ConnectionEvent;

pub fn subscribe_peripheral(
    delegate: &dyn corebluetooth::PeripheralDelegate,
) -> async_broadcast::Receiver<PeripheralEvent> {
    let delegate: &dyn Any = delegate;
    let delegate: &PeripheralDelegate = delegate.downcast_ref().unwrap();
    delegate.subscribe()
}

pub fn subscribe_central(
    delegate: &dyn corebluetooth::CentralManagerDelegate,
) -> async_broadcast::Receiver<CentralEvent> {
    let delegate: &dyn Any = delegate;
    let delegate: &CentralDelegate = delegate.downcast_ref().unwrap();
    delegate.subscribe()
}

#[derive(Clone)]
pub enum CentralEvent {
    Connect {
        peripheral: Handle<corebluetooth::Peripheral>,
    },
    Disconnect {
        peripheral: Handle<corebluetooth::Peripheral>,
        error: Option<corebluetooth::Error>,
    },
    ConnectFailed {
        peripheral: Handle<corebluetooth::Peripheral>,
        error: corebluetooth::Error,
    },
    ConnectionEvent {
        peripheral: Handle<corebluetooth::Peripheral>,
        event: ConnectionEvent,
    },
    Discovered {
        peripheral: Handle<corebluetooth::Peripheral>,
        advertisement_data: crate::AdvertisementData,
        rssi: i16,
    },
    StateChanged(CBManagerState),
}

impl std::fmt::Debug for CentralEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect { peripheral } => f.debug_struct("Connect").field("peripheral", peripheral).finish(),
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
            Self::Discovered { peripheral, rssi, .. } => f
                .debug_struct("Discovered")
                .field("peripheral", peripheral)
                .field("rssi", rssi)
                .finish(),
            Self::StateChanged(state) => f.debug_tuple("StateChanged").field(state).finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PeripheralEvent {
    Connected,
    Disconnected {
        error: corebluetooth::Error,
    },
    DiscoveredServices {
        result: corebluetooth::Result<()>,
    },
    DiscoveredIncludedServices {
        service: Handle<corebluetooth::Service>,
        result: corebluetooth::Result<()>,
    },
    DiscoveredCharacteristics {
        service: Handle<corebluetooth::Service>,
        result: corebluetooth::Result<()>,
    },
    DiscoveredDescriptors {
        characteristic: Handle<corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    },
    CharacteristicValueUpdate {
        characteristic: Handle<corebluetooth::Characteristic>,
        result: corebluetooth::Result<Vec<u8>>,
    },
    DescriptorValueUpdate {
        descriptor: Handle<corebluetooth::Descriptor>,
        result: corebluetooth::Result<()>,
    },
    CharacteristicValueWrite {
        characteristic: Handle<corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    },
    DescriptorValueWrite {
        descriptor: Handle<corebluetooth::Descriptor>,
        result: corebluetooth::Result<()>,
    },
    ReadyToWrite,
    NotificationStateUpdate {
        characteristic: Handle<corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    },
    ReadRssi {
        rssi: corebluetooth::Result<i16>,
    },
    NameUpdate,
    ServicesChanged {
        invalidated_services: Vec<Handle<corebluetooth::Service>>,
    },
    #[cfg(feature = "l2cap")]
    L2CAPChannelOpened {
        result: corebluetooth::Result<(L2capChannelReader, L2capChannelWriter)>,
    },
}

pub(crate) struct CentralDelegate {
    pub sender: async_broadcast::Sender<CentralEvent>,
    _receiver: async_broadcast::InactiveReceiver<CentralEvent>,
    executor: Executor,
}

impl corebluetooth::CentralManagerDelegate for CentralDelegate {
    fn new_peripheral_delegate(&self) -> Box<dyn corebluetooth::PeripheralDelegate> {
        Box::new(PeripheralDelegate::new(self.executor.clone()))
    }

    fn did_update_state(&self, central: corebluetooth::CentralManager) {
        let _ = self.sender.try_broadcast(CentralEvent::StateChanged(central.state()));
    }

    fn did_discover(
        &self,
        _central: corebluetooth::CentralManager,
        peripheral: corebluetooth::Peripheral,
        advertisement_data: corebluetooth::advertisement_data::AdvertisementData,
        rssi: i16,
    ) {
        let event = CentralEvent::Discovered {
            peripheral: self.executor.handle(peripheral),
            advertisement_data: advertisement_data.into(),
            rssi,
        };
        debug!("CentralDelegate received {:?}", event);
        let _res = self.sender.try_broadcast(event);
    }

    fn did_connect(&self, _central: corebluetooth::CentralManager, peripheral: corebluetooth::Peripheral) {
        let delegate: &dyn Any = peripheral.delegate();
        let delegate: &PeripheralDelegate = delegate.downcast_ref().unwrap();
        let _res = delegate.sender.try_broadcast(PeripheralEvent::Connected);

        let peripheral = self.executor.handle(peripheral);
        let event = CentralEvent::Connect { peripheral };
        debug!("CentralDelegate received {:?}", event);
        let _ = self.sender.try_broadcast(event);
    }

    fn did_fail_to_connect(
        &self,
        _central: corebluetooth::CentralManager,
        peripheral: corebluetooth::Peripheral,
        error: corebluetooth::Error,
    ) {
        let peripheral = self.executor.handle(peripheral);
        let _ = self
            .sender
            .try_broadcast(CentralEvent::ConnectFailed { peripheral, error });
    }

    fn did_disconnect(
        &self,
        _central: corebluetooth::CentralManager,
        peripheral: corebluetooth::Peripheral,
        _timestamp: Option<std::time::SystemTime>,
        _is_reconnecting: bool,
        error: Option<corebluetooth::Error>,
    ) {
        let delegate: &dyn Any = peripheral.delegate();
        let delegate: &PeripheralDelegate = delegate.downcast_ref().unwrap();
        let _res = delegate.sender.try_broadcast(PeripheralEvent::Disconnected {
            error: error.clone().unwrap_or_else(|| {
                corebluetooth::Error::from(corebluetooth::error::ErrorKind::Bluetooth(CBError::NotConnected))
            }),
        });

        let peripheral = self.executor.handle(peripheral);
        let event = CentralEvent::Disconnect { peripheral, error };
        debug!("CentralDelegate received {:?}", event);
        let _ = self.sender.try_broadcast(event);
    }

    fn on_connection_event(
        &self,
        _central: corebluetooth::CentralManager,
        event: CBConnectionEvent,
        peripheral: corebluetooth::Peripheral,
    ) {
        debug!("CentralDelegate received {:?}", event);

        let sender = &self.sender;
        let event = if event == CBConnectionEvent::PeerConnected {
            Some(ConnectionEvent::Connected)
        } else if event == CBConnectionEvent::PeerDisconnected {
            Some(ConnectionEvent::Disconnected)
        } else {
            None
        };

        if let Some(event) = event {
            let peripheral = self.executor.handle(peripheral);
            let event = CentralEvent::ConnectionEvent { peripheral, event };
            let _res = sender.try_broadcast(event);
        } else {
            warn!("Unrecognized connection event received");
        }
    }
}

impl CentralDelegate {
    pub fn new(executor: Executor) -> Self {
        let (mut sender, receiver) = async_broadcast::broadcast::<CentralEvent>(16);
        sender.set_overflow(true);
        let _receiver = receiver.deactivate();

        Self {
            sender,
            _receiver,
            executor,
        }
    }

    pub fn subscribe(&self) -> async_broadcast::Receiver<CentralEvent> {
        self.sender.new_receiver()
    }
}

pub(crate) struct PeripheralDelegate {
    pub sender: async_broadcast::Sender<PeripheralEvent>,
    _receiver: async_broadcast::InactiveReceiver<PeripheralEvent>,
    executor: Executor,
}

impl corebluetooth::PeripheralDelegate for PeripheralDelegate {
    fn did_update_name(&self, _peripheral: corebluetooth::Peripheral) {
        let _ = self.sender.try_broadcast(PeripheralEvent::NameUpdate);
    }

    fn did_modify_services(
        &self,
        _peripheral: corebluetooth::Peripheral,
        invalidated_services: Vec<corebluetooth::Service>,
    ) {
        let invalidated_services = invalidated_services
            .into_iter()
            .map(|service| self.executor.handle(service))
            .collect();

        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::ServicesChanged { invalidated_services });
    }

    fn did_read_rssi(&self, _peripheral: corebluetooth::Peripheral, rssi: corebluetooth::Result<i16>) {
        let _ = self.sender.try_broadcast(PeripheralEvent::ReadRssi { rssi });
    }

    fn did_discover_services(&self, _peripheral: corebluetooth::Peripheral, result: corebluetooth::Result<()>) {
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredServices { result });
    }

    fn did_discover_included_services(
        &self,
        _peripheral: corebluetooth::Peripheral,
        service: corebluetooth::Service,
        result: corebluetooth::Result<()>,
    ) {
        let service = self.executor.handle(service);
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredIncludedServices { service, result });
    }

    fn did_discover_characteristics(
        &self,
        _peripheral: corebluetooth::Peripheral,
        service: corebluetooth::Service,
        result: corebluetooth::Result<()>,
    ) {
        let service = self.executor.handle(service);
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredCharacteristics { service, result });
    }

    fn did_update_value_for_characteristic(
        &self,
        _peripheral: corebluetooth::Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: corebluetooth::Result<()>,
    ) {
        let result = result.map(|_| characteristic.value().unwrap());
        let characteristic = self.executor.handle(characteristic);
        let event = PeripheralEvent::CharacteristicValueUpdate { characteristic, result };
        debug!("PeripheralDelegate received {:?}", event);
        let _res = self.sender.try_broadcast(event);
    }

    fn did_write_value_for_characteristic(
        &self,
        _peripheral: corebluetooth::Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: corebluetooth::Result<()>,
    ) {
        let characteristic = self.executor.handle(characteristic);
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::CharacteristicValueWrite { characteristic, result });
    }

    fn did_update_notification_state_for_characteristic(
        &self,
        _peripheral: corebluetooth::Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: corebluetooth::Result<()>,
    ) {
        let characteristic = self.executor.handle(characteristic);
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::NotificationStateUpdate { characteristic, result });
    }

    fn did_discover_descriptors_for_characteristic(
        &self,
        _peripheral: corebluetooth::Peripheral,
        characteristic: corebluetooth::Characteristic,
        result: corebluetooth::Result<()>,
    ) {
        let characteristic = self.executor.handle(characteristic);
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredDescriptors { characteristic, result });
    }

    fn did_update_value_for_descriptor(
        &self,
        _peripheral: corebluetooth::Peripheral,
        descriptor: corebluetooth::Descriptor,
        result: corebluetooth::Result<()>,
    ) {
        let descriptor = self.executor.handle(descriptor);
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DescriptorValueUpdate { descriptor, result });
    }

    fn did_write_value_for_descriptor(
        &self,
        _peripheral: corebluetooth::Peripheral,
        descriptor: corebluetooth::Descriptor,
        result: corebluetooth::Result<()>,
    ) {
        let descriptor = self.executor.handle(descriptor);
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DescriptorValueWrite { descriptor, result });
    }

    fn is_ready_to_send_write_without_response(&self, _peripheral: corebluetooth::Peripheral) {
        let _ = self.sender.try_broadcast(PeripheralEvent::ReadyToWrite);
    }

    #[cfg(feature = "l2cap")]
    fn did_open_l2cap_channel(
        &self,
        _peripheral: corebluetooth::Peripheral,
        result: corebluetooth::Result<(
            corebluetooth::L2capChannel<corebluetooth::Peripheral>,
            std::os::unix::net::UnixStream,
        )>,
    ) {
        use std::sync::Arc;

        use async_io::Async;

        let result = match result {
            Ok((channel, stream)) => {
                let stream = Arc::new(Async::new(stream).unwrap());
                let reader = L2capChannelReader::new(self.executor.handle(channel.clone()), stream.clone());
                let writer = L2capChannelWriter::new(self.executor.handle(channel), stream);
                Ok((reader, writer))
            }
            Err(err) => Err(err),
        };

        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::L2CAPChannelOpened { result });
    }
}

impl PeripheralDelegate {
    pub fn new(executor: Executor) -> Self {
        let (mut sender, receiver) = async_broadcast::broadcast::<PeripheralEvent>(16);
        sender.set_overflow(true);
        let _receiver = receiver.deactivate();
        Self {
            sender,
            _receiver,
            executor,
        }
    }

    pub fn subscribe(&self) -> async_broadcast::Receiver<PeripheralEvent> {
        self.sender.new_receiver()
    }
}

use std::any::Any;
#[cfg(feature = "l2cap")]
use std::sync::Mutex;

use corebluetooth::error::CBError;
use corebluetooth::{CBConnectionEvent, CBManagerState};
use dispatch_executor::{DispatchCell, Ref};
use tracing::{debug, warn};
use uuid::Uuid;

#[cfg(feature = "l2cap")]
use super::l2cap_channel::L2capChannel;
use crate::ConnectionEvent;

pub(crate) fn peripheral_delegate<'a>(peripheral: &'a Ref<'_, corebluetooth::Peripheral>) -> &'a PeripheralDelegate {
    let delegate: &dyn Any = peripheral.delegate();
    delegate.downcast_ref::<PeripheralDelegate>().unwrap()
}

pub(crate) fn central_delegate<'a>(central: &'a Ref<'_, corebluetooth::CentralManager>) -> &'a CentralDelegate {
    let delegate: &dyn Any = central.delegate();
    delegate.downcast_ref::<CentralDelegate>().unwrap()
}

#[derive(Clone)]
pub enum CentralEvent {
    Connect {
        peripheral_id: Uuid,
    },
    Disconnect {
        peripheral_id: Uuid,
        error: Option<corebluetooth::Error>,
    },
    ConnectFailed {
        peripheral_id: Uuid,
        error: corebluetooth::Error,
    },
    #[cfg_attr(target_os = "macos", expect(unused))]
    ConnectionEvent {
        peripheral_id: Uuid,
        event: ConnectionEvent,
    },
    Discovered {
        peripheral: DispatchCell<corebluetooth::Peripheral>,
        advertisement_data: crate::AdvertisementData,
        rssi: i16,
    },
    StateChanged(CBManagerState),
}

#[derive(Clone)]
pub enum PeripheralEvent {
    Connected,
    Disconnected {
        error: corebluetooth::Error,
    },
    DiscoveredServices {
        result: corebluetooth::Result<()>,
    },
    DiscoveredIncludedServices {
        service: DispatchCell<corebluetooth::Service>,
        result: corebluetooth::Result<()>,
    },
    DiscoveredCharacteristics {
        service: DispatchCell<corebluetooth::Service>,
        result: corebluetooth::Result<()>,
    },
    DiscoveredDescriptors {
        characteristic: DispatchCell<corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    },
    CharacteristicValueUpdate {
        characteristic: DispatchCell<corebluetooth::Characteristic>,
        result: corebluetooth::Result<Vec<u8>>,
    },
    DescriptorValueUpdate {
        descriptor: DispatchCell<corebluetooth::Descriptor>,
        result: corebluetooth::Result<()>,
    },
    CharacteristicValueWrite {
        characteristic: DispatchCell<corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    },
    DescriptorValueWrite {
        descriptor: DispatchCell<corebluetooth::Descriptor>,
        result: corebluetooth::Result<()>,
    },
    ReadyToWrite,
    NotificationStateUpdate {
        characteristic: DispatchCell<corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    },
    ReadRssi {
        rssi: corebluetooth::Result<i16>,
    },
    NameUpdate,
    ServicesChanged {
        invalidated_services: Vec<DispatchCell<corebluetooth::Service>>,
    },
}

pub(crate) struct CentralDelegate {
    pub sender: async_broadcast::Sender<CentralEvent>,
    _receiver: async_broadcast::InactiveReceiver<CentralEvent>,
}

impl corebluetooth::CentralManagerDelegate for CentralDelegate {
    fn new_peripheral_delegate(&self) -> Box<dyn corebluetooth::PeripheralDelegate> {
        Box::new(PeripheralDelegate::new())
    }

    fn did_update_state(&self, central: Ref<'_, corebluetooth::CentralManager>) {
        let _ = self.sender.try_broadcast(CentralEvent::StateChanged(central.state()));
    }

    fn did_discover(
        &self,
        _central: Ref<'_, corebluetooth::CentralManager>,
        peripheral: Ref<'_, corebluetooth::Peripheral>,
        advertisement_data: corebluetooth::advertisement_data::AdvertisementData,
        rssi: i16,
    ) {
        debug!(
            "CentralDelegate received Discovered {{ peripheral: {:?}, rssi: {:?} }}",
            peripheral, rssi
        );
        let event = CentralEvent::Discovered {
            peripheral: peripheral.retain(),
            advertisement_data: advertisement_data.into(),
            rssi,
        };
        let _res = self.sender.try_broadcast(event);
    }

    fn did_connect(
        &self,
        _central: Ref<'_, corebluetooth::CentralManager>,
        peripheral: Ref<'_, corebluetooth::Peripheral>,
    ) {
        let delegate = peripheral_delegate(&peripheral);
        let _res = delegate.sender.try_broadcast(PeripheralEvent::Connected);

        debug!("CentralDelegate received Connect {{ peripheral: {:?} }}", peripheral);
        let event = CentralEvent::Connect {
            peripheral_id: peripheral.identifier(),
        };
        let _ = self.sender.try_broadcast(event);
    }

    fn did_fail_to_connect(
        &self,
        _central: Ref<'_, corebluetooth::CentralManager>,
        peripheral: Ref<'_, corebluetooth::Peripheral>,
        error: corebluetooth::Error,
    ) {
        let _ = self.sender.try_broadcast(CentralEvent::ConnectFailed {
            peripheral_id: peripheral.identifier(),
            error,
        });
    }

    fn did_disconnect(
        &self,
        _central: Ref<'_, corebluetooth::CentralManager>,
        peripheral: Ref<'_, corebluetooth::Peripheral>,
        _timestamp: Option<std::time::SystemTime>,
        _is_reconnecting: bool,
        error: Option<corebluetooth::Error>,
    ) {
        let delegate = peripheral_delegate(&peripheral);
        let disconnect_error = error.clone().unwrap_or_else(|| {
            corebluetooth::Error::from(corebluetooth::error::ErrorKind::Bluetooth(CBError::NotConnected))
        });
        let _res = delegate.sender.try_broadcast(PeripheralEvent::Disconnected {
            error: disconnect_error.clone(),
        });
        #[cfg(feature = "l2cap")]
        if let Some(sender) = delegate.pending_l2cap_connection.lock().unwrap().take() {
            let _res = sender.send(Err(crate::Error::from_kind_and_error(
                crate::error::ErrorKind::NotConnected,
                disconnect_error,
            )));
        }

        debug!(
            "CentralDelegate received Disconnect {{ peripheral: {:?}, error: {:?} }}",
            peripheral, error
        );
        let event = CentralEvent::Disconnect {
            peripheral_id: peripheral.identifier(),
            error,
        };
        let _ = self.sender.try_broadcast(event);
    }

    fn on_connection_event(
        &self,
        _central: Ref<'_, corebluetooth::CentralManager>,
        event: CBConnectionEvent,
        peripheral: Ref<'_, corebluetooth::Peripheral>,
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
            let peripheral_id = peripheral.identifier();
            let event = CentralEvent::ConnectionEvent { peripheral_id, event };
            let _res = sender.try_broadcast(event);
        } else {
            warn!("Unrecognized connection event received");
        }
    }
}

impl CentralDelegate {
    pub fn new() -> Self {
        let (mut sender, receiver) = async_broadcast::broadcast::<CentralEvent>(16);
        sender.set_overflow(true);
        let _receiver = receiver.deactivate();

        Self { sender, _receiver }
    }

    pub fn subscribe(&self) -> async_broadcast::Receiver<CentralEvent> {
        self.sender.new_receiver()
    }
}

pub(crate) struct PeripheralDelegate {
    pub sender: async_broadcast::Sender<PeripheralEvent>,
    _receiver: async_broadcast::InactiveReceiver<PeripheralEvent>,
    #[cfg(feature = "l2cap")]
    pub(crate) pending_l2cap_connection: Mutex<Option<oneshot::Sender<crate::Result<L2capChannel>>>>,
}

impl corebluetooth::PeripheralDelegate for PeripheralDelegate {
    fn did_update_name(&self, _peripheral: Ref<'_, corebluetooth::Peripheral>) {
        let _ = self.sender.try_broadcast(PeripheralEvent::NameUpdate);
    }

    fn did_modify_services(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        invalidated_services: Vec<DispatchCell<corebluetooth::Service>>,
    ) {
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::ServicesChanged { invalidated_services });
    }

    fn did_read_rssi(&self, _peripheral: Ref<'_, corebluetooth::Peripheral>, rssi: corebluetooth::Result<i16>) {
        let _ = self.sender.try_broadcast(PeripheralEvent::ReadRssi { rssi });
    }

    fn did_discover_services(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        result: corebluetooth::Result<()>,
    ) {
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredServices { result });
    }

    fn did_discover_included_services(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        service: Ref<'_, corebluetooth::Service>,
        result: corebluetooth::Result<()>,
    ) {
        let service = service.retain();
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredIncludedServices { service, result });
    }

    fn did_discover_characteristics(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        service: Ref<'_, corebluetooth::Service>,
        result: corebluetooth::Result<()>,
    ) {
        let service = service.retain();
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredCharacteristics { service, result });
    }

    fn did_update_value_for_characteristic(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        characteristic: Ref<'_, corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    ) {
        let result = result.map(|_| characteristic.value().unwrap());
        let characteristic = characteristic.retain();
        let event = PeripheralEvent::CharacteristicValueUpdate { characteristic, result };
        debug!("PeripheralDelegate received CharacteristicValueUpdate");
        let _res = self.sender.try_broadcast(event);
    }

    fn did_write_value_for_characteristic(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        characteristic: Ref<'_, corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    ) {
        let characteristic = characteristic.retain();
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::CharacteristicValueWrite { characteristic, result });
    }

    fn did_update_notification_state_for_characteristic(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        characteristic: Ref<'_, corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    ) {
        let characteristic = characteristic.retain();
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::NotificationStateUpdate { characteristic, result });
    }

    fn did_discover_descriptors_for_characteristic(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        characteristic: Ref<'_, corebluetooth::Characteristic>,
        result: corebluetooth::Result<()>,
    ) {
        let characteristic = characteristic.retain();
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DiscoveredDescriptors { characteristic, result });
    }

    fn did_update_value_for_descriptor(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        descriptor: Ref<'_, corebluetooth::Descriptor>,
        result: corebluetooth::Result<()>,
    ) {
        let descriptor = descriptor.retain();
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DescriptorValueUpdate { descriptor, result });
    }

    fn did_write_value_for_descriptor(
        &self,
        _peripheral: Ref<'_, corebluetooth::Peripheral>,
        descriptor: Ref<'_, corebluetooth::Descriptor>,
        result: corebluetooth::Result<()>,
    ) {
        let descriptor = descriptor.retain();
        let _ = self
            .sender
            .try_broadcast(PeripheralEvent::DescriptorValueWrite { descriptor, result });
    }

    fn is_ready_to_send_write_without_response(&self, _peripheral: Ref<'_, corebluetooth::Peripheral>) {
        let _ = self.sender.try_broadcast(PeripheralEvent::ReadyToWrite);
    }

    #[cfg(feature = "l2cap")]
    fn did_open_l2cap_channel(
        &self,
        peripheral: Ref<'_, corebluetooth::Peripheral>,
        result: corebluetooth::Result<(
            DispatchCell<corebluetooth::L2capChannel<corebluetooth::Peripheral>>,
            std::os::unix::net::UnixStream,
        )>,
    ) {
        use std::sync::Arc;

        use async_io::Async;

        if let Some(channel) = self.pending_l2cap_connection.lock().unwrap().take() {
            let result = result
                .map_err(crate::Error::from)
                .and_then(|(channel, stream)| Ok((channel, Async::new(stream)?)))
                .map(|(channel, stream)| L2capChannel::new(channel.borrow(peripheral.scope()), Arc::new(stream)));
            let _ = channel.send(result);
        }
    }
}

impl PeripheralDelegate {
    pub fn new() -> Self {
        let (mut sender, receiver) = async_broadcast::broadcast::<PeripheralEvent>(16);
        sender.set_overflow(true);
        let _receiver = receiver.deactivate();
        Self {
            sender,
            _receiver,
            #[cfg(feature = "l2cap")]
            pending_l2cap_connection: Mutex::new(None),
        }
    }

    pub fn subscribe(&self) -> async_broadcast::Receiver<PeripheralEvent> {
        self.sender.new_receiver()
    }
}

#![allow(clippy::let_unit_value)]

use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Protocol, Sel},
    sel, sel_impl,
};
use objc_foundation::{INSArray, NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, ShareId, Shared};
use std::{os::raw::c_void, sync::Once};
use tracing::{debug, error};

use super::types::{id, CBCharacteristic, CBDescriptor, CBL2CAPChannel, CBPeripheral, CBService, NSError, NSInteger};

#[derive(Clone)]
pub enum CentralEvent {
    Connect {
        peripheral: ShareId<CBPeripheral>,
    },
    Disconnect {
        peripheral: ShareId<CBPeripheral>,
        error: Option<ShareId<NSError>>,
    },
    ConnectFailed {
        peripheral: ShareId<CBPeripheral>,
        error: Option<ShareId<NSError>>,
    },
    ConnectionEvent {
        peripheral: ShareId<CBPeripheral>,
        event: CBConnectionEvent,
    },
    Discovered {
        peripheral: ShareId<CBPeripheral>,
        adv_data: ShareId<NSDictionary<NSString, NSObject>>,
        rssi: i16,
    },
    StateChanged,
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
            Self::StateChanged => write!(f, "StateChanged"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PeripheralEvent {
    DiscoveredServices {
        error: Option<ShareId<NSError>>,
    },
    DiscoveredIncludedServices {
        service: ShareId<CBService>,
        error: Option<ShareId<NSError>>,
    },
    DiscoveredCharacteristics {
        service: ShareId<CBService>,
        error: Option<ShareId<NSError>>,
    },
    DiscoveredDescriptors {
        characteristic: ShareId<CBCharacteristic>,
        error: Option<ShareId<NSError>>,
    },
    CharacteristicValueUpdate {
        characteristic: ShareId<CBCharacteristic>,
        error: Option<ShareId<NSError>>,
    },
    DescriptorValueUpdate {
        descriptor: ShareId<CBDescriptor>,
        error: Option<ShareId<NSError>>,
    },
    CharacteristicValueWrite {
        characteristic: ShareId<CBCharacteristic>,
        error: Option<ShareId<NSError>>,
    },
    DescriptorValueWrite {
        descriptor: ShareId<CBDescriptor>,
        error: Option<ShareId<NSError>>,
    },
    ReadyToWrite,
    NotificationStateUpdate {
        characteristic: ShareId<CBCharacteristic>,
        error: Option<ShareId<NSError>>,
    },
    ReadRssi {
        rssi: i16,
        error: Option<ShareId<NSError>>,
    },
    NameUpdate,
    ServicesChanged {
        invalidated_services: Vec<ShareId<CBService>>,
    },
    L2CAPChannelOpened {
        channel: ShareId<CBL2CAPChannel>,
        error: Option<ShareId<NSError>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBConnectionEvent {
    Disconnected,
    Connected,
}

impl TryFrom<NSInteger> for CBConnectionEvent {
    type Error = NSInteger;

    fn try_from(value: NSInteger) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(CBConnectionEvent::Disconnected),
            1 => Ok(CBConnectionEvent::Connected),
            _ => Err(value),
        }
    }
}

macro_rules! delegate_method {
    (@value $param:ident: Object) => {
        ShareId::from_ptr($param as *mut _)
    };
    (@value $param:ident: Option) => {
        (!$param.is_null()).then(|| ShareId::from_ptr($param as *mut _))
    };
    (@value $param:ident: i16) => {
        {
            let n: i16 = msg_send![$param, shortValue];
            n
        }
    };
    (@value $param:ident: Vec) => {
        ShareId::from_ptr($param as *mut NSArray<_, Shared>).to_shared_vec()
    };
    ($name:ident < $event:ident > ( central $(, $param:ident: $ty:ident)*)) => {
        extern "C" fn $name(this: &mut Object, _sel: Sel, _central: id, $($param: id),*) {
            unsafe {
                let ptr = *this.get_ivar::<*mut c_void>("sender") as *mut tokio::sync::broadcast::Sender<CentralEvent>;
                if !ptr.is_null() {
                    let event = CentralEvent::$event {
                        $($param: delegate_method!(@value $param: $ty)),*
                    };
                    debug!("CentralDelegate received {:?}", event);
                    let _ = (*ptr).send(event);
                }
            }
        }
    };

    ($name:ident < $event:ident > ( peripheral $(, $param:ident: $ty:ident)*)) => {
        extern "C" fn $name(this: &mut Object, _sel: Sel, _peripheral: id, $($param: id),*) {
            unsafe {
                let ptr = *this.get_ivar::<*mut c_void>("sender") as *mut tokio::sync::broadcast::Sender<PeripheralEvent>;
                if !ptr.is_null() {
                    let event = PeripheralEvent::$event {
                        $($param: delegate_method!(@value $param: $ty)),*
                    };
                    debug!("PeripheralDelegate received {:?}", event);
                    let _ = (*ptr).send(event);
                }
            }
        }
    };
}

pub struct CentralDelegate {
    _private: (),
}
unsafe impl objc::Message for CentralDelegate {}

impl objc_foundation::INSObject for CentralDelegate {
    fn class() -> &'static ::objc::runtime::Class {
        CentralDelegate::class()
    }
}
impl PartialEq for CentralDelegate {
    fn eq(&self, other: &Self) -> bool {
        use objc_foundation::INSObject;
        self.is_equal(other)
    }
}
impl Eq for CentralDelegate {}

impl std::hash::Hash for CentralDelegate {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        use objc_foundation::INSObject;
        self.hash_code().hash(state);
    }
}
impl ::std::fmt::Debug for CentralDelegate {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        use objc_foundation::{INSObject, INSString};
        ::std::fmt::Debug::fmt(self.description().as_str(), f)
    }
}

pub struct PeripheralDelegate {
    _private: (),
}
unsafe impl objc::Message for PeripheralDelegate {}

impl objc_foundation::INSObject for PeripheralDelegate {
    fn class() -> &'static ::objc::runtime::Class {
        PeripheralDelegate::class()
    }
}
impl PartialEq for PeripheralDelegate {
    fn eq(&self, other: &Self) -> bool {
        use objc_foundation::INSObject;
        self.is_equal(other)
    }
}
impl Eq for PeripheralDelegate {}

impl std::hash::Hash for PeripheralDelegate {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        use objc_foundation::INSObject;
        self.hash_code().hash(state);
    }
}
impl ::std::fmt::Debug for PeripheralDelegate {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        use objc_foundation::{INSObject, INSString};
        ::std::fmt::Debug::fmt(self.description().as_str(), f)
    }
}

impl CentralDelegate {
    pub fn with_sender(sender: tokio::sync::broadcast::Sender<CentralEvent>) -> Option<Id<CentralDelegate>> {
        unsafe {
            let obj: *mut Self = msg_send![Self::class(), alloc];
            let obj: *mut Self = msg_send![obj, initWithSender: Box::into_raw(Box::new(sender)) as *mut c_void];
            (!obj.is_null()).then(|| Id::from_retained_ptr(obj))
        }
    }

    extern "C" fn init(this: &mut Object, _sel: Sel, sender: *mut c_void) -> id {
        unsafe { this.set_ivar("sender", sender) };
        this
    }

    extern "C" fn dealloc(this: &mut Object, _sel: Sel) {
        unsafe {
            let sender: *mut c_void = *this.get_ivar("sender");
            this.set_ivar("sender", std::ptr::null_mut::<c_void>());
            if !sender.is_null() {
                let _ = Box::from_raw(sender as *mut tokio::sync::broadcast::Sender<CentralEvent>);
            }
        };
    }

    delegate_method!(did_connect<Connect>(central, peripheral: Object));
    delegate_method!(did_disconnect<Disconnect>(central, peripheral: Object, error: Option));
    delegate_method!(did_fail_to_connect<ConnectFailed>(central, peripheral: Object, error: Option));
    delegate_method!(did_update_state<StateChanged>(central));

    extern "C" fn did_discover_peripheral(
        this: &mut Object,
        _sel: Sel,
        _central: id,
        peripheral: id,
        adv_data: id,
        rssi: id,
    ) {
        unsafe {
            let ptr = *this.get_ivar::<*mut c_void>("sender") as *mut tokio::sync::broadcast::Sender<CentralEvent>;
            if !ptr.is_null() {
                let rssi: i16 = msg_send![rssi, charValue];
                let _ = (*ptr).send(CentralEvent::Discovered {
                    peripheral: ShareId::from_ptr(peripheral as *mut _),
                    adv_data: ShareId::from_ptr(adv_data as *mut _),
                    rssi,
                });
            }
        }
    }

    extern "C" fn on_connection_event(
        this: &mut Object,
        _sel: Sel,
        _central: id,
        connection_event: NSInteger,
        peripheral: id,
    ) {
        unsafe {
            let ptr = *this.get_ivar::<*mut c_void>("sender") as *mut tokio::sync::broadcast::Sender<CentralEvent>;
            if !ptr.is_null() {
                match connection_event.try_into() {
                    Ok(event) => {
                        let _ = (*ptr).send(CentralEvent::ConnectionEvent {
                            peripheral: ShareId::from_ptr(peripheral as _),
                            event,
                        });
                    }
                    Err(err) => {
                        error!("Invalid value for CBConnectionEvent: {}", err);
                    }
                }
            }
        }
    }

    fn class() -> &'static Class {
        static DELEGATE_CLASS_INIT: Once = Once::new();
        DELEGATE_CLASS_INIT.call_once(|| {
            let mut cls = ClassDecl::new("BluestCentralDelegate", class!(NSObject)).unwrap();
            cls.add_ivar::<*mut c_void>("sender");
            cls.add_protocol(Protocol::get("CBCentralManagerDelegate").unwrap());

            unsafe {
                // Initialization
                cls.add_method(
                    sel!(initWithSender:),
                    Self::init as extern "C" fn(&mut Object, Sel, *mut c_void) -> id,
                );

                // Cleanup
                cls.add_method(sel!(dealloc), Self::dealloc as extern "C" fn(&mut Object, Sel));

                // CBCentralManagerDelegate
                // Monitoring Connections with Peripherals
                cls.add_method(
                    sel!(centralManager:didConnectPeripheral:),
                    Self::did_connect as extern "C" fn(&mut Object, Sel, id, id),
                );
                cls.add_method(
                    sel!(centralManager:didDisconnectPeripheral:error:),
                    Self::did_disconnect as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                cls.add_method(
                    sel!(centralManager:didFailToConnectPeripheral:error:),
                    Self::did_fail_to_connect as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                cls.add_method(
                    sel!(centralManager:connectionEventDidOccur:forPeripheral:),
                    Self::on_connection_event as extern "C" fn(&mut Object, Sel, id, NSInteger, id),
                );
                // Discovering and Retrieving Peripherals
                cls.add_method(
                    sel!(centralManager:didDiscoverPeripheral:advertisementData:RSSI:),
                    Self::did_discover_peripheral as extern "C" fn(&mut Object, Sel, id, id, id, id),
                );
                // Monitoring the Central Manager's State
                cls.add_method(
                    sel!(centralManagerDidUpdateState:),
                    Self::did_update_state as extern "C" fn(&mut Object, Sel, id),
                );
            }

            cls.register();
        });

        class!(BluestCentralDelegate)
    }
}

impl PeripheralDelegate {
    pub fn with_sender(sender: tokio::sync::broadcast::Sender<PeripheralEvent>) -> Id<PeripheralDelegate> {
        unsafe {
            let obj: *mut Self = msg_send![Self::class(), alloc];
            let obj: *mut Self = msg_send![obj, initWithSender: Box::into_raw(Box::new(sender)) as *mut c_void];
            Id::from_retained_ptr(obj)
        }
    }

    pub fn sender(&self) -> Option<&tokio::sync::broadcast::Sender<PeripheralEvent>> {
        unsafe {
            let sender: *const c_void = msg_send![self, sender];
            (!sender.is_null()).then(|| &*(sender as *const tokio::sync::broadcast::Sender<PeripheralEvent>))
        }
    }

    extern "C" fn init(this: &mut Object, _sel: Sel, sender: *mut c_void) -> id {
        unsafe { this.set_ivar("sender", sender) };
        this
    }

    extern "C" fn dealloc(this: &mut Object, _sel: Sel) {
        unsafe {
            let sender: *mut c_void = *this.get_ivar("sender");
            this.set_ivar("sender", std::ptr::null_mut::<c_void>());
            if !sender.is_null() {
                let _ = Box::from_raw(sender as *mut tokio::sync::broadcast::Sender<PeripheralEvent>);
            }
        };
    }

    extern "C" fn sender_getter(this: &mut Object, _sel: Sel) -> *const c_void {
        unsafe { *this.get_ivar("sender") }
    }

    delegate_method!(did_discover_services<DiscoveredServices>(peripheral, error: Option));
    delegate_method!(did_discover_included_services<DiscoveredIncludedServices>(peripheral, service: Object, error: Option));
    delegate_method!(did_discover_characteristics<DiscoveredCharacteristics>(peripheral, service: Object, error: Option));
    delegate_method!(did_discover_descriptors<DiscoveredDescriptors>(peripheral, characteristic: Object, error: Option));
    delegate_method!(did_update_value_for_characteristic<CharacteristicValueUpdate>(peripheral, characteristic: Object, error: Option));
    delegate_method!(did_update_value_for_descriptor<DescriptorValueUpdate>(peripheral, descriptor: Object, error: Option));
    delegate_method!(did_write_value_for_characteristic<CharacteristicValueWrite>(peripheral, characteristic: Object, error: Option));
    delegate_method!(did_write_value_for_descriptor<DescriptorValueWrite>(peripheral, descriptor: Object, error: Option));
    delegate_method!(is_ready_to_write_without_response<ReadyToWrite>(peripheral));
    delegate_method!(did_update_notification_state<NotificationStateUpdate>(peripheral, characteristic: Object, error: Option));
    delegate_method!(did_read_rssi<ReadRssi>(peripheral, rssi: i16, error: Option));
    delegate_method!(did_update_name<NameUpdate>(peripheral));
    delegate_method!(did_modify_services<ServicesChanged>(peripheral, invalidated_services: Vec));
    delegate_method!(did_open_l2cap_channel<L2CAPChannelOpened>(peripheral, channel: Object, error: Option));

    fn class() -> &'static Class {
        static DELEGATE_CLASS_INIT: Once = Once::new();
        DELEGATE_CLASS_INIT.call_once(|| {
            let mut cls = ClassDecl::new("BluestPeripheralDelegate", class!(NSObject)).unwrap();
            cls.add_ivar::<*mut c_void>("sender");
            cls.add_protocol(Protocol::get("CBPeripheralDelegate").unwrap());

            unsafe {
                // Initialization
                cls.add_method(
                    sel!(initWithSender:),
                    Self::init as extern "C" fn(&mut Object, Sel, *mut c_void) -> id,
                );

                // Cleanup
                cls.add_method(sel!(dealloc), Self::dealloc as extern "C" fn(&mut Object, Sel));

                // Sender property
                cls.add_method(
                    sel!(sender),
                    Self::sender_getter as extern "C" fn(&mut Object, Sel) -> *const c_void,
                );

                // CBPeripheralDelegate
                // Discovering Services
                cls.add_method(
                    sel!(peripheral:didDiscoverServices:),
                    Self::did_discover_services as extern "C" fn(&mut Object, Sel, id, id),
                );
                cls.add_method(
                    sel!(peripheral:didDiscoverIncludedServicesForService:error:),
                    Self::did_discover_included_services as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                // Discovering Characteristics and their Descriptors
                cls.add_method(
                    sel!(peripheral:didDiscoverCharacteristicsForService:error:),
                    Self::did_discover_characteristics as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                cls.add_method(
                    sel!(peripheral:didDiscoverDescriptorsForCharacteristic:error:),
                    Self::did_discover_descriptors as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                // Retrieving Characteristic and Descriptor Values
                cls.add_method(
                    sel!(peripheral:didUpdateValueForCharacteristic:error:),
                    Self::did_update_value_for_characteristic as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                cls.add_method(
                    sel!(peripheral:didUpdateValueForDescriptor:error:),
                    Self::did_update_value_for_descriptor as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                // Writing Characteristic and Descriptor Values
                cls.add_method(
                    sel!(peripheral:didWriteValueForCharacteristic:error:),
                    Self::did_write_value_for_characteristic as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                cls.add_method(
                    sel!(peripheral:didWriteValueForDescriptor:error:),
                    Self::did_write_value_for_descriptor as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                cls.add_method(
                    sel!(peripheralIsReadyToSendWriteWithoutResponse:),
                    Self::is_ready_to_write_without_response as extern "C" fn(&mut Object, Sel, id),
                );
                // Managing Notifications for a Characteristic's Value
                cls.add_method(
                    sel!(peripheral:didUpdateNotificationStateForCharacteristic:error:),
                    Self::did_update_notification_state as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                // Retrieving a Perpipheral's RSSI Data
                cls.add_method(
                    sel!(peripheral:didReadRSSI:error:),
                    Self::did_read_rssi as extern "C" fn(&mut Object, Sel, id, id, id),
                );
                // Monitoring Changes to a Peripheral's Name or Services
                cls.add_method(
                    sel!(peripheralDidUpdateName:),
                    Self::did_update_name as extern "C" fn(&mut Object, Sel, id),
                );
                cls.add_method(
                    sel!(peripheral:didModifyServices:),
                    Self::did_modify_services as extern "C" fn(&mut Object, Sel, id, id),
                );
                // Monitoring L2CAP Channels
                cls.add_method(
                    sel!(peripheral:didOpenL2CAPChannel:error:),
                    Self::did_open_l2cap_channel as extern "C" fn(&mut Object, Sel, id, id, id),
                );
            }

            cls.register();
        });

        class!(BluestPeripheralDelegate)
    }
}

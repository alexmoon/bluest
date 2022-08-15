#![allow(clippy::let_unit_value)]

use std::os::raw::{c_char, c_void};

use num_enum::FromPrimitive;
use objc::{
    msg_send,
    runtime::{Object, BOOL, NO},
    sel, sel_impl,
};
use objc_foundation::{
    object_struct, INSData, INSObject, INSString, NSArray, NSData, NSDictionary, NSObject, NSString,
};
use objc_id::{Id, ShareId};
use uuid::Uuid;

use super::delegates::{CentralDelegate, PeripheralDelegate};

use crate::{btuuid::BluetoothUuidExt, error::ErrorKind};

#[allow(non_camel_case_types)]
pub type id = *mut Object;

pub type NSInteger = isize;
pub type NSUInteger = usize;

#[allow(non_upper_case_globals)]
pub const nil: *mut Object = std::ptr::null_mut();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBManagerState {
    Unknown = 0,
    Resetting = 1,
    Unsupported = 2,
    Unauthorized = 3,
    PoweredOff = 4,
    PoweredOn = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBCharacteristicWriteType {
    WithResponse = 0,
    WithoutResponse = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBPeripheralState {
    Disconnected = 0,
    Connecting = 1,
    Connected = 2,
    Disconnecting = 3,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, FromPrimitive)]
#[repr(isize)]
pub enum CBError {
    #[default]
    Unknown = 0,
    InvalidParameters = 1,
    InvalidHandle = 2,
    NotConnected = 3,
    OutOfSpace = 4,
    OperationCancelled = 5,
    ConnectionTimeout = 6,
    PeripheralDisconnected = 7,
    UuidNotAllowed = 8,
    AlreadyAdvertising = 9,
    ConnectionFailed = 10,
    ConnectionLimitReached = 11,
    UnkownDevice = 12,
    OperationNotSupported = 13,
    PeerRemovedPairingInformation = 14,
    EncryptionTimedOut = 15,
    TooManyLEPairedDevices = 16,
}

impl From<CBError> for ErrorKind {
    fn from(err: CBError) -> Self {
        match err {
            CBError::Unknown => ErrorKind::Unknown,
            CBError::InvalidParameters => ErrorKind::InvalidParameters,
            CBError::InvalidHandle => ErrorKind::InvalidHandle,
            CBError::NotConnected => ErrorKind::NotConnected,
            CBError::OutOfSpace => ErrorKind::OutOfSpace,
            CBError::OperationCancelled => ErrorKind::OperationCancelled,
            CBError::ConnectionTimeout => ErrorKind::ConnectionTimeout,
            CBError::PeripheralDisconnected => ErrorKind::PeripheralDisconnected,
            CBError::UuidNotAllowed => ErrorKind::UuidNotAllowed,
            CBError::AlreadyAdvertising => ErrorKind::AlreadyAdvertising,
            CBError::ConnectionFailed => ErrorKind::ConnectionFailed,
            CBError::ConnectionLimitReached => ErrorKind::ConnectionLimitReached,
            CBError::UnkownDevice => ErrorKind::UnkownDevice,
            CBError::OperationNotSupported => ErrorKind::OperationNotSupported,
            CBError::PeerRemovedPairingInformation => ErrorKind::PeerRemovedPairingInformation,
            CBError::EncryptionTimedOut => ErrorKind::EncryptionTimedOut,
            CBError::TooManyLEPairedDevices => ErrorKind::TooManyLEPairedDevices,
        }
    }
}

#[link(name = "CoreBluetooth", kind = "framework")]
extern "C" {
    pub fn dispatch_queue_create(label: *const c_char, attr: id) -> id;
    pub fn dispatch_release(object: id) -> c_void;
}

pub fn id_or_nil<T, O>(val: &Option<Id<T, O>>) -> *const T {
    match val {
        Some(x) => &**x,
        None => std::ptr::null(),
    }
}

object_struct!(NSError);
object_struct!(NSUUID);
object_struct!(CBUUID);
object_struct!(CBCentralManager);
object_struct!(CBPeripheral);
object_struct!(CBService);
object_struct!(CBCharacteristic);
object_struct!(CBDescriptor);
object_struct!(CBL2CAPChannel);

impl From<&NSError> for crate::Error {
    fn from(err: &NSError) -> Self {
        let code: NSInteger = unsafe { msg_send![err, code] };
        let domain: *mut NSString = unsafe { msg_send![err, domain] };
        let domain = unsafe { ShareId::from_ptr(domain) };
        let description: *mut NSString = unsafe { msg_send![err, localizedDescription] };
        let description = unsafe { ShareId::from_ptr(description) };

        let code = if domain.as_str() == "CBErrorDomain" {
            CBError::from(code)
        } else {
            CBError::Unknown
        };

        crate::Error {
            kind: ErrorKind::from(code),
            message: description.as_str().to_string(),
        }
    }
}

impl NSUUID {
    pub fn to_uuid(&self) -> Uuid {
        let mut bytes = [0u8; 16];
        let _: () = unsafe { msg_send!(self, getUUIDBytes: &mut bytes) };
        Uuid::from_bytes(bytes)
    }
}

impl CBUUID {
    pub fn from_uuid(uuid: &Uuid) -> Id<Self> {
        unsafe {
            let obj: *mut Self =
                msg_send![Self::class(), UUIDWithData: NSData::from_vec(uuid.as_bluetooth_bytes().to_owned())];
            Id::from_retained_ptr(obj)
        }
    }

    pub fn to_uuid(&self) -> Uuid {
        let data: *mut NSData = unsafe { msg_send!(self, data) };
        let data = unsafe { ShareId::from_ptr(data) };
        Uuid::from_bluetooth_bytes(data.bytes())
    }
}

impl CBCentralManager {
    pub fn with_delegate(delegate: Id<CentralDelegate>, queue: id) -> Id<CBCentralManager> {
        unsafe {
            let obj: *mut Self = msg_send![Self::class(), alloc];
            let obj: *mut Self = msg_send![obj, initWithDelegate: delegate queue: queue];
            Id::from_retained_ptr(obj)
        }
    }

    pub fn state(&self) -> CBManagerState {
        unsafe {
            let state: NSInteger = msg_send![self, state];
            match state {
                0 => CBManagerState::Unknown,
                1 => CBManagerState::Resetting,
                2 => CBManagerState::Unsupported,
                3 => CBManagerState::Unauthorized,
                4 => CBManagerState::PoweredOff,
                5 => CBManagerState::PoweredOn,
                _ => CBManagerState::Unknown,
            }
        }
    }

    pub fn connect_peripheral(&self, peripheral: &CBPeripheral, options: Option<Id<NSDictionary<NSString, NSObject>>>) {
        unsafe { msg_send![self, connectPeripheral: peripheral options: id_or_nil(&options)] }
    }

    pub fn cancel_peripheral_connection(&self, peripheral: &CBPeripheral) {
        unsafe { msg_send![self, cancelPeripheralConnection: peripheral] }
    }
}

impl CBPeripheral {
    pub fn identifier(&self) -> ShareId<NSUUID> {
        unsafe {
            let id: *mut NSUUID = msg_send![self, identifier];
            ShareId::from_ptr(id)
        }
    }

    pub fn name(&self) -> Option<ShareId<NSString>> {
        unsafe {
            let name: *mut NSString = msg_send![self, name];
            (!name.is_null()).then(|| ShareId::from_ptr(name))
        }
    }

    pub fn delegate(&self) -> Option<ShareId<PeripheralDelegate>> {
        unsafe {
            let delegate: *mut PeripheralDelegate = msg_send![self, delegate];
            (!delegate.is_null()).then(|| ShareId::from_ptr(delegate))
        }
    }

    pub fn set_delegate(&self, delegate: Id<PeripheralDelegate>) {
        unsafe { msg_send![self, setDelegate: delegate] }
    }

    pub fn services(&self) -> Option<ShareId<NSArray<CBService>>> {
        unsafe {
            let services: *mut NSArray<CBService> = msg_send![self, services];
            (!services.is_null()).then(|| ShareId::from_ptr(services))
        }
    }
    pub fn discover_services(&self, services: Option<Id<NSArray<CBUUID>>>) {
        unsafe { msg_send![self, discoverServices: id_or_nil(&services)] }
    }

    pub fn discover_included_services(&self, service: &CBService, services: Option<Id<NSArray<CBUUID>>>) {
        unsafe { msg_send![self, discoverIncludedServices: id_or_nil(&services) forService: service] }
    }

    pub fn discover_characteristics(&self, service: &CBService, characteristics: Option<Id<NSArray<CBUUID>>>) {
        unsafe { msg_send![self, discoverCharacteristics: id_or_nil(&characteristics) forService: service] }
    }

    pub fn discover_descriptors(&self, characteristic: &CBCharacteristic, descriptors: Option<Id<NSArray<CBUUID>>>) {
        unsafe { msg_send![self, discoverDescriptors: id_or_nil(&descriptors) forCharacteristic: characteristic] }
    }

    pub fn read_characteristic_value(&self, characteristic: &CBCharacteristic) {
        unsafe { msg_send![self, readValueForCharacteristic: characteristic] }
    }

    pub fn read_descriptor_value(&self, descriptor: &CBDescriptor) {
        unsafe { msg_send![self, readValueForDescriptor: descriptor] }
    }

    pub fn write_characteristic_value(
        &self,
        characteristic: &CBCharacteristic,
        value: &NSData,
        write_type: CBCharacteristicWriteType,
    ) {
        let write_type: isize = write_type as isize;
        unsafe { msg_send![self, writeValue: value forCharacteristic: characteristic type: write_type] }
    }

    pub fn write_descriptor_value(&self, descriptor: &CBDescriptor, value: &NSData) {
        unsafe { msg_send![self, writeValue: value forDescriptor: descriptor] }
    }

    pub fn set_notify(&self, characteristic: &CBCharacteristic, enabled: bool) {
        unsafe { msg_send![self, setNotifyValue: enabled as BOOL forCharacteristic: characteristic] }
    }

    pub fn state(&self) -> CBPeripheralState {
        let n: NSInteger = unsafe { msg_send![self, state] };
        match n {
            0 => CBPeripheralState::Disconnected,
            1 => CBPeripheralState::Connecting,
            2 => CBPeripheralState::Connected,
            3 => CBPeripheralState::Disconnecting,
            _ => panic!("Unexpected peripheral state"),
        }
    }

    pub fn read_rssi(&self) {
        unsafe { msg_send![self, readRSSI] }
    }
}

impl CBService {
    pub fn uuid(&self) -> ShareId<CBUUID> {
        unsafe {
            let uuid: *mut CBUUID = msg_send![self, UUID];
            ShareId::from_ptr(uuid)
        }
    }

    pub fn peripheral(&self) -> ShareId<CBPeripheral> {
        unsafe {
            let peripheral: *mut CBPeripheral = msg_send![self, peripheral];
            ShareId::from_ptr(peripheral)
        }
    }

    pub fn is_primary(&self) -> bool {
        let res: BOOL = unsafe { msg_send![self, isPrimary] };
        res != NO
    }

    pub fn characteristics(&self) -> Option<ShareId<NSArray<CBCharacteristic>>> {
        unsafe {
            let characteristics: *mut NSArray<CBCharacteristic> = msg_send![self, includedServices];
            (!characteristics.is_null()).then(|| ShareId::from_ptr(characteristics))
        }
    }

    pub fn included_services(&self) -> Option<ShareId<NSArray<CBService>>> {
        unsafe {
            let services: *mut NSArray<CBService> = msg_send![self, includedServices];
            (!services.is_null()).then(|| ShareId::from_ptr(services))
        }
    }
}

impl CBCharacteristic {
    pub fn uuid(&self) -> ShareId<CBUUID> {
        unsafe {
            let uuid: *mut CBUUID = msg_send![self, UUID];
            ShareId::from_ptr(uuid)
        }
    }

    pub fn service(&self) -> ShareId<CBService> {
        unsafe {
            let service: *mut CBService = msg_send![self, service];
            ShareId::from_ptr(service)
        }
    }

    pub fn value(&self) -> Option<ShareId<NSData>> {
        unsafe {
            let value: *mut NSData = msg_send![self, value];
            (!value.is_null()).then(|| ShareId::from_ptr(value))
        }
    }

    pub fn descriptors(&self) -> Option<ShareId<NSArray<CBDescriptor>>> {
        unsafe {
            let descriptors: *mut NSArray<CBDescriptor> = msg_send![self, descriptors];
            (!descriptors.is_null()).then(|| ShareId::from_ptr(descriptors))
        }
    }

    pub fn properties(&self) -> NSUInteger {
        unsafe { msg_send![self, properties] }
    }

    pub fn is_notifying(&self) -> bool {
        let res: BOOL = unsafe { msg_send![self, isNotifying] };
        res != NO
    }

    pub fn is_broadcasting(&self) -> bool {
        let res: BOOL = unsafe { msg_send![self, isBroadcasting] };
        res != NO
    }
}

impl CBDescriptor {
    pub fn uuid(&self) -> ShareId<CBUUID> {
        unsafe {
            let uuid: *mut CBUUID = msg_send![self, UUID];
            ShareId::from_ptr(uuid)
        }
    }

    pub fn characteristic(&self) -> ShareId<CBCharacteristic> {
        unsafe {
            let characteristic: *mut CBCharacteristic = msg_send![self, characteristic];
            ShareId::from_ptr(characteristic)
        }
    }

    pub fn value(&self) -> Option<ShareId<NSObject>> {
        unsafe {
            let value: *mut NSObject = msg_send![self, value];
            (!value.is_null()).then(|| ShareId::from_ptr(value))
        }
    }
}

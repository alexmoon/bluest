#![allow(clippy::let_unit_value)]
#![allow(missing_docs)]
#![allow(unused)]

use std::collections::HashMap;
use std::os::raw::{c_char, c_void};

use enumflags2::{bitflags, BitFlags};
use num_enum::{FromPrimitive, TryFromPrimitive};
use objc::runtime::{Object, BOOL, NO};
use objc::{msg_send, sel, sel_impl};
use objc_foundation::{
    object_struct, INSData, INSDictionary, INSFastEnumeration, INSObject, INSString, NSArray, NSData, NSDictionary,
    NSObject, NSString,
};
use objc_id::{Id, ShareId};

use super::delegates::{CentralDelegate, PeripheralDelegate};
use crate::btuuid::BluetoothUuidExt;
use crate::{AdvertisementData, ManufacturerData, SmallVec, Uuid};

#[allow(non_camel_case_types)]
pub type id = *mut Object;

pub type NSInteger = isize;
pub type NSUInteger = usize;

#[allow(non_upper_case_globals)]
pub const nil: *mut Object = std::ptr::null_mut();

#[repr(isize)]
#[non_exhaustive]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum CBManagerState {
    #[default]
    Unknown = 0,
    Resetting = 1,
    Unsupported = 2,
    Unauthorized = 3,
    PoweredOff = 4,
    PoweredOn = 5,
}

#[repr(isize)]
#[non_exhaustive]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum CBManagerAuthorization {
    #[default]
    NotDetermined = 0,
    Restricted = 1,
    Denied = 2,
    AllowedAlways = 3,
}

#[repr(u32)]
#[bitflags]
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBCentralManagerFeature {
    ExtendedScanAndConnect = 1,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBCharacteristicWriteType {
    WithResponse = 0,
    WithoutResponse = 1,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CBPeripheralState {
    Disconnected = 0,
    Connecting = 1,
    Connected = 2,
    Disconnecting = 3,
}

#[non_exhaustive]
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

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(isize)]
pub enum CBATTError {
    Success = 0,
    InvalidHandle = 1,
    ReadNotPermitted = 2,
    WriteNotPermitted = 3,
    InvalidPdu = 4,
    InsufficientAuthentication = 5,
    RequestNotSupported = 6,
    InvalidOffset = 7,
    InsufficientAuthorization = 8,
    PrepareQueueFull = 9,
    AttributeNotFound = 10,
    AttributeNotLong = 11,
    InsufficientEncryptionKeySize = 12,
    InvalidAttributeValueLength = 13,
    UnlikelyError = 14,
    InsufficientEncryption = 15,
    UnsupportedGroupType = 16,
    InsufficientResources = 17,
}

impl AdvertisementData {
    pub(super) fn from_nsdictionary(adv_data: ShareId<NSDictionary<NSString, NSObject>>) -> Self {
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

pub unsafe fn option_from_ptr<T: objc::Message, O: objc_id::Ownership>(ptr: *mut T) -> Option<Id<T, O>> {
    (!ptr.is_null()).then(|| Id::from_ptr(ptr))
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

impl NSError {
    pub fn code(&self) -> NSInteger {
        unsafe { msg_send![self, code] }
    }

    pub fn domain(&self) -> ShareId<NSString> {
        unsafe { Id::from_ptr(msg_send![self, domain]) }
    }

    pub fn user_info(&self) -> ShareId<NSDictionary<NSString, NSObject>> {
        unsafe { Id::from_ptr(msg_send![self, userInfo]) }
    }

    pub fn localized_description(&self) -> ShareId<NSString> {
        unsafe { Id::from_ptr(msg_send![self, localizedDescription]) }
    }

    pub fn localized_recovery_options(&self) -> Option<ShareId<NSArray<NSString>>> {
        unsafe { option_from_ptr(msg_send![self, localizedRecoveryOptions]) }
    }

    pub fn localized_recovery_suggestion(&self) -> Option<ShareId<NSString>> {
        unsafe { option_from_ptr(msg_send![self, localizedRecoverySuggestion]) }
    }

    pub fn localized_failure_reason(&self) -> Option<ShareId<NSString>> {
        unsafe { option_from_ptr(msg_send![self, localizedFailureReason]) }
    }

    pub fn help_anchor(&self) -> Option<ShareId<NSString>> {
        unsafe { option_from_ptr(msg_send![self, helpAnchor]) }
    }

    pub fn underlying_errors(&self) -> ShareId<NSArray<NSError>> {
        unsafe { Id::from_ptr(msg_send![self, underlyingErrors]) }
    }
}

impl NSUUID {
    pub fn from_uuid(uuid: Uuid) -> Id<Self> {
        unsafe {
            let obj: *mut Self = msg_send![Self::class(), alloc];
            Id::from_retained_ptr(msg_send![obj, initWithBytes: uuid.as_bytes()])
        }
    }

    pub fn to_uuid(&self) -> Uuid {
        let mut bytes = [0u8; 16];
        let _: () = unsafe { msg_send!(self, getUUIDBytes: &mut bytes) };
        Uuid::from_bytes(bytes)
    }
}

impl CBUUID {
    pub fn from_uuid(uuid: Uuid) -> Id<Self> {
        unsafe {
            let obj: *mut Self =
                msg_send![Self::class(), UUIDWithData: NSData::from_vec(uuid.as_bluetooth_bytes().to_owned())];
            Id::from_retained_ptr(obj)
        }
    }

    pub fn to_uuid(&self) -> Uuid {
        let data: ShareId<NSData> = unsafe { ShareId::from_ptr(msg_send!(self, data)) };
        Uuid::from_bluetooth_bytes(data.bytes())
    }
}

impl CBCentralManager {
    pub fn with_delegate(delegate: Id<CentralDelegate>, queue: id) -> Id<CBCentralManager> {
        unsafe {
            let obj: *mut Self = msg_send![Self::class(), alloc];
            Id::from_retained_ptr(msg_send![obj, initWithDelegate: delegate queue: queue])
        }
    }

    pub fn state(&self) -> CBManagerState {
        let state: NSInteger = unsafe { msg_send![self, state] };
        state.into()
    }

    pub fn authorization() -> CBManagerAuthorization {
        let authorization: NSInteger = unsafe { msg_send![Self::class(), authorization] };
        authorization.into()
    }

    pub fn connect_peripheral(&self, peripheral: &CBPeripheral, options: Option<Id<NSDictionary<NSString, NSObject>>>) {
        unsafe { msg_send![self, connectPeripheral: peripheral options: id_or_nil(&options)] }
    }

    pub fn cancel_peripheral_connection(&self, peripheral: &CBPeripheral) {
        unsafe { msg_send![self, cancelPeripheralConnection: peripheral] }
    }

    pub fn retrieve_connected_peripherals_with_services(
        &self,
        services: Id<NSArray<CBUUID>>,
    ) -> Id<NSArray<CBPeripheral>> {
        unsafe { Id::from_ptr(msg_send![self, retrieveConnectedPeripheralsWithServices: services]) }
    }

    pub fn retrieve_peripherals_with_identifiers(&self, identifiers: Id<NSArray<NSUUID>>) -> Id<NSArray<CBPeripheral>> {
        unsafe { Id::from_ptr(msg_send![self, retrievePeripheralsWithIdentifiers: identifiers]) }
    }

    pub fn scan_for_peripherals_with_services(
        &self,
        services: Option<Id<NSArray<CBUUID>>>,
        options: Option<Id<NSDictionary<NSString, NSObject>>>,
    ) {
        unsafe { msg_send![self, scanForPeripheralsWithServices: id_or_nil(&services) options: id_or_nil(&options)] }
    }

    pub fn stop_scan(&self) {
        unsafe { msg_send![self, stopScan] }
    }

    pub fn is_scanning(&self) -> bool {
        let res: BOOL = unsafe { msg_send![self, isScanning] };
        res != NO
    }

    pub fn supports_features(&self, features: BitFlags<CBCentralManagerFeature>) -> bool {
        let features = features.bits() as NSUInteger;
        let res: BOOL = unsafe { msg_send![self, supportsFeatures: features] };
        res != NO
    }

    pub fn delegate(&self) -> Option<ShareId<CentralDelegate>> {
        unsafe { option_from_ptr(msg_send![self, delegate]) }
    }

    pub fn register_for_connection_events_with_options(&self, options: Id<NSDictionary<NSString, NSObject>>) {
        unsafe { msg_send![self, registerForConnectionEventsWithOptions: options] }
    }
}

impl CBPeripheral {
    pub fn identifier(&self) -> ShareId<NSUUID> {
        unsafe { ShareId::from_ptr(msg_send![self, identifier]) }
    }

    pub fn name(&self) -> Option<ShareId<NSString>> {
        unsafe { option_from_ptr(msg_send![self, name]) }
    }

    pub fn delegate(&self) -> Option<ShareId<PeripheralDelegate>> {
        unsafe { option_from_ptr(msg_send![self, delegate]) }
    }

    pub fn subscribe(&self) -> crate::Result<tokio::sync::broadcast::Receiver<super::delegates::PeripheralEvent>> {
        self.delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or_else(|| {
                crate::Error::new(
                    crate::error::ErrorKind::Internal,
                    None,
                    "failed to get sender for peripheral delegate".to_string(),
                )
            })
    }
    pub fn set_delegate(&self, delegate: Id<PeripheralDelegate>) {
        unsafe { msg_send![self, setDelegate: delegate] }
    }

    pub fn services(&self) -> Option<ShareId<NSArray<CBService>>> {
        unsafe { option_from_ptr(msg_send![self, services]) }
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

    pub fn discover_descriptors(&self, characteristic: &CBCharacteristic) {
        unsafe { msg_send![self, discoverDescriptorsForCharacteristic: characteristic] }
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
        unsafe { ShareId::from_ptr(msg_send![self, UUID]) }
    }

    pub fn peripheral(&self) -> ShareId<CBPeripheral> {
        unsafe { ShareId::from_ptr(msg_send![self, peripheral]) }
    }

    pub fn is_primary(&self) -> bool {
        let res: BOOL = unsafe { msg_send![self, isPrimary] };
        res != NO
    }

    pub fn characteristics(&self) -> Option<ShareId<NSArray<CBCharacteristic>>> {
        unsafe { option_from_ptr(msg_send![self, characteristics]) }
    }

    pub fn included_services(&self) -> Option<ShareId<NSArray<CBService>>> {
        unsafe { option_from_ptr(msg_send![self, includedServices]) }
    }
}

impl CBCharacteristic {
    pub fn uuid(&self) -> ShareId<CBUUID> {
        unsafe { ShareId::from_ptr(msg_send![self, UUID]) }
    }

    pub fn service(&self) -> ShareId<CBService> {
        unsafe { ShareId::from_ptr(msg_send![self, service]) }
    }

    pub fn value(&self) -> Option<ShareId<NSData>> {
        unsafe { option_from_ptr(msg_send![self, value]) }
    }

    pub fn descriptors(&self) -> Option<ShareId<NSArray<CBDescriptor>>> {
        unsafe { option_from_ptr(msg_send![self, descriptors]) }
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
        unsafe { ShareId::from_ptr(msg_send![self, UUID]) }
    }

    pub fn characteristic(&self) -> ShareId<CBCharacteristic> {
        unsafe { ShareId::from_ptr(msg_send![self, characteristic]) }
    }

    pub fn value(&self) -> Option<ShareId<NSObject>> {
        unsafe { option_from_ptr(msg_send![self, value]) }
    }
}

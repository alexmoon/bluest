use objc::{class, msg_send, sel, sel_impl};
use objc_foundation::{INSData, INSObject, NSObject};
use objc_id::ShareId;
use smallvec::SmallVec;
use uuid::Uuid;

use super::delegates::PeripheralEvent;
use super::types::{CBDescriptor, NSUInteger};

use crate::error::ErrorKind;
use crate::{Error, Result};

/// A Bluetooth GATT descriptor
pub struct Descriptor {
    descriptor: ShareId<CBDescriptor>,
}

fn value_to_slice(val: &NSObject) -> SmallVec<[u8; 16]> {
    if val.is_kind_of(class!(NSNumber)) {
        // Characteristic EXtended Properties, Client Characteristic COnfiguration, Service Characteristic Configuration, or L2CAP PSM Value Characteristic
        let n: u16 = unsafe { msg_send![val, unsignedShortValue] };
        SmallVec::from_slice(&n.to_le_bytes()[..])
    } else if val.is_kind_of(class!(NSString)) {
        // Characteristic User Description
        let ptr: *const u8 = unsafe { msg_send![val, UTF8String] };
        let val = if ptr.is_null() {
            &[]
        } else {
            let len: NSUInteger = unsafe { msg_send![val, lengthOfBytesUsingEncoding: 4usize] }; // NSUTF8StringEncoding
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        SmallVec::from_slice(val)
    } else if val.is_kind_of(class!(NSData)) {
        // All other descriptors
        let ptr: *const u8 = unsafe { msg_send![val, bytes] };
        let val = if ptr.is_null() {
            &[]
        } else {
            let len: NSUInteger = unsafe { msg_send![val, length] };
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        SmallVec::from_slice(val)
    } else {
        SmallVec::new()
    }
}

impl Descriptor {
    pub(super) fn new(descriptor: &CBDescriptor) -> Self {
        Descriptor {
            descriptor: unsafe { ShareId::from_ptr(descriptor as *const _ as *mut _) },
        }
    }

    /// The [Uuid] identifying the type of this GATT descriptor
    pub fn uuid(&self) -> Uuid {
        self.descriptor.uuid().to_uuid()
    }

    /// The cached value of this descriptor
    ///
    /// If the value has not yet been read, this function may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<SmallVec<[u8; 16]>> {
        self.descriptor.value().map(|val| value_to_slice(&*val)).ok_or(Error {
            kind: ErrorKind::AdapterUnavailable,
            message: String::new(),
        })
    }

    /// Read the value of this descriptor from the device
    pub async fn read(&self) -> Result<SmallVec<[u8; 16]>> {
        let peripheral = self.descriptor.characteristic().service().peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;

        peripheral.read_descriptor_value(&self.descriptor);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::DescriptorValueUpdate { descriptor, error }) if descriptor == self.descriptor => {
                    match error {
                        Some(err) => Err(&*err)?,
                        None => return self.value().await,
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }
    }

    /// Write the value of this descriptor on the device to `value`
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let peripheral = self.descriptor.characteristic().service().peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;

        let data = INSData::from_vec(value.to_vec());
        peripheral.write_descriptor_value(&self.descriptor, &data);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::DescriptorValueWrite { descriptor, error }) if descriptor == self.descriptor => {
                    match error {
                        Some(err) => Err(&*err)?,
                        None => return Ok(()),
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }
    }
}

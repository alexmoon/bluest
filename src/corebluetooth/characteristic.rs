use enumflags2::{bitflags, BitFlags};
use futures_core::Stream;
use objc_foundation::{INSArray, INSData, INSFastEnumeration, NSArray};
use objc_id::ShareId;
use smallvec::SmallVec;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use super::delegates::PeripheralEvent;
use super::types::{CBCharacteristicWriteType, CBUUID};
use super::{descriptor::Descriptor, types::CBCharacteristic};

use crate::error::ErrorKind;
use crate::{Error, Result};

#[bitflags]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Properties {
    Broadcast = 0x01,
    Read = 0x02,
    WriteWithoutResponse = 0x04,
    Write = 0x08,
    Notify = 0x10,
    Indicate = 0x20,
    AuthenticatedSignedWrites = 0x40,
    ExtendedProperties = 0x80,
}

#[bitflags]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ExtendedProperties {
    ReliableWrite = 0x0001,
    WritableAuxiliaries = 0x0002,
}

pub struct Characteristic {
    characteristic: ShareId<CBCharacteristic>,
}

impl Characteristic {
    pub(crate) fn new(characteristic: &CBCharacteristic) -> Self {
        Characteristic {
            characteristic: unsafe { ShareId::from_ptr(characteristic as *const _ as *mut _) },
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.characteristic.uuid().to_uuid()
    }

    pub fn properties(&self) -> BitFlags<Properties> {
        BitFlags::from_bits(self.characteristic.properties() as u8).unwrap()
    }

    pub fn value(&self) -> Option<SmallVec<[u8; 16]>> {
        self.characteristic.value().map(|val| SmallVec::from_slice(val.bytes()))
    }

    pub async fn read(&self) -> Result<SmallVec<[u8; 16]>> {
        let peripheral = self.characteristic.service().peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;

        peripheral.read_characteristic_value(&self.characteristic);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::CharacteristicValueUpdate { characteristic, error })
                    if characteristic == self.characteristic =>
                {
                    match error {
                        Some(err) => Err(&*err)?,
                        None => return Ok(self.value().unwrap()),
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }
    }

    pub async fn write(&self, value: &[u8]) -> Result<()> {
        self.write_kind(value, CBCharacteristicWriteType::WithoutResponse).await
    }

    pub async fn write_with_response(&self, value: &[u8]) -> Result<()> {
        self.write_kind(value, CBCharacteristicWriteType::WithResponse).await
    }

    async fn write_kind(&self, value: &[u8], kind: CBCharacteristicWriteType) -> Result<()> {
        let peripheral = self.characteristic.service().peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;

        let data = INSData::from_vec(value.to_vec());
        peripheral.write_characteristic_value(&self.characteristic, &data, kind);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::CharacteristicValueWrite { characteristic, error })
                    if characteristic == self.characteristic =>
                {
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

    pub async fn notify(&self) -> Result<impl Stream<Item = Result<SmallVec<[u8; 16]>>> + '_> {
        let guard = scopeguard::guard((), move |_| {
            let peripheral = self.characteristic.service().peripheral();
            peripheral.set_notify(&self.characteristic, false);
        });

        let peripheral = self.characteristic.service().peripheral();
        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::NotificationStateUpdate { characteristic, error })
                    if characteristic == self.characteristic =>
                {
                    match error {
                        Some(err) => Err(&*err)?,
                        None => break,
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }

        let updates = BroadcastStream::new(receiver).filter_map(move |x| {
            let _guard = &guard;
            match x {
                Ok(PeripheralEvent::CharacteristicValueUpdate { characteristic, error })
                    if characteristic == self.characteristic =>
                {
                    match error {
                        Some(err) => Some(Err(Error::from(&*err))),
                        None => Some(Ok(self.value().unwrap())),
                    }
                }
                _ => None,
            }
        });

        peripheral.set_notify(&self.characteristic, true);

        Ok(updates)
    }

    pub fn is_notifying(&self) -> bool {
        self.characteristic.is_notifying()
    }

    pub fn is_broadcasting(&self) -> bool {
        self.characteristic.is_broadcasting()
    }

    pub async fn discover_descriptors(&self, descriptors: Option<&[Uuid]>) -> Result<SmallVec<[Descriptor; 2]>> {
        let descriptors = descriptors.map(|x| {
            let vec = x.iter().map(CBUUID::from_uuid).collect::<Vec<_>>();
            NSArray::from_vec(vec)
        });

        let peripheral = self.characteristic.service().peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;
        peripheral.discover_descriptors(&self.characteristic, descriptors);

        loop {
            match receiver.recv().await {
                Ok(PeripheralEvent::DiscoveredDescriptors { characteristic, error })
                    if characteristic == self.characteristic =>
                {
                    match error {
                        Some(err) => Err(&*err)?,
                        None => break,
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }

        Ok(self.descriptors().await)
    }

    pub async fn descriptors(&self) -> SmallVec<[Descriptor; 2]> {
        match self.characteristic.descriptors() {
            Some(s) => s.enumerator().map(Descriptor::new).collect(),
            None => SmallVec::new(),
        }
    }
}

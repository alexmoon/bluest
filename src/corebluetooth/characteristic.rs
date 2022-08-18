use enumflags2::BitFlags;
use futures::Stream;
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
use crate::{CharacteristicProperty, Error, Result};

/// A Bluetooth GATT characteristic
pub struct Characteristic {
    characteristic: ShareId<CBCharacteristic>,
}

impl Characteristic {
    pub(super) fn new(characteristic: &CBCharacteristic) -> Self {
        Characteristic {
            characteristic: unsafe { ShareId::from_ptr(characteristic as *const _ as *mut _) },
        }
    }

    /// The [Uuid] identifying the type of this GATT characteristic
    pub fn uuid(&self) -> Uuid {
        self.characteristic.uuid().to_uuid()
    }

    /// The properties of this this GATT characteristic.
    ///
    /// Characteristic properties indicate which operations (e.g. read, write, notify, etc) may be performed on this
    /// characteristic.
    pub fn properties(&self) -> BitFlags<CharacteristicProperty> {
        BitFlags::from_bits(self.characteristic.properties() as u32).unwrap_or_else(|x| x.truncate())
    }

    /// The cached value of this characteristic
    ///
    /// If the value has not yet been read, this function may either return an error or perform a read of the value.
    pub fn value(&self) -> Result<SmallVec<[u8; 16]>> {
        self.characteristic
            .value()
            .map(|val| SmallVec::from_slice(val.bytes()))
            .ok_or_else(|| Error {
                kind: ErrorKind::AdapterUnavailable,
                message: String::new(),
            })
    }

    /// Read the value of this characteristic from the device
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
                        None => return self.value(),
                    }
                }
                Err(_err) => Err(ErrorKind::InternalError)?,
                _ => (),
            }
        }
    }

    /// Write the value of this descriptor on the device to `value`
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        self.write_kind(value, CBCharacteristicWriteType::WithoutResponse).await
    }

    /// Write the value of this descriptor on the device to `value` and request the device return a response indicating
    /// a successful write.
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

    /// Enables notification of value changes for this GATT characteristic.
    ///
    /// Returns a stream of values for the characteristic sent from the device.
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

    /// Is the device currently sending notifications for this characteristic?
    pub async fn is_notifying(&self) -> Result<bool> {
        Ok(self.characteristic.is_notifying())
    }

    /// Discover the descriptors associated with this service.
    ///
    /// If a [Uuid] is provided, only descriptors with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// descriptors for this characteristic will be discovered.
    pub async fn discover_descriptors(&self, uuid: Option<Uuid>) -> Result<SmallVec<[Descriptor; 2]>> {
        let uuids = uuid.map(|x| {
            let vec = vec![CBUUID::from_uuid(x)];
            NSArray::from_vec(vec)
        });

        let peripheral = self.characteristic.service().peripheral();

        let mut receiver = peripheral
            .delegate()
            .and_then(|x| x.sender().map(|x| x.subscribe()))
            .ok_or(ErrorKind::InternalError)?;
        peripheral.discover_descriptors(&self.characteristic, uuids);

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

    /// Get previously discovered descriptors.
    ///
    /// If no descriptors have been discovered yet, this function may either perform descriptor discovery or
    /// return an empty set.
    pub async fn descriptors(&self) -> SmallVec<[Descriptor; 2]> {
        match self.characteristic.descriptors() {
            Some(s) => s.enumerator().map(Descriptor::new).collect(),
            None => SmallVec::new(),
        }
    }
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Characteristic {
    inner: ShareId<CBCharacteristic>,
}

impl Characteristic {
    pub(super) fn new(characteristic: &CBCharacteristic) -> Self {
        Characteristic {
            inner: unsafe { ShareId::from_ptr(characteristic as *const _ as *mut _) },
        }
    }

    /// The [Uuid] identifying the type of this GATT characteristic
    pub fn uuid(&self) -> Uuid {
        self.inner.uuid().to_uuid()
    }

    /// The properties of this this GATT characteristic.
    ///
    /// Characteristic properties indicate which operations (e.g. read, write, notify, etc) may be performed on this
    /// characteristic.
    pub fn properties(&self) -> BitFlags<CharacteristicProperty> {
        BitFlags::from_bits(self.inner.properties() as u32).unwrap_or_else(|x| x.truncate())
    }

    /// The cached value of this characteristic
    ///
    /// If the value has not yet been read, this function may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<SmallVec<[u8; 16]>> {
        self.inner
            .value()
            .map(|val| SmallVec::from_slice(val.bytes()))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "the characteristic value has not been read".to_string(),
                )
            })
    }

    /// Read the value of this characteristic from the device
    pub async fn read(&self) -> Result<SmallVec<[u8; 16]>> {
        let peripheral = self.inner.service().peripheral();
        let mut receiver = peripheral.subscribe()?;

        peripheral.read_characteristic_value(&self.inner);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::CharacteristicValueUpdate { characteristic, error }
                    if characteristic == self.inner =>
                {
                    match error {
                        Some(err) => return Err(Error::from_nserror(err)),
                        None => return self.value().await,
                    }
                }
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
        let peripheral = self.inner.service().peripheral();
        let mut receiver = peripheral.subscribe()?;
        let data = INSData::from_vec(value.to_vec());
        peripheral.write_characteristic_value(&self.inner, &data, kind);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::CharacteristicValueWrite { characteristic, error } if characteristic == self.inner => {
                    match error {
                        Some(err) => Err(Error::from_nserror(err))?,
                        None => return Ok(()),
                    }
                }
                _ => (),
            }
        }
    }

    /// Enables notification of value changes for this GATT characteristic.
    ///
    /// Returns a stream of values for the characteristic sent from the device.
    pub async fn notify(&self) -> Result<impl Stream<Item = Result<SmallVec<[u8; 16]>>> + '_> {
        let guard = scopeguard::guard((), move |_| {
            let peripheral = self.inner.service().peripheral();
            peripheral.set_notify(&self.inner, false);
        });

        let peripheral = self.inner.service().peripheral();
        let mut receiver = peripheral.subscribe()?;

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::NotificationStateUpdate { characteristic, error } if characteristic == self.inner => {
                    match error {
                        Some(err) => Err(Error::from_nserror(err))?,
                        None => break,
                    }
                }
                _ => (),
            }
        }

        let updates = BroadcastStream::new(receiver)
            .filter_map(move |x| {
                let _guard = &guard;
                match x {
                    Ok(PeripheralEvent::CharacteristicValueUpdate { characteristic, error })
                        if characteristic == self.inner =>
                    {
                        match error {
                            Some(err) => Some(Err(Error::from_nserror(err))),
                            None => Some(Ok(())),
                        }
                    }
                    _ => None,
                }
            })
            .then(move |x| {
                Box::pin(async move {
                    match x {
                        Ok(_) => self.value().await,
                        Err(err) => Err(err),
                    }
                })
            });

        peripheral.set_notify(&self.inner, true);

        Ok(updates)
    }

    /// Is the device currently sending notifications for this characteristic?
    pub async fn is_notifying(&self) -> Result<bool> {
        Ok(self.inner.is_notifying())
    }

    /// Is the device currently broadcasting this characteristic?
    ///
    /// # Platform specific
    ///
    /// This function is available on MacOS/iOS only.
    pub async fn is_broadcasting(&self) -> Result<bool> {
        Ok(self.inner.is_broadcasting())
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

        let peripheral = self.inner.service().peripheral();
        let mut receiver = peripheral.subscribe()?;
        peripheral.discover_descriptors(&self.inner, uuids);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredDescriptors { characteristic, error } if characteristic == self.inner => {
                    match error {
                        Some(err) => Err(Error::from_nserror(err))?,
                        None => break,
                    }
                }
                _ => (),
            }
        }

        self.descriptors().await
    }

    /// Get previously discovered descriptors.
    ///
    /// If no descriptors have been discovered yet, this function may either perform descriptor discovery or
    /// return an error.
    pub async fn descriptors(&self) -> Result<SmallVec<[Descriptor; 2]>> {
        self.inner
            .descriptors()
            .map(|s| s.enumerator().map(Descriptor::new).collect())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "no descriptors have been discovered".to_string(),
                )
            })
    }
}

use enumflags2::{BitFlags, FromBitsError};
use futures::Stream;
use objc_foundation::{INSData, INSFastEnumeration};
use objc_id::ShareId;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use super::delegates::PeripheralEvent;
use super::descriptor::Descriptor;
use super::types::{CBCharacteristic, CBCharacteristicWriteType};
use crate::error::ErrorKind;
use crate::{CharacteristicProperty, Error, Result, SmallVec, Uuid};

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
        BitFlags::from_bits(self.inner.properties() as u32).unwrap_or_else(FromBitsError::truncate)
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
        let service = self.inner.service();
        let peripheral = service.peripheral();
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
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&service) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }
    }

    /// Write the value of this descriptor on the device to `value` and request the device return a response indicating
    /// a successful write.
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let service = self.inner.service();
        let peripheral = service.peripheral();
        let mut receiver = peripheral.subscribe()?;
        let data = INSData::from_vec(value.to_vec());
        peripheral.write_characteristic_value(&self.inner, &data, CBCharacteristicWriteType::WithResponse);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::CharacteristicValueWrite { characteristic, error } if characteristic == self.inner => {
                    match error {
                        Some(err) => return Err(Error::from_nserror(err)),
                        None => return Ok(()),
                    }
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&service) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }
    }

    /// Write the value of this descriptor on the device to `value` without requesting a response.
    pub async fn write_without_response(&self, value: &[u8]) {
        let data = INSData::from_vec(value.to_vec());
        self.inner.service().peripheral().write_characteristic_value(
            &self.inner,
            &data,
            CBCharacteristicWriteType::WithoutResponse,
        );
    }

    /// Enables notification of value changes for this GATT characteristic.
    ///
    /// Returns a stream of values for the characteristic sent from the device.
    pub async fn notify(&self) -> Result<impl Stream<Item = Result<SmallVec<[u8; 16]>>> + '_> {
        if !(self
            .properties()
            .intersects(CharacteristicProperty::Notify | CharacteristicProperty::Indicate))
        {
            return Err(Error::new(
                ErrorKind::NotSupported,
                None,
                "characteristic does not support indications or notifications".to_string(),
            ));
        };

        let guard = scopeguard::guard((), move |_| {
            let peripheral = self.inner.service().peripheral();
            peripheral.set_notify(&self.inner, false);
        });

        let service = self.inner.service();
        let peripheral = service.peripheral();
        let mut receiver = peripheral.subscribe()?;

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::NotificationStateUpdate { characteristic, error } if characteristic == self.inner => {
                    match error {
                        Some(err) => return Err(Error::from_nserror(err)),
                        None => break,
                    }
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&service) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
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
                    Ok(PeripheralEvent::ServicesChanged { invalidated_services })
                        if invalidated_services.contains(&service) =>
                    {
                        Some(Err(ErrorKind::ServiceChanged.into()))
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

    /// Discover the descriptors associated with this service.
    ///
    /// If a [Uuid] is provided, only descriptors with that [Uuid] will be discovered. If `uuid` is `None` then all
    /// descriptors for this characteristic will be discovered.
    pub async fn discover_descriptors(&self) -> Result<SmallVec<[Descriptor; 2]>> {
        let service = self.inner.service();
        let peripheral = service.peripheral();
        let mut receiver = peripheral.subscribe()?;
        peripheral.discover_descriptors(&self.inner);

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredDescriptors { characteristic, error } if characteristic == self.inner => {
                    match error {
                        Some(err) => return Err(Error::from_nserror(err)),
                        None => break,
                    }
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&service) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
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

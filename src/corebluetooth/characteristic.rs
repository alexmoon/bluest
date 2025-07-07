use corebluetooth::{CBCharacteristicProperties, CBPeripheralState, CharacteristicWriteType};
use dispatch_executor::Handle;
use futures_core::Stream;
use futures_lite::StreamExt;

use super::delegates::{subscribe_peripheral, PeripheralEvent};
use crate::error::ErrorKind;
use crate::util::defer;
use crate::{Characteristic, CharacteristicProperties, Descriptor, Error, Result, Uuid};

/// A Bluetooth GATT characteristic
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CharacteristicImpl {
    inner: Handle<corebluetooth::Characteristic>,
}

impl Characteristic {
    pub(super) fn new(inner: Handle<corebluetooth::Characteristic>) -> Self {
        Characteristic(CharacteristicImpl { inner })
    }
}

impl CharacteristicImpl {
    /// The [`Uuid`] identifying the type of this GATT characteristic
    pub fn uuid(&self) -> Uuid {
        self.inner.lock(|characteristic, _| characteristic.uuid().into())
    }

    /// The [`Uuid`] identifying the type of this GATT characteristic
    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(self.uuid())
    }

    /// The properties of this this GATT characteristic.
    ///
    /// Characteristic properties indicate which operations (e.g. read, write, notify, etc) may be performed on this
    /// characteristic.
    pub async fn properties(&self) -> Result<CharacteristicProperties> {
        let cb_props = self.inner.lock(|characteristic, _| characteristic.properties());

        let mut props = CharacteristicProperties::default();
        if cb_props.contains(CBCharacteristicProperties::Broadcast) {
            props.broadcast = true;
        }
        if cb_props.contains(CBCharacteristicProperties::Read) {
            props.read = true;
        }
        if cb_props.contains(CBCharacteristicProperties::WriteWithoutResponse) {
            props.write_without_response = true;
        }
        if cb_props.contains(CBCharacteristicProperties::Write) {
            props.write = true;
        }
        if cb_props.contains(CBCharacteristicProperties::Notify) {
            props.notify = true;
        }
        if cb_props.contains(CBCharacteristicProperties::Indicate) {
            props.indicate = true;
        }
        if cb_props.contains(CBCharacteristicProperties::AuthenticatedSignedWrites) {
            props.authenticated_signed_writes = true;
        }
        if cb_props.contains(CBCharacteristicProperties::ExtendedProperties) {
            props.extended_properties = true;
        }

        Ok(props)
    }

    /// The cached value of this characteristic
    ///
    /// If the value has not yet been read, this method may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<Vec<u8>> {
        self.inner
            .lock(|characteristic, _| characteristic.value())
            .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "the characteristic value has not been read"))
    }

    /// Read the value of this characteristic from the device
    pub async fn read(&self) -> Result<Vec<u8>> {
        let (service, mut receiver) = self.inner.lock(|characteristic, executor| {
            let service = characteristic
                .service()
                .ok_or(Error::new(ErrorKind::NotFound, None, "service not found"))?;
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.read_characteristic_value(characteristic);

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok((executor.handle(service), receiver))
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::CharacteristicValueUpdate { characteristic, result }
                    if characteristic == self.inner =>
                {
                    result?;
                    return self.value().await;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(error.into());
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

    /// Write the value of this characteristic on the device to `value` and request the device return a response indicating
    /// a successful write.
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let (service, mut receiver) = self.inner.lock(|characteristic, executor| {
            let service = characteristic
                .service()
                .ok_or(Error::new(ErrorKind::NotFound, None, "service not found"))?;
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.write_characteristic_value(
                characteristic,
                value.to_vec(),
                CharacteristicWriteType::WithResponse,
            );

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok((executor.handle(service), receiver))
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::CharacteristicValueWrite { characteristic, result }
                    if characteristic == self.inner =>
                {
                    return result.map_err(Into::into);
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(error.into());
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
    pub async fn write_without_response(&self, value: &[u8]) -> Result<()> {
        let mut receiver = self.inner.lock(|characteristic, _| {
            let peripheral = characteristic
                .service()
                .and_then(|service| service.peripheral())
                .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            let receiver = subscribe_peripheral(peripheral.delegate());
            Result::<_, Error>::Ok(receiver)
        })?;

        loop {
            let service = self.inner.lock(|characteristic, executor| {
                let service =
                    characteristic
                        .service()
                        .ok_or(Error::new(ErrorKind::NotFound, None, "service not found"))?;
                let peripheral =
                    service
                        .peripheral()
                        .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

                if peripheral.state() != CBPeripheralState::Connected {
                    Err(Error::from(ErrorKind::NotConnected))
                } else if peripheral.can_send_write_without_repsonse() {
                    peripheral.write_characteristic_value(
                        characteristic,
                        value.to_vec(),
                        CharacteristicWriteType::WithoutResponse,
                    );
                    Ok(None)
                } else {
                    Ok(Some(executor.handle(service)))
                }
            })?;

            if let Some(service) = service {
                while let Ok(evt) = receiver.recv().await {
                    match evt {
                        PeripheralEvent::ReadyToWrite => break,
                        PeripheralEvent::Disconnected { error } => {
                            return Err(error.into());
                        }
                        PeripheralEvent::ServicesChanged { invalidated_services }
                            if invalidated_services.contains(&service) =>
                        {
                            return Err(ErrorKind::ServiceChanged.into());
                        }
                        _ => (),
                    }
                }
            } else {
                return Ok(());
            }
        }
    }

    /// Get the maximum amount of data that can be written in a single packet for this characteristic.
    pub fn max_write_len(&self) -> Result<usize> {
        self.inner.lock(|characteristic, _| {
            let peripheral = characteristic.service().and_then(|x| x.peripheral()).ok_or(Error::new(
                ErrorKind::NotFound,
                None,
                "peripheral not found",
            ))?;
            Ok(peripheral.max_write_value_len(CharacteristicWriteType::WithoutResponse))
        })
    }

    /// Get the maximum amount of data that can be written in a single packet for this characteristic.
    pub async fn max_write_len_async(&self) -> Result<usize> {
        self.max_write_len()
    }

    /// Enables notification of value changes for this GATT characteristic.
    ///
    /// Returns a stream of values for the characteristic sent from the device.
    pub async fn notify(&self) -> Result<impl Stream<Item = Result<Vec<u8>>> + Send + Unpin + '_> {
        let properties = self.properties().await?;
        if !(properties.notify || properties.indicate) {
            return Err(Error::new(
                ErrorKind::NotSupported,
                None,
                "characteristic does not support indications or notifications",
            ));
        };

        let (service, mut receiver) = self.inner.lock(|characteristic, executor| {
            let service = characteristic
                .service()
                .ok_or(Error::new(ErrorKind::NotFound, None, "service not found"))?;
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.set_notify(characteristic, true);

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok((executor.handle(service), receiver))
        })?;

        let guard = defer(move || {
            self.inner.lock(|characteristic, _| {
                if let Some(peripheral) = characteristic.service().and_then(|x| x.peripheral()) {
                    if peripheral.state() == CBPeripheralState::Connected {
                        peripheral.set_notify(characteristic, false);
                    }
                }
            });
        });

        loop {
            match receiver.recv().await? {
                PeripheralEvent::NotificationStateUpdate { characteristic, result } if characteristic == self.inner => {
                    result?;
                    break;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(error.into());
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&service) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }

        let updates = receiver.filter_map(move |x| {
            let _guard = &guard;
            match x {
                PeripheralEvent::CharacteristicValueUpdate { characteristic, result }
                    if characteristic == self.inner =>
                {
                    Some(result.map_err(Into::into))
                }
                PeripheralEvent::Disconnected { error } => Some(Err(error.into())),
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&service) =>
                {
                    Some(Err(ErrorKind::ServiceChanged.into()))
                }
                _ => None,
            }
        });

        Ok(updates)
    }

    /// Is the device currently sending notifications for this characteristic?
    pub async fn is_notifying(&self) -> Result<bool> {
        Ok(self.inner.lock(|characteristic, _| characteristic.is_notifying()))
    }

    /// Discover the descriptors associated with this characteristic.
    pub async fn discover_descriptors(&self) -> Result<Vec<Descriptor>> {
        let (service, mut receiver) = self.inner.lock(|characteristic, executor| {
            let service = characteristic
                .service()
                .ok_or(Error::new(ErrorKind::NotFound, None, "service not found"))?;
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.discover_descriptors(characteristic);

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok((executor.handle(service), receiver))
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DiscoveredDescriptors { characteristic, result } if characteristic == self.inner => {
                    result?;
                    break;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(error.into());
                }
                PeripheralEvent::ServicesChanged { invalidated_services }
                    if invalidated_services.contains(&service) =>
                {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }

        self.descriptors_inner()
    }

    /// Get previously discovered descriptors.
    ///
    /// If no descriptors have been discovered yet, this method will perform descriptor discovery.
    pub async fn descriptors(&self) -> Result<Vec<Descriptor>> {
        match self.descriptors_inner() {
            Ok(descriptors) => Ok(descriptors),
            Err(_) => self.discover_descriptors().await,
        }
    }

    fn descriptors_inner(&self) -> Result<Vec<Descriptor>> {
        self.inner.lock(|characteristic, executor| {
            characteristic
                .descriptors()
                .map(|s| s.into_iter().map(|x| Descriptor::new(executor.handle(x))).collect())
                .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "no descriptors have been discovered"))
        })
    }
}

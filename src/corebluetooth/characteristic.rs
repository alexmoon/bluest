use futures_core::Stream;
use futures_lite::StreamExt;
use objc2::Message;
use objc2::rc::Retained;
use objc2_foundation::NSData;

use super::delegates::{PeripheralDelegate, PeripheralEvent};
use crate::error::ErrorKind;
use crate::util::defer;
use crate::{Characteristic, CharacteristicProperties, Descriptor, Error, Result, Uuid};
use objc2_core_bluetooth::{
    CBCharacteristic, CBCharacteristicProperties, CBCharacteristicWriteType, CBPeripheralState,
};

/// A Bluetooth GATT characteristic
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CharacteristicImpl {
    inner: Retained<CBCharacteristic>,
    delegate: Retained<PeripheralDelegate>,
}

impl Characteristic {
    pub(super) fn new(
        characteristic: &CBCharacteristic,
        delegate: Retained<PeripheralDelegate>,
    ) -> Self {
        Characteristic(CharacteristicImpl {
            inner: characteristic.retain(),
            delegate,
        })
    }
}

impl CharacteristicImpl {
    /// The [`Uuid`] identifying the type of this GATT characteristic
    pub fn uuid(&self) -> Uuid {
        unsafe { Uuid::from_slice(self.inner.UUID().data().as_bytes_unchecked()).unwrap() }
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
        let mut props = CharacteristicProperties::default();
        match unsafe { self.inner.properties() } {
            CBCharacteristicProperties::Broadcast => props.broadcast = true,
            CBCharacteristicProperties::Read => props.read = true,
            CBCharacteristicProperties::WriteWithoutResponse => props.write_without_response = true,
            CBCharacteristicProperties::Write => props.write = true,
            CBCharacteristicProperties::Notify => props.notify = true,
            CBCharacteristicProperties::Indicate => props.indicate = true,
            CBCharacteristicProperties::AuthenticatedSignedWrites => {
                props.authenticated_signed_writes = true
            }
            CBCharacteristicProperties::ExtendedProperties => props.extended_properties = true,
            CBCharacteristicProperties::NotifyEncryptionRequired => {}
            CBCharacteristicProperties::IndicateEncryptionRequired => {}
            _ => {}
        }
        Ok(props)
    }

    /// The cached value of this characteristic
    ///
    /// If the value has not yet been read, this method may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<Vec<u8>> {
        unsafe {
            self.inner
                .value()
                .map(|val| val.as_bytes_unchecked().to_vec())
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::NotReady,
                        None,
                        "the characteristic value has not been read",
                    )
                })
        }
    }

    /// Read the value of this characteristic from the device
    pub async fn read(&self) -> Result<Vec<u8>> {
        let service = unsafe { self.inner.service() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "service not found",
        ))?;
        let peripheral = unsafe { service.peripheral() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "peripheral not found",
        ))?;
        let mut receiver = self.delegate.sender().new_receiver();

        if unsafe { peripheral.state() } != CBPeripheralState::Connected {
            return Err(ErrorKind::NotConnected.into());
        }

        unsafe { peripheral.readValueForCharacteristic(&self.inner) };

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::CharacteristicValueUpdate {
                    characteristic,
                    data,
                    error,
                } if characteristic == self.inner => match error {
                    Some(err) => return Err(Error::from_nserror(err)),
                    None => {
                        let data = data
                            .map(|val| unsafe { val.as_bytes_unchecked() }.to_vec())
                            .unwrap_or_default();
                        return Ok(data);
                    }
                },
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                }
                PeripheralEvent::ServicesChanged {
                    invalidated_services,
                } if invalidated_services.contains(&service) => {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }
    }

    /// Write the value of this descriptor on the device to `value` and request the device return a response indicating
    /// a successful write.
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let service = unsafe { self.inner.service() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "service not found",
        ))?;
        let peripheral = unsafe { service.peripheral() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "peripheral not found",
        ))?;
        let mut receiver = self.delegate.sender().new_receiver();

        if unsafe { peripheral.state() } != CBPeripheralState::Connected {
            return Err(ErrorKind::NotConnected.into());
        }

        let data = NSData::with_bytes(value);

        unsafe {
            peripheral.writeValue_forCharacteristic_type(
                &data,
                &self.inner,
                CBCharacteristicWriteType::WithResponse,
            )
        };

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::CharacteristicValueWrite {
                    characteristic,
                    error,
                } if characteristic == self.inner => match error {
                    Some(err) => return Err(Error::from_nserror(err)),
                    None => return Ok(()),
                },
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                }
                PeripheralEvent::ServicesChanged {
                    invalidated_services,
                } if invalidated_services.contains(&service) => {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }
    }

    /// Write the value of this descriptor on the device to `value` without requesting a response.
    pub async fn write_without_response(&self, value: &[u8]) -> Result<()> {
        let service = unsafe { self.inner.service() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "service not found",
        ))?;
        let peripheral = unsafe { service.peripheral() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "peripheral not found",
        ))?;
        let mut receiver = self.delegate.sender().new_receiver();

        if unsafe { peripheral.state() } != CBPeripheralState::Connected {
            return Err(ErrorKind::NotConnected.into());
        } else if !unsafe { peripheral.canSendWriteWithoutResponse() } {
            while let Ok(evt) = receiver.recv().await {
                match evt {
                    PeripheralEvent::ReadyToWrite => break,
                    PeripheralEvent::Disconnected { error } => {
                        return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                    }
                    PeripheralEvent::ServicesChanged {
                        invalidated_services,
                    } if invalidated_services.contains(&service) => {
                        return Err(ErrorKind::ServiceChanged.into());
                    }
                    _ => (),
                }
            }
        }

        let data = NSData::with_bytes(value);
        unsafe {
            peripheral.writeValue_forCharacteristic_type(
                &data,
                &self.inner,
                CBCharacteristicWriteType::WithoutResponse,
            )
        };
        Ok(())
    }

    /// Get the maximum amount of data that can be written in a single packet for this characteristic.
    pub fn max_write_len(&self) -> Result<usize> {
        let peripheral = unsafe { self.inner.service().and_then(|x| x.peripheral()) }.ok_or(
            Error::new(ErrorKind::NotFound, None, "peripheral not found"),
        )?;
        unsafe {
            Ok(peripheral
                .maximumWriteValueLengthForType(CBCharacteristicWriteType::WithoutResponse))
        }
    }

    /// Get the maximum amount of data that can be written in a single packet for this characteristic.
    pub async fn max_write_len_async(&self) -> Result<usize> {
        self.max_write_len()
    }

    /// Enables notification of value changes for this GATT characteristic.
    ///
    /// Returns a stream of values for the characteristic sent from the device.
    pub async fn notify(&self) -> Result<impl Stream<Item = Result<Vec<u8>>> + Unpin + '_> {
        let properties = self.properties().await?;
        if !(properties.notify || properties.indicate) {
            return Err(Error::new(
                ErrorKind::NotSupported,
                None,
                "characteristic does not support indications or notifications",
            ));
        };

        let service = unsafe { self.inner.service() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "service not found",
        ))?;
        let peripheral = unsafe { service.peripheral() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "peripheral not found",
        ))?;
        let mut receiver = self.delegate.sender().new_receiver();

        if unsafe { peripheral.state() } != CBPeripheralState::Connected {
            return Err(ErrorKind::NotConnected.into());
        }

        unsafe { peripheral.setNotifyValue_forCharacteristic(true, &self.inner) };
        let guard = defer(move || {
            if let Some(peripheral) = unsafe { self.inner.service().and_then(|x| x.peripheral()) } {
                unsafe { peripheral.setNotifyValue_forCharacteristic(false, &self.inner) };
            }
        });

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::NotificationStateUpdate {
                    characteristic,
                    error,
                } if characteristic == self.inner => match error {
                    Some(err) => return Err(Error::from_nserror(err)),
                    None => break,
                },
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                }
                PeripheralEvent::ServicesChanged {
                    invalidated_services,
                } if invalidated_services.contains(&service) => {
                    return Err(ErrorKind::ServiceChanged.into());
                }
                _ => (),
            }
        }

        let updates = receiver
            .filter_map(move |x| {
                let _guard = &guard;
                match x {
                    PeripheralEvent::CharacteristicValueUpdate {
                        characteristic,
                        data,
                        error,
                    } if characteristic == self.inner => match error {
                        Some(err) => Some(Err(Error::from_nserror(err))),
                        None => {
                            let data = data.map(|val| val.to_vec()).unwrap_or_default();
                            Some(Ok(data))
                        }
                    },
                    PeripheralEvent::Disconnected { error } => Some(Err(
                        Error::from_kind_and_nserror(ErrorKind::NotConnected, error),
                    )),
                    PeripheralEvent::ServicesChanged {
                        invalidated_services,
                    } if invalidated_services.contains(&service) => {
                        Some(Err(ErrorKind::ServiceChanged.into()))
                    }
                    _ => None,
                }
            })
            .then(move |x| {
                Box::pin(async move {
                    match x {
                        Ok(data) => Ok(data),
                        Err(err) => Err(err),
                    }
                })
            });

        Ok(updates)
    }

    /// Is the device currently sending notifications for this characteristic?
    pub async fn is_notifying(&self) -> Result<bool> {
        Ok(unsafe { self.inner.isNotifying() })
    }

    /// Discover the descriptors associated with this characteristic.
    pub async fn discover_descriptors(&self) -> Result<Vec<Descriptor>> {
        let service = unsafe { self.inner.service() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "service not found",
        ))?;
        let peripheral = unsafe { service.peripheral() }.ok_or(Error::new(
            ErrorKind::NotFound,
            None,
            "peripheral not found",
        ))?;
        let mut receiver = self.delegate.sender().new_receiver();

        if unsafe { peripheral.state() } != CBPeripheralState::Connected {
            return Err(ErrorKind::NotConnected.into());
        }

        unsafe { peripheral.discoverDescriptorsForCharacteristic(&self.inner) }

        loop {
            match receiver.recv().await.map_err(Error::from_recv_error)? {
                PeripheralEvent::DiscoveredDescriptors {
                    characteristic,
                    error,
                } if characteristic == self.inner => match error {
                    Some(err) => return Err(Error::from_nserror(err)),
                    None => break,
                },
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_nserror(ErrorKind::NotConnected, error));
                }
                PeripheralEvent::ServicesChanged {
                    invalidated_services,
                } if invalidated_services.contains(&service) => {
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
        unsafe { self.inner.descriptors() }
            .map(|s| {
                s.iter()
                    .map(|x| Descriptor::new(&x, self.delegate.clone()))
                    .collect()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotReady,
                    None,
                    "no descriptors have been discovered",
                )
            })
    }
}

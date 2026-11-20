use std::fmt;

use btuuid::BluetoothUuid;
use corebluetooth::CBPeripheralState;
use dispatch_executor::{DispatchCell, Ref};

use super::delegates::{PeripheralEvent, peripheral_delegate};
use crate::error::ErrorKind;
use crate::{Descriptor, Error, Result, Uuid};

/// A Bluetooth GATT descriptor
#[derive(Clone, PartialEq, Eq)]
pub struct DescriptorImpl {
    inner: DispatchCell<corebluetooth::Descriptor>,
    peripheral: DispatchCell<corebluetooth::Peripheral>,
    uuid: BluetoothUuid,
}

impl fmt::Debug for DescriptorImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.uuid.fmt(f)
    }
}

impl Descriptor {
    pub(super) fn new(
        inner: Ref<'_, corebluetooth::Descriptor>,
        peripheral: DispatchCell<corebluetooth::Peripheral>,
    ) -> Self {
        let uuid = inner.uuid();
        Descriptor(DescriptorImpl {
            inner: inner.retain(),
            peripheral,
            uuid,
        })
    }
}

impl DescriptorImpl {
    /// The [`Uuid`] identifying the type of this GATT descriptor
    pub fn uuid(&self) -> Uuid {
        self.uuid.into()
    }

    /// The [`Uuid`] identifying the type of this GATT descriptor
    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(self.uuid())
    }

    /// The cached value of this descriptor
    ///
    /// If the value has not yet been read, this method may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<Vec<u8>> {
        self.inner
            .barrier_sync(|descriptor| descriptor.value())
            .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "the descriptor value has not been read"))
    }

    /// Read the value of this descriptor from the device
    pub async fn read(&self) -> Result<Vec<u8>> {
        let (service, mut receiver) = self.inner.barrier_sync(|descriptor| {
            with_peripheral_and_service(descriptor, |peripheral, service| {
                if peripheral.state() != CBPeripheralState::Connected {
                    return Err(Error::from(ErrorKind::NotConnected));
                }

                peripheral.read_descriptor_value(&descriptor);

                let receiver = peripheral_delegate(&peripheral).subscribe();
                Ok((service.retain(), receiver))
            })
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DescriptorValueUpdate { descriptor, result } if descriptor == self.inner => {
                    result?;
                    return self.value().await;
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_error(ErrorKind::NotConnected, error));
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

    /// Write the value of this descriptor on the device to `value`
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let (service, mut receiver) = self.inner.barrier_sync(|descriptor| {
            with_peripheral_and_service(descriptor, |peripheral, service| {
                if peripheral.state() != CBPeripheralState::Connected {
                    return Err(Error::from(ErrorKind::NotConnected));
                }

                peripheral.write_descriptor_value(&descriptor, value.to_vec());

                let receiver = peripheral_delegate(&peripheral).subscribe();
                Ok((service.retain(), receiver))
            })
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DescriptorValueWrite { descriptor, result } if descriptor == self.inner => {
                    return result.map_err(Into::into);
                }
                PeripheralEvent::Disconnected { error } => {
                    return Err(Error::from_kind_and_error(ErrorKind::NotConnected, error));
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
}

pub(crate) fn with_characteristic<T>(
    descriptor: Ref<'_, corebluetooth::Descriptor>,
    f: impl FnOnce(Ref<'_, corebluetooth::Characteristic>) -> Result<T>,
) -> Result<T> {
    let characteristic =
        descriptor
            .characteristic()
            .ok_or(Error::new(ErrorKind::NotFound, None, "characteristic not found"))?;
    f(characteristic.borrow(descriptor.scope()))
}

pub(crate) fn with_peripheral_and_service<T>(
    descriptor: Ref<'_, corebluetooth::Descriptor>,
    f: impl FnOnce(Ref<'_, corebluetooth::Peripheral>, Ref<'_, corebluetooth::Service>) -> Result<T>,
) -> Result<T> {
    with_characteristic(descriptor, |characteristic| {
        super::characteristic::with_peripheral_and_service(characteristic, f)
    })
}

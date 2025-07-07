use corebluetooth::CBPeripheralState;
use dispatch_executor::Handle;

use super::delegates::{subscribe_peripheral, PeripheralEvent};
use crate::error::ErrorKind;
use crate::{Descriptor, Error, Result, Uuid};

/// A Bluetooth GATT descriptor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorImpl {
    inner: Handle<corebluetooth::Descriptor>,
}

impl Descriptor {
    pub(super) fn new(inner: Handle<corebluetooth::Descriptor>) -> Self {
        Descriptor(DescriptorImpl { inner })
    }
}

impl DescriptorImpl {
    /// The [`Uuid`] identifying the type of this GATT descriptor
    pub fn uuid(&self) -> Uuid {
        self.inner.lock(|descriptor, _| descriptor.uuid().into())
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
            .lock(|descriptor, _| descriptor.value())
            .ok_or_else(|| Error::new(ErrorKind::NotReady, None, "the descriptor value has not been read"))
    }

    /// Read the value of this descriptor from the device
    pub async fn read(&self) -> Result<Vec<u8>> {
        let (service, mut receiver) = self.inner.lock(|descriptor, executor| {
            let service = descriptor.characteristic().and_then(|x| x.service()).ok_or(Error::new(
                ErrorKind::NotFound,
                None,
                "service not found",
            ))?;
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.read_descriptor_value(descriptor);

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok((executor.handle(service), receiver))
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DescriptorValueUpdate { descriptor, result } if descriptor == self.inner => {
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

    /// Write the value of this descriptor on the device to `value`
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let (service, mut receiver) = self.inner.lock(|descriptor, executor| {
            let service = descriptor.characteristic().and_then(|x| x.service()).ok_or(Error::new(
                ErrorKind::NotFound,
                None,
                "service not found",
            ))?;
            let peripheral =
                service
                    .peripheral()
                    .ok_or(Error::new(ErrorKind::NotFound, None, "peripheral not found"))?;

            if peripheral.state() != CBPeripheralState::Connected {
                return Err(Error::from(ErrorKind::NotConnected));
            }

            peripheral.write_descriptor_value(descriptor, value.to_vec());

            let receiver = subscribe_peripheral(peripheral.delegate());
            Ok((executor.handle(service), receiver))
        })?;

        loop {
            match receiver.recv().await? {
                PeripheralEvent::DescriptorValueWrite { descriptor, result } if descriptor == self.inner => {
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
}

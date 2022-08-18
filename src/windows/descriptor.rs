use smallvec::SmallVec;
use uuid::Uuid;
use windows::{
    Devices::Bluetooth::{
        BluetoothCacheMode,
        GenericAttributeProfile::{GattCommunicationStatus, GattDescriptor},
    },
    Storage::Streams::{DataReader, DataWriter},
};

use crate::{error::ErrorKind, Error, Result};

/// A Bluetooth GATT descriptor
pub struct Descriptor {
    descriptor: GattDescriptor,
}

impl Descriptor {
    pub(super) fn new(descriptor: GattDescriptor) -> Self {
        Descriptor { descriptor }
    }

    /// The [Uuid] identifying the type of descriptor
    pub fn uuid(&self) -> Result<Uuid> {
        Ok(Uuid::from_u128(self.descriptor.Uuid()?.to_u128()))
    }

    /// The cached value of this descriptor
    ///
    /// If the value has not yet been read, this function may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<SmallVec<[u8; 16]>> {
        self.read_value(BluetoothCacheMode::Cached).await
    }

    /// Read the value of this descriptor from the device
    pub async fn read(&self) -> Result<SmallVec<[u8; 16]>> {
        self.read_value(BluetoothCacheMode::Uncached).await
    }

    async fn read_value(&self, cachemode: BluetoothCacheMode) -> Result<SmallVec<[u8; 16]>> {
        let res = self.descriptor.ReadValueWithCacheModeAsync(cachemode)?.await?;

        if let Ok(GattCommunicationStatus::Success) = res.Status() {
            let buf = res.Value()?;
            let mut data = SmallVec::from_elem(0, buf.Length()? as usize);
            let reader = DataReader::FromBuffer(&buf)?;
            reader.ReadBytes(data.as_mut_slice())?;
            Ok(data)
        } else {
            Err(Error {
                kind: ErrorKind::AdapterUnavailable,
                message: String::new(),
            })
        }
    }

    /// Write the value of this descriptor on the device to `value`
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(value)?;
        let buf = writer.DetachBuffer()?;
        let res = self.descriptor.WriteValueWithResultAsync(&buf)?.await?;

        match res.Status() {
            Ok(GattCommunicationStatus::Success) => Ok(()),
            _ => Err(Error {
                kind: ErrorKind::AdapterUnavailable,
                message: String::new(),
            }),
        }
    }
}

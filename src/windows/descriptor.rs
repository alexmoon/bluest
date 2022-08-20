use smallvec::SmallVec;
use uuid::Uuid;
use windows::{
    Devices::Bluetooth::{BluetoothCacheMode, GenericAttributeProfile::GattDescriptor},
    Storage::Streams::{DataReader, DataWriter},
};

use crate::Result;

use super::error::check_communication_status;

/// A Bluetooth GATT descriptor
#[derive(Clone, PartialEq, Eq)]
pub struct Descriptor {
    inner: GattDescriptor,
}

impl std::fmt::Debug for Descriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Characteristic")
            .field("uuid", &self.inner.Uuid().unwrap())
            .field("handle", &self.inner.AttributeHandle().unwrap())
            .finish()
    }
}

impl Descriptor {
    pub(super) fn new(descriptor: GattDescriptor) -> Self {
        Descriptor { inner: descriptor }
    }

    /// The [Uuid] identifying the type of descriptor
    pub fn uuid(&self) -> Result<Uuid> {
        Ok(Uuid::from_u128(self.inner.Uuid()?.to_u128()))
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
        let res = self.inner.ReadValueWithCacheModeAsync(cachemode)?.await?;

        check_communication_status(res.Status()?, res.ProtocolError()?, "reading descriptor value")?;

        let buf = res.Value()?;
        let mut data = SmallVec::from_elem(0, buf.Length()? as usize);
        let reader = DataReader::FromBuffer(&buf)?;
        reader.ReadBytes(data.as_mut_slice())?;
        Ok(data)
    }

    /// Write the value of this descriptor on the device to `value`
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(value)?;
        let buf = writer.DetachBuffer()?;
        let res = self.inner.WriteValueWithResultAsync(&buf)?.await?;

        check_communication_status(res.Status()?, res.ProtocolError()?, "writing descriptor value")
    }
}

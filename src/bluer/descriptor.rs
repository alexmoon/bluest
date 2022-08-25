use crate::{Result, Uuid};

/// A Bluetooth GATT descriptor
#[derive(Debug, Clone)]
pub struct Descriptor {
    inner: bluer::gatt::remote::Descriptor,
}

impl PartialEq for Descriptor {
    fn eq(&self, other: &Self) -> bool {
        self.inner.adapter_name() == other.inner.adapter_name()
            && self.inner.device_address() == other.inner.device_address()
            && self.inner.service_id() == other.inner.service_id()
            && self.inner.characteristic_id() == other.inner.characteristic_id()
            && self.inner.id() == other.inner.id()
    }
}

impl Eq for Descriptor {}

impl Descriptor {
    pub(super) fn new(inner: bluer::gatt::remote::Descriptor) -> Self {
        Descriptor { inner }
    }

    /// The [`Uuid`] identifying the type of this GATT descriptor
    pub fn uuid(&self) -> Uuid {
        // This may block the current async executor, but we need this method to be sync for cross-platform compatibility
        futures::executor::block_on(async { self.uuid_async().await.unwrap() })
    }

    /// The [`Uuid`] identifying the type of this GATT descriptor
    ///
    /// # Platform specific
    ///
    /// This method is available on Linux only.
    pub async fn uuid_async(&self) -> Result<Uuid> {
        self.inner.uuid().await.map_err(Into::into)
    }

    /// The cached value of this descriptor
    ///
    /// If the value has not yet been read, this method may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<Vec<u8>> {
        self.inner
            .cached_value()
            .await
            .map_err(Into::into)
            .map(|x| x.into_iter().collect())
    }

    /// Read the value of this descriptor from the device
    pub async fn read(&self) -> Result<Vec<u8>> {
        self.inner
            .read()
            .await
            .map_err(Into::into)
            .map(|x| x.into_iter().collect())
    }

    /// Write the value of this descriptor on the device to `value`
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        self.inner.write(value).await.map_err(Into::into)
    }
}

use bluer::gatt::remote::CharacteristicWriteRequest;
use bluer::gatt::WriteOp;
use futures_util::{Stream, StreamExt};

use super::descriptor::Descriptor;
use crate::{CharacteristicProperties, Result, Uuid};

/// A Bluetooth GATT characteristic
#[derive(Debug, Clone)]
pub struct Characteristic {
    inner: bluer::gatt::remote::Characteristic,
}

impl PartialEq for Characteristic {
    fn eq(&self, other: &Self) -> bool {
        self.inner.adapter_name() == other.inner.adapter_name()
            && self.inner.device_address() == other.inner.device_address()
            && self.inner.service_id() == other.inner.service_id()
            && self.inner.id() == other.inner.id()
    }
}

impl Eq for Characteristic {}

impl Characteristic {
    pub(super) fn new(inner: bluer::gatt::remote::Characteristic) -> Self {
        Characteristic { inner }
    }

    /// The [`Uuid`] identifying the type of this GATT characteristic
    ///
    /// # Panics
    ///
    /// On Linux, this method will panic if there is a current Tokio runtime and it is single-threaded, if there is no
    /// current Tokio runtime and creating one fails, or if the underlying [`Characteristic::uuid_async()`] method
    /// fails.
    pub fn uuid(&self) -> Uuid {
        // Call an async function from a synchronous context
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(move || handle.block_on(self.uuid_async())),
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(self.uuid_async()),
        }
        .unwrap()
    }

    /// The [`Uuid`] identifying the type of this GATT characteristic
    pub async fn uuid_async(&self) -> Result<Uuid> {
        self.inner.uuid().await.map_err(Into::into)
    }

    /// The properties of this this GATT characteristic.
    ///
    /// Characteristic properties indicate which operations (e.g. read, write, notify, etc) may be performed on this
    /// characteristic.
    pub async fn properties(&self) -> Result<CharacteristicProperties> {
        self.inner.flags().await.map(Into::into)
    }

    /// The cached value of this characteristic
    ///
    /// If the value has not yet been read, this method may either return an error or perform a read of the value.
    pub async fn value(&self) -> Result<Vec<u8>> {
        self.inner.cached_value().await.map_err(Into::into)
    }

    /// Read the value of this characteristic from the device
    pub async fn read(&self) -> Result<Vec<u8>> {
        self.inner.read().await.map_err(Into::into)
    }

    /// Write the value of this descriptor on the device to `value` and request the device return a response indicating
    /// a successful write.
    pub async fn write(&self, value: &[u8]) -> Result<()> {
        self.inner.write(value).await.map_err(Into::into)
    }

    /// Write the value of this descriptor on the device to `value` without requesting a response.
    pub async fn write_without_response(&self, value: &[u8]) {
        let _ = self
            .inner
            .write_ext(
                value,
                &CharacteristicWriteRequest {
                    op_type: WriteOp::Command,
                    ..Default::default()
                },
            )
            .await;
    }

    /// Enables notification of value changes for this GATT characteristic.
    ///
    /// Returns a stream of values for the characteristic sent from the device.
    pub async fn notify(&self) -> Result<impl Stream<Item = Result<Vec<u8>>> + '_> {
        Ok(Box::pin(self.inner.notify().await?.map(Ok)))
    }

    /// Is the device currently sending notifications for this characteristic?
    pub async fn is_notifying(&self) -> Result<bool> {
        self.inner.notifying().await.map_err(Into::into)
    }

    /// Discover the descriptors associated with this characteristic.
    pub async fn discover_descriptors(&self) -> Result<Vec<Descriptor>> {
        self.descriptors().await
    }

    /// Get previously discovered descriptors.
    ///
    /// If no descriptors have been discovered yet, this method may either perform descriptor discovery or
    /// return an error.
    pub async fn descriptors(&self) -> Result<Vec<Descriptor>> {
        self.inner
            .descriptors()
            .await
            .map_err(Into::into)
            .map(|x| x.into_iter().map(Descriptor::new).collect())
    }
}

impl From<bluer::gatt::CharacteristicFlags> for CharacteristicProperties {
    fn from(flags: bluer::gatt::CharacteristicFlags) -> Self {
        CharacteristicProperties {
            broadcast: flags.broadcast,
            read: flags.read,
            write_without_response: flags.write_without_response,
            write: flags.write,
            notify: flags.notify,
            indicate: flags.indicate,
            authenticated_signed_writes: flags.authenticated_signed_writes,
            extended_properties: flags.extended_properties,
            reliable_write: flags.reliable_write,
            writable_auxiliaries: flags.writable_auxiliaries,
        }
    }
}

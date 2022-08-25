use futures::pin_mut;
use tokio_stream::StreamExt;

use super::service::Service;
use crate::error::ErrorKind;
use crate::{btuuid, AdvertisementData, Error, ManufacturerData, Result, Uuid};

/// A platform-specific device identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceId(pub(super) bluer::Address);

/// A Bluetooth LE device
#[derive(Debug, Clone)]
pub struct Device {
    pub(super) inner: bluer::Device,
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.inner.adapter_name() == other.inner.adapter_name() && self.inner.address() == other.inner.address()
    }
}

impl Eq for Device {}

impl std::hash::Hash for Device {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.adapter_name().hash(state);
        self.inner.address().hash(state);
    }
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name().as_deref().unwrap_or("(Unknown)"))
    }
}

impl Device {
    pub(super) fn new(adapter: &bluer::Adapter, addr: bluer::Address) -> Result<Self> {
        Ok(Device {
            inner: adapter.device(addr)?,
        })
    }

    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        DeviceId(self.inner.address())
    }

    /// The local name for this device, if available
    pub fn name(&self) -> Option<String> {
        // This may block the current async executor, but we need this method to be sync for cross-platform compatibility
        futures::executor::block_on(self.name_async())
    }

    /// The local name for this device, if available
    ///
    /// # Platform specific
    ///
    /// This method is available on Linux only.
    pub async fn name_async(&self) -> Option<String> {
        self.inner.alias().await.ok()
    }

    /// The connection status for this device
    pub fn is_connected(&self) -> bool {
        // This may block the current async executor, but we need this method to be sync for cross-platform compatibility
        futures::executor::block_on(self.is_connected_async())
    }

    /// The connection status for this device
    ///
    /// # Platform specific
    ///
    /// This method is available on Linux only .
    pub async fn is_connected_async(&self) -> bool {
        self.inner.is_connected().await.unwrap_or(false)
    }

    /// Discover the primary services of this device.
    pub async fn discover_services(&self) -> Result<Vec<Service>> {
        self.services().await
    }

    /// Discover the primary service(s) of this device with the given [Uuid].
    pub async fn discover_services_with_uuid(&self, _uuid: Uuid) -> Result<Vec<Service>> {
        self.services().await
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this method may either perform service discovery or return an error.
    pub async fn services(&self) -> Result<Vec<Service>> {
        Ok(self.inner.services().await?.into_iter().map(Service::new).collect())
    }

    /// Asynchronously blocks until a GATT services changed packet is received
    pub async fn services_changed(&self) -> Result<()> {
        let services = self.services().await?;
        for service in services {
            if service.uuid_async().await? == btuuid::services::GENERIC_ATTRIBUTE {
                for characteristic in service.characteristics().await? {
                    if characteristic.uuid_async().await? == btuuid::characteristics::SERVICE_CHANGED {
                        let notifications = characteristic.notify().await?;
                        pin_mut!(notifications);
                        return match notifications.next().await {
                            Some(Ok(_)) => Ok(()),
                            Some(Err(err)) => Err(err),
                            None => Err(Error::new(
                                ErrorKind::Internal,
                                None,
                                "service changed notifications ended unexpectedly".to_string(),
                            )),
                        };
                    }
                }
            }
        }

        Err(ErrorKind::NotFound.into())
    }

    /// Get the current signal strength from the device in dBm.
    ///
    /// # Platform specific
    ///
    /// This method is available on Linux and MacOS/iOS only.
    pub async fn rssi(&self) -> Result<i16> {
        self.inner.rssi().await?.ok_or_else(|| ErrorKind::NotFound.into())
    }

    pub(super) async fn adv_data(&self) -> AdvertisementData {
        let device = &self.inner;

        let is_connectable = true;

        let local_name = device.alias().await.unwrap_or_default();
        let local_name = (!local_name.is_empty()).then(|| local_name);

        let manufacturer_data = device
            .manufacturer_data()
            .await
            .unwrap_or_default()
            .and_then(|data| data.into_iter().next())
            .map(|(company_id, data)| ManufacturerData { company_id, data });

        let tx_power_level = device.tx_power().await.unwrap_or_default();

        let service_data = device.service_data().await.unwrap_or_default().unwrap_or_default();

        let services = device
            .uuids()
            .await
            .unwrap_or_default()
            .map_or(Vec::new(), |x| x.into_iter().collect());

        AdvertisementData {
            local_name,
            manufacturer_data,
            service_data,
            services,
            tx_power_level,
            is_connectable,
        }
    }
}

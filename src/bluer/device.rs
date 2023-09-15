use futures_util::StreamExt;
use tokio::pin;

use super::adapter::session;
use super::DeviceId;
use crate::error::ErrorKind;
use crate::pairing::PairingAgent;
use crate::{btuuid, AdvertisementData, Device, Error, ManufacturerData, Result, Service, Uuid};

/// A Bluetooth LE device
#[derive(Debug, Clone)]
pub struct DeviceImpl {
    pub(super) inner: bluer::Device,
}

impl PartialEq for DeviceImpl {
    fn eq(&self, other: &Self) -> bool {
        self.inner.adapter_name() == other.inner.adapter_name() && self.inner.address() == other.inner.address()
    }
}

impl Eq for DeviceImpl {}

impl std::hash::Hash for DeviceImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.adapter_name().hash(state);
        self.inner.address().hash(state);
    }
}

impl std::fmt::Display for DeviceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name().as_deref().unwrap_or("(Unknown)"))
    }
}

impl Device {
    pub(super) fn new(adapter: &bluer::Adapter, addr: bluer::Address) -> Result<Device> {
        Ok(Device(DeviceImpl {
            inner: adapter.device(addr)?,
        }))
    }
}

impl DeviceImpl {
    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        DeviceId(self.inner.address())
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    ///
    /// # Panics
    ///
    /// This method will panic if there is a current Tokio runtime and it is single-threaded, if there is no current
    /// Tokio runtime and creating one fails, or if the underlying [`DeviceImpl::name_async()`] method fails.
    pub fn name(&self) -> Result<String> {
        // Call an async function from a synchronous context
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(move || handle.block_on(self.name_async())),
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(self.name_async()),
        }
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub async fn name_async(&self) -> Result<String> {
        self.inner.alias().await.map_err(Into::into)
    }

    /// The connection status for this device
    pub async fn is_connected(&self) -> bool {
        self.inner.is_connected().await.unwrap_or(false)
    }

    /// The pairing status for this device
    pub async fn is_paired(&self) -> Result<bool> {
        self.inner.is_paired().await.map_err(Into::into)
    }

    /// Attempt to pair this device using the system default pairing UI
    pub async fn pair(&self) -> Result<()> {
        if self.is_paired().await? {
            return Ok(());
        }

        self.inner.pair().await.map_err(Into::into)
    }

    /// Attempt to pair this device using the system default pairing UI
    pub async fn pair_with_agent<T: PairingAgent + 'static>(&self, agent: &T) -> Result<()> {
        if self.is_paired().await? {
            return Ok(());
        }

        let agent = {
            // Safety: This `bluer::agent::Agent`, including the encapsulated closures and async blocks will be dropped
            // when the `_handle` below is dropped. Therefore, the lifetime of the captures of `agent` will not
            // out-live the lifetime of `agent`. Unfortunately, the compiler has no way to prove this, so we must cast
            // `agent` to the static lifetime.
            let agent: &'static T = unsafe { std::mem::transmute(agent) };

            async fn req_device(adapter: &str, addr: bluer::Address) -> Result<Device, bluer::agent::ReqError> {
                let session = session().await.map_err(|_| bluer::agent::ReqError::Rejected)?;
                let adapter = session.adapter(adapter).map_err(|_| bluer::agent::ReqError::Rejected)?;
                let device = adapter.device(addr).map_err(|_| bluer::agent::ReqError::Rejected)?;
                Ok(Device(DeviceImpl { inner: device }))
            }

            bluer::agent::Agent {
                request_passkey: Some(Box::new(move |req: bluer::agent::RequestPasskey| {
                    Box::pin(async move {
                        let device = req_device(&req.adapter, req.device).await?;
                        match agent.request_passkey(&device).await {
                            Ok(passkey) => Ok(passkey.into()),
                            Err(_) => Err(bluer::agent::ReqError::Rejected),
                        }
                    })
                })),
                display_passkey: Some(Box::new(move |req: bluer::agent::DisplayPasskey| {
                    Box::pin(async move {
                        let device = req_device(&req.adapter, req.device).await?;
                        if let Ok(passkey) = req.passkey.try_into() {
                            agent.display_passkey(&device, passkey);
                            Ok(())
                        } else {
                            Err(bluer::agent::ReqError::Rejected)
                        }
                    })
                })),
                request_confirmation: Some(Box::new(move |req: bluer::agent::RequestConfirmation| {
                    Box::pin(async move {
                        let device = req_device(&req.adapter, req.device).await?;
                        if let Ok(passkey) = req.passkey.try_into() {
                            agent
                                .confirm_passkey(&device, passkey)
                                .await
                                .map_err(|_| bluer::agent::ReqError::Rejected)
                        } else {
                            Err(bluer::agent::ReqError::Rejected)
                        }
                    })
                })),
                ..Default::default()
            }
        };

        let session = session().await?;
        let _handle = session.register_agent(agent).await?;

        self.pair().await
    }

    /// Disconnect and unpair this device from the system
    pub async fn unpair(&self) -> Result<()> {
        if self.is_connected().await {
            self.inner.disconnect().await?;
        }

        let session = session().await?;
        let adapter = session.adapter(self.inner.adapter_name())?;
        adapter.remove_device(self.inner.address()).await.map_err(Into::into)
    }

    /// Discover the primary services of this device.
    pub async fn discover_services(&self) -> Result<Vec<Service>> {
        self.services().await
    }

    /// Discover the primary service(s) of this device with the given [`Uuid`].
    pub async fn discover_services_with_uuid(&self, _uuid: Uuid) -> Result<Vec<Service>> {
        self.services().await
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this method will perform service discovery.
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
                        pin!(notifications);
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
    /// Returns [ErrorKind::NotSupported].
    pub async fn rssi(&self) -> Result<i16> {
        Err(ErrorKind::NotSupported.into())
    }

    pub(super) async fn adv_data(&self) -> AdvertisementData {
        let device = &self.inner;

        let is_connectable = true;

        let local_name = device.alias().await.unwrap_or_default();
        let local_name = (!local_name.is_empty()).then_some(local_name);

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

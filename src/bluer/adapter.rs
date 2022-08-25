use bluer::{AdapterProperty, Session};
use futures::Stream;
use once_cell::sync::OnceCell;
use tokio_stream::StreamExt;

use super::device::{Device, DeviceId};
use crate::error::ErrorKind;
use crate::{AdapterEvent, AdvertisingDevice, Error, Result, Uuid};

static SESSION: OnceCell<Session> = OnceCell::new();

pub(super) async fn session() -> bluer::Result<&'static Session> {
    if let Some(session) = SESSION.get() {
        Ok(session)
    } else {
        // If called concurrently, this will race but all threads will agree on the result and extra sessions will be
        // dropped.
        let _ = SESSION.set(Session::new().await?);
        Ok(SESSION.get().unwrap())
    }
}

/// The system's Bluetooth adapter interface.
///
/// The default adapter for the system may be accessed with the [Adapter::default()] method.
#[derive(Debug, Clone)]
pub struct Adapter {
    inner: bluer::Adapter,
}

impl PartialEq for Adapter {
    fn eq(&self, other: &Self) -> bool {
        self.inner.name() == other.inner.name()
    }
}

impl Eq for Adapter {}

impl std::hash::Hash for Adapter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.name().hash(state);
    }
}

impl Adapter {
    /// Creates an interface to the default Bluetooth adapter for the system
    pub async fn default() -> Option<Self> {
        session()
            .await
            .ok()?
            .default_adapter()
            .await
            .ok()
            .map(|inner| Adapter { inner })
    }

    /// A stream of [AdapterEvent] which allows the application to identify when the adapter is enabled or disabled.
    pub async fn events(&self) -> Result<impl Stream<Item = Result<AdapterEvent>> + '_> {
        let stream = self.inner.events().await?;
        Ok(stream.filter_map(|event| match event {
            bluer::AdapterEvent::PropertyChanged(AdapterProperty::Powered(true)) => Some(Ok(AdapterEvent::Available)),
            bluer::AdapterEvent::PropertyChanged(AdapterProperty::Powered(false)) => {
                Some(Ok(AdapterEvent::Unavailable))
            }
            _ => None,
        }))
    }

    /// Asynchronously blocks until the adapter is available
    pub async fn wait_available(&self) -> Result<()> {
        let events = self.events();
        if !self.inner.is_powered().await? {
            events
                .await?
                .skip_while(|x| x.is_ok() && !matches!(x, Ok(AdapterEvent::Available)))
                .next()
                .await
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::Internal,
                        None,
                        "adapter event stream closed unexpectedly".to_string(),
                    )
                })??;
        }
        Ok(())
    }

    /// Attempts to create the device identified by `id`
    pub async fn open_device(&self, id: DeviceId) -> Result<Device> {
        Device::new(&self.inner, id.0)
    }

    /// Finds all connected Bluetooth LE devices
    pub async fn connected_devices(&self) -> Result<Vec<Device>> {
        let mut devices = Vec::new();
        for device in self
            .inner
            .device_addresses()
            .await?
            .into_iter()
            .filter_map(|addr| Device::new(&self.inner, addr).ok())
        {
            if device.is_connected_async().await {
                devices.push(device);
            }
        }

        Ok(devices)
    }

    /// Finds all connected devices providing any service in `services`
    ///
    /// # Panics
    ///
    /// Panics if `services` is empty.
    pub async fn connected_devices_with_services(&self, services: &[Uuid]) -> Result<Vec<Device>> {
        assert!(!services.is_empty());

        let devices = self.connected_devices().await?;
        let mut res = Vec::new();
        for device in devices {
            for service in device.inner.services().await? {
                if services.contains(&service.uuid().await?) {
                    res.push(device);
                    break;
                }
            }
        }

        Ok(res)
    }

    /// Starts scanning for Bluetooth advertising packets.
    ///
    /// Returns a stream of [AdvertisingDevice] structs which contain the data from the advertising packet and the
    /// [Device] which sent it. Scanning is automatically stopped when the stream is dropped. Inclusion of duplicate
    /// packets is a platform-specific implementation detail.
    pub async fn scan<'a>(&'a self, services: &'a [Uuid]) -> Result<impl Stream<Item = AdvertisingDevice> + 'a> {
        Ok(self
            .inner
            .discover_devices()
            .await?
            .then(move |event| {
                Box::pin(async move {
                    match event {
                        bluer::AdapterEvent::DeviceAdded(addr) => {
                            let device = Device::new(&self.inner, addr).ok()?;
                            let adv_data = device.adv_data().await;
                            let rssi = device.rssi().await.ok();
                            Some(AdvertisingDevice { device, adv_data, rssi })
                        }
                        _ => None,
                    }
                })
            })
            .filter_map(|x| x)
            .filter(|x| services.is_empty() || x.adv_data.services.iter().any(|y| services.contains(y))))
    }

    /// Connects to the [Device]
    pub async fn connect_device(&self, device: &Device) -> Result<()> {
        device.inner.connect().await.map_err(Into::into)
    }

    /// Disconnects from the [Device]
    pub async fn disconnect_device(&self, device: &Device) -> Result<()> {
        device.inner.disconnect().await.map_err(Into::into)
    }
}

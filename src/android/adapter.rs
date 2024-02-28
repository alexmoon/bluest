use futures_core::Stream;
use futures_lite::{stream, StreamExt};
use uuid::Uuid;

use crate::{AdapterEvent, AdvertisingDevice, ConnectionEvent, Device, DeviceId, Result};

#[derive(Clone)]
pub struct AdapterImpl {}
impl AdapterImpl {
    pub async fn default() -> Option<Self> {
        Some(Self {})
    }

    pub(crate) async fn events(&self) -> Result<impl Stream<Item = Result<AdapterEvent>> + Send + Unpin + '_> {
        Ok(stream::empty()) // TODO
    }

    pub async fn wait_available(&self) -> Result<()> {
        Ok(())
    }

    pub async fn open_device(&self, _id: &DeviceId) -> Result<Device> {
        todo!()
    }

    pub async fn connected_devices(&self) -> Result<Vec<Device>> {
        todo!()
    }

    pub async fn connected_devices_with_services(&self, _services: &[Uuid]) -> Result<Vec<Device>> {
        todo!()
    }

    pub async fn scan<'a>(
        &'a self,
        _services: &'a [Uuid],
    ) -> Result<impl Stream<Item = AdvertisingDevice> + Send + Unpin + 'a> {
        Ok(stream::empty()) // TODO
    }

    pub async fn discover_devices<'a>(
        &'a self,
        services: &'a [Uuid],
    ) -> Result<impl Stream<Item = Result<Device>> + Send + Unpin + 'a> {
        let connected = stream::iter(self.connected_devices_with_services(services).await?).map(Ok);

        // try_unfold is used to ensure we do not start scanning until the connected devices have been consumed
        let advertising = Box::pin(stream::try_unfold(None, |state| async {
            let mut stream = match state {
                Some(stream) => stream,
                None => self.scan(services).await?,
            };
            Ok(stream.next().await.map(|x| (x.device, Some(stream))))
        }));

        Ok(connected.chain(advertising))
    }

    pub async fn connect_device(&self, _device: &Device) -> Result<()> {
        // Windows manages the device connection automatically
        Ok(())
    }

    pub async fn disconnect_device(&self, _device: &Device) -> Result<()> {
        // Windows manages the device connection automatically
        Ok(())
    }

    pub async fn device_connection_events<'a>(
        &'a self,
        _device: &'a Device,
    ) -> Result<impl Stream<Item = ConnectionEvent> + Send + Unpin + 'a> {
        Ok(stream::empty()) // TODO
    }
}

impl PartialEq for AdapterImpl {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for AdapterImpl {}

impl std::hash::Hash for AdapterImpl {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

impl std::fmt::Debug for AdapterImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Adapter").finish()
    }
}

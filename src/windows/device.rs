use futures_channel::mpsc;
use futures_util::future::{select, Either};
use futures_util::{pin_mut, StreamExt};
use tracing::error;
use windows::core::{GUID, HSTRING};
use windows::Devices::Bluetooth::{
    BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
};
use windows::Devices::Enumeration::{DevicePairingKinds, DevicePairingRequestedEventArgs};
use windows::Foundation::TypedEventHandler;

use super::error::{check_communication_status, check_pairing_status};
use crate::error::ErrorKind;
use crate::pairing::{IoCapability, PairingAgent, Passkey};
use crate::util::defer;
use crate::{Device, DeviceId, Result, Service, Uuid};

/// A Bluetooth LE device
#[derive(Clone)]
pub struct DeviceImpl {
    inner: BluetoothLEDevice,
}

impl PartialEq for DeviceImpl {
    fn eq(&self, other: &Self) -> bool {
        self.inner.DeviceId() == other.inner.DeviceId()
    }
}

impl Eq for DeviceImpl {}

impl std::hash::Hash for DeviceImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.DeviceId().unwrap().to_os_string().hash(state);
    }
}

impl std::fmt::Debug for DeviceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("Device");
        f.field("id", &self.id());
        if let Ok(name) = self.name() {
            f.field("name", &name);
        }
        f.finish()
    }
}

impl std::fmt::Display for DeviceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name().as_deref().unwrap_or("(Unknown)"))
    }
}

impl Device {
    pub(super) async fn from_addr(addr: u64, kind: BluetoothAddressType) -> windows::core::Result<Self> {
        let inner = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, kind)?.await?;
        Ok(Device(DeviceImpl { inner }))
    }

    pub(super) async fn from_id(id: &HSTRING) -> windows::core::Result<Self> {
        let inner = BluetoothLEDevice::FromIdAsync(id)?.await?;
        Ok(Device(DeviceImpl { inner }))
    }
}

impl DeviceImpl {
    /// This device's unique identifier
    pub fn id(&self) -> DeviceId {
        super::DeviceId(
            self.inner
                .DeviceId()
                .expect("error getting DeviceId for BluetoothLEDevice")
                .to_os_string(),
        )
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub fn name(&self) -> Result<String> {
        let name = self.inner.Name()?;
        Ok(name.to_string_lossy())
    }

    /// The local name for this device, if available
    ///
    /// This can either be a name advertised or read from the device, or a name assigned to the device by the OS.
    pub async fn name_async(&self) -> Result<String> {
        self.name()
    }

    /// The connection status for this device
    pub async fn is_connected(&self) -> bool {
        self.inner.ConnectionStatus() == Ok(BluetoothConnectionStatus::Connected)
    }

    /// The pairing status for this device
    pub async fn is_paired(&self) -> Result<bool> {
        self.inner
            .DeviceInformation()?
            .Pairing()?
            .IsPaired()
            .map_err(Into::into)
    }

    /// Attempt to pair this device using the system default pairing UI
    ///
    /// This will fail unless it is called from a UWP application.
    pub async fn pair(&self) -> Result<()> {
        let op = self.inner.DeviceInformation()?.Pairing()?.PairAsync()?;
        let res = op.await?;
        check_pairing_status(res.Status()?)
    }

    /// Attempt to pair this device using the system default pairing UI
    pub async fn pair_with_agent<T: PairingAgent>(&self, agent: &T) -> Result<()> {
        let pairing_kinds_supported = match agent.io_capability() {
            IoCapability::DisplayOnly => DevicePairingKinds::DisplayPin,
            IoCapability::DisplayYesNo => {
                DevicePairingKinds::ConfirmOnly | DevicePairingKinds::DisplayPin | DevicePairingKinds::ConfirmPinMatch
            }
            IoCapability::KeyboardOnly => DevicePairingKinds::ConfirmOnly | DevicePairingKinds::ProvidePin,
            IoCapability::NoInputNoOutput => DevicePairingKinds::ConfirmOnly,
            IoCapability::KeyboardDisplay => {
                DevicePairingKinds::ConfirmOnly
                    | DevicePairingKinds::DisplayPin
                    | DevicePairingKinds::ProvidePin
                    | DevicePairingKinds::ConfirmPinMatch
            }
        };

        let (mut tx, mut rx) = mpsc::channel(1);
        let id = self.id();
        let custom = self.inner.DeviceInformation()?.Pairing()?.Custom()?;
        custom.PairingRequested(&TypedEventHandler::new(
            move |_custom, event_args: &Option<DevicePairingRequestedEventArgs>| {
                if let Some(event_args) = event_args.clone() {
                    let deferral = event_args.GetDeferral()?;
                    let _ = tx.try_send((event_args, deferral));
                }
                Ok(())
            },
        ))?;

        let mut op = custom.PairAsync(pairing_kinds_supported)?;
        let res = loop {
            match select(op, rx.next()).await {
                Either::Left((res, _)) => break res,
                Either::Right((Some((event_args, deferral)), pair_op)) => {
                    let id = id.clone();
                    let agent_fut = async move {
                        match event_args.PairingKind()? {
                            DevicePairingKinds::ConfirmOnly => {
                                if agent.confirm(&id).await.is_ok() {
                                    event_args.Accept()?;
                                }
                            }
                            DevicePairingKinds::DisplayPin => {
                                if let Ok(passkey) = event_args.Pin()?.to_string_lossy().parse::<Passkey>() {
                                    agent.display_passkey(&id, passkey);
                                }
                            }
                            DevicePairingKinds::ProvidePin => {
                                if let Ok(passkey) = agent.request_passkey(&id).await {
                                    event_args.AcceptWithPin(&passkey.to_string().into())?;
                                }
                            }
                            DevicePairingKinds::ConfirmPinMatch => {
                                if let Ok(passkey) = event_args.Pin()?.to_string_lossy().parse::<Passkey>() {
                                    if let Ok(()) = agent.confirm_passkey(&id, passkey).await {
                                        event_args.Accept()?;
                                    }
                                }
                            }
                            _ => (),
                        }

                        deferral.Complete().map_err(Into::into)
                    };
                    pin_mut!(agent_fut);

                    match select(pair_op, agent_fut).await {
                        Either::Left((res, _)) => break res,
                        Either::Right((Ok(()), pair_op)) => {
                            op = pair_op;
                        }
                        Either::Right((Err(err), pair_op)) => {
                            let _ = pair_op.Cancel();
                            return Err(err);
                        }
                    }
                }
                Either::Right((None, op)) => break op.await,
            }
        }?;

        check_pairing_status(res.Status()?)
    }

    /// Discover the primary services of this device.
    pub async fn discover_services(&self) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Discover the primary service(s) of this device with the given [`Uuid`].
    pub async fn discover_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetGattServicesForUuidWithCacheModeAsync(GUID::from_u128(uuid.as_u128()), BluetoothCacheMode::Uncached)?
            .await?;

        check_communication_status(res.Status()?, res.ProtocolError(), "discovering services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Get previously discovered services.
    ///
    /// If no services have been discovered yet, this method may either perform service discovery or return an empty
    /// set.
    pub async fn services(&self) -> Result<Vec<Service>> {
        let res = self
            .inner
            .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Cached)?
            .await?;
        check_communication_status(res.Status()?, res.ProtocolError(), "discovering services")?;
        let services = res.Services()?;
        Ok(services.into_iter().map(Service::new).collect())
    }

    /// Asynchronously blocks until a GATT services changed packet is received
    pub async fn services_changed(&self) -> Result<()> {
        let (sender, receiver) = futures_channel::oneshot::channel();
        let mut sender = Some(sender);
        let token = self.inner.GattServicesChanged(&TypedEventHandler::new(move |_, _| {
            if let Some(sender) = sender.take() {
                let _ = sender.send(());
            }
            Ok(())
        }))?;

        let _guard = defer(move || {
            if let Err(err) = self.inner.RemoveGattServicesChanged(token) {
                error!("Error removing state changed handler: {:?}", err);
            }
        });

        receiver.await.unwrap();
        Ok(())
    }

    /// Get the current signal strength from the device in dBm.
    ///
    /// Returns [ErrorKind::NotSupported].
    pub async fn rssi(&self) -> Result<i16> {
        Err(ErrorKind::NotSupported.into())
    }
}

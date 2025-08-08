use futures_core::Stream;
use futures_lite::StreamExt;
use java_spaghetti::Global;
use std::sync::{Arc, OnceLock};
use tracing::info;
use uuid::Uuid;

use crate::{error::ErrorKind, pairing::PairingAgent, DeviceId, Error, Result, Service, ServicesChanged};

use super::bindings::android::bluetooth::BluetoothDevice;
use super::event_receiver::GlobalEvent;
use super::gatt_tree::{CachedWeak, GattConnection, GattTree};
use super::jni::Monitor;
#[cfg(feature = "l2cap")]
use super::l2cap_channel::{L2capChannelReader, L2capChannelWriter};
use super::service::ServiceImpl;
use super::vm_context::{android_api_level, jni_with_env};
use super::{BoolExt, OptionExt};

#[derive(Clone)]
pub struct DeviceImpl {
    pub(super) id: DeviceId,
    pub(super) device: Global<BluetoothDevice>,
    pub(super) connection: CachedWeak<GattConnection>,
    pub(super) once_connected: Arc<OnceLock<()>>,
}

impl PartialEq for DeviceImpl {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for DeviceImpl {}

impl std::hash::Hash for DeviceImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl std::fmt::Debug for DeviceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("Device");
        f.field("name", &self.name().unwrap_or("(Unknown name)".into()));
        f.field("id", &self.id());
        f.finish()
    }
}

impl std::fmt::Display for DeviceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name().as_deref().unwrap_or("(Unknown name)"))
    }
}

impl DeviceImpl {
    pub fn id(&self) -> DeviceId {
        self.id.clone()
    }

    pub fn name(&self) -> Result<String> {
        jni_with_env(|env| {
            self.device
                .as_ref(env)
                .getName()
                .map_err(|e| Error::new(ErrorKind::Internal, None, format!("getName threw: {e:?}")))
                .and_then(|s| s.non_null())
                .map(|s| s.to_string_lossy())
        })
    }

    pub async fn name_async(&self) -> Result<String> {
        self.name()
    }

    pub async fn is_connected(&self) -> bool {
        self.connection.get().is_some()
    }

    pub async fn is_paired(&self) -> Result<bool> {
        jni_with_env(|env| {
            self.device
                .as_ref(env)
                .getBondState()
                .map_err(|e| Error::new(ErrorKind::Internal, None, format!("getBondState threw: {e:?}")))
                .map(|i| i == BluetoothDevice::BOND_BONDED)
        })
    }

    pub async fn pair(&self) -> Result<()> {
        let conn = self.get_connection()?;
        let mut receiver = self.get_connection()?.global_event_receiver.subscribe().await?;

        let bond_state = jni_with_env(|env| {
            let device = self.device.as_ref(env);
            device.getBondState().map_err(crate::Error::from)
        })?;
        match bond_state {
            BluetoothDevice::BOND_BONDED => return Ok(()),
            BluetoothDevice::BOND_BONDING => (),
            _ => {
                jni_with_env(|env| {
                    let device = self.device.as_ref(env);
                    let gatt = conn.gatt.as_ref(env);
                    let _lock = Monitor::new(&gatt);
                    device.createBond()?.non_false()?;
                    Ok::<_, crate::Error>(())
                })?;
            }
        }

        // Inspired by <https://github.com/NordicSemiconductor/Android-BLE-Library>, BleManagerHandler.java
        while let Some(event) = receiver.next().await {
            match event {
                GlobalEvent::BondStateChanged(dev_id, prev_st, st) if dev_id == self.id => match st {
                    BluetoothDevice::BOND_BONDED => return Ok(()),
                    BluetoothDevice::BOND_NONE => {
                        if prev_st == BluetoothDevice::BOND_BONDING {
                            return Err(crate::Error::new(
                                ErrorKind::NotAuthorized,
                                None,
                                "pairing process failed",
                            ));
                        } else if prev_st == BluetoothDevice::BOND_BONDED {
                            info!("deregistered connection with {dev_id} in Device::pair");
                            GattTree::deregister_connection(&dev_id);
                            return Err(ErrorKind::NotConnected.into());
                        }
                    }
                    _ => (),
                },
                _ => (),
            }
        }
        Err(ErrorKind::NotConnected.into())
    }

    pub async fn pair_with_agent<T: PairingAgent + 'static>(&self, _agent: &T) -> Result<()> {
        Err(Error::new(
            ErrorKind::NotSupported,
            None,
            "Android does not support custom pairing agent",
        ))
    }

    pub async fn unpair(&self) -> Result<()> {
        Err(Error::new(
            ErrorKind::NotSupported,
            None,
            "Android might not allow bluetooth device unpairing in an application",
        ))
    }

    pub async fn discover_services(&self) -> Result<Vec<Service>> {
        let conn = self.get_connection()?;
        let disc_lock = conn.discover_services.lock().await;
        jni_with_env(|env| {
            let gatt = conn.gatt.as_ref(env);
            let gatt = Monitor::new(&gatt);
            gatt.discoverServices()?.non_false()?;
            Ok::<_, crate::Error>(())
        })?;
        disc_lock.wait_unlock().await.ok_or_check_conn(&self.id)??;
        self.services().await
    }

    pub async fn discover_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        Ok(self
            .discover_services()
            .await?
            .into_iter()
            .filter(|serv| serv.0.uuid() == uuid)
            .collect())
    }

    pub async fn services(&self) -> Result<Vec<Service>> {
        Ok(self
            .get_connection()?
            .services
            .lock()
            .unwrap()
            .keys()
            .map(|&service_id| crate::Service(ServiceImpl::new(self.id.clone(), service_id)))
            .collect())
    }

    pub async fn service_changed_indications(
        &self,
    ) -> Result<impl Stream<Item = Result<ServicesChanged>> + Send + Unpin + '_> {
        if android_api_level() < 31 {
            return Err(crate::Error::new(
                ErrorKind::NotSupported,
                None,
                "this requires BluetoothGattCallback.onServiceChanged() introduced in API level 31",
            ));
        }
        Ok(self
            .get_connection()?
            .services_changes
            .subscribe(|| Ok::<_, crate::Error>(()), || ())
            .await?
            .map(|_| {
                Ok(ServicesChanged(ServicesChangedImpl {
                    dev_id: self.id.clone(),
                }))
            }))
    }

    pub async fn rssi(&self) -> Result<i16> {
        let conn = self.get_connection()?;
        let read_rssi_lock = conn.read_rssi.lock().await;
        jni_with_env(|env| {
            let gatt = conn.gatt.as_ref(env);
            let gatt = Monitor::new(&gatt);
            gatt.readRemoteRssi()?.non_false()?;
            Ok::<_, crate::Error>(())
        })?;
        Ok(read_rssi_lock.wait_unlock().await.ok_or_check_conn(&self.id)??)
    }

    #[cfg(feature = "l2cap")]
    pub async fn open_l2cap_channel(
        &self,
        psm: u16,
        secure: bool,
    ) -> std::prelude::v1::Result<(L2capChannelReader, L2capChannelWriter), crate::Error> {
        super::l2cap_channel::open_l2cap_channel(self.device.clone(), psm, secure)
    }

    fn get_connection(&self) -> Result<Arc<GattConnection>, crate::Error> {
        self.connection
            .get_or_find(|| GattTree::find_connection(&self.id).ok_or(ErrorKind::NotConnected.into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServicesChangedImpl {
    dev_id: DeviceId, // XXX: this is not enough for a unique hash value
}

impl ServicesChangedImpl {
    pub fn was_invalidated(&self, service: &Service) -> bool {
        GattTree::find_service(&self.dev_id, service.uuid()).is_none()
    }
}

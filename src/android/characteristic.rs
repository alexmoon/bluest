use std::sync::Arc;

use futures_core::Stream;
use futures_lite::StreamExt;
use java_spaghetti::ByteArray;
use uuid::Uuid;

use super::bindings::android::bluetooth::BluetoothGattCharacteristic;
use super::descriptor::DescriptorImpl;
use super::gatt_tree::{CachedWeak, CharacteristicInner, GattTree};
use super::jni::{ByteArrayExt, Monitor};
use super::vm_context::{android_api_level, jni_with_env};
use super::{BoolExt, IntExt, OptionExt};
use crate::error::ErrorKind;
use crate::{CharacteristicProperties, Descriptor, DeviceId, Result};

#[derive(Debug, Clone)]
pub struct CharacteristicImpl {
    dev_id: DeviceId,
    service_id: Uuid,
    char_id: Uuid,
    inner: CachedWeak<CharacteristicInner>,
}

impl PartialEq for CharacteristicImpl {
    fn eq(&self, other: &Self) -> bool {
        self.dev_id == other.dev_id && self.service_id == other.service_id && self.char_id == other.char_id
    }
}

impl Eq for CharacteristicImpl {}

impl std::hash::Hash for CharacteristicImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.dev_id.hash(state);
        self.service_id.hash(state);
        self.char_id.hash(state);
    }
}

impl CharacteristicImpl {
    pub(crate) fn new(dev_id: DeviceId, service_id: Uuid, char_id: Uuid) -> Self {
        Self {
            dev_id,
            service_id,
            char_id,
            inner: CachedWeak::new(),
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.char_id
    }

    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(self.char_id)
    }

    pub async fn properties(&self) -> Result<CharacteristicProperties> {
        jni_with_env(|env| {
            let val = self.get_inner()?.char.as_ref(env).getProperties()?;
            Ok(CharacteristicProperties::from_bits(val.cast_unsigned()))
        })
    }

    pub async fn value(&self) -> Result<Vec<u8>> {
        Ok(self.get_inner()?.read.last_value().ok_or(crate::Error::new(
            ErrorKind::NotReady,
            None,
            "please call `Characteristic::read` at first",
        ))??)
    }

    pub async fn read(&self) -> Result<Vec<u8>> {
        let conn = GattTree::find_connection(&self.dev_id).ok_or_check_conn(&self.dev_id)?;
        let inner = self.get_inner()?;
        let read_lock = inner.read.lock().await;
        let _write_lock = inner.write.lock().await;
        jni_with_env(|env| {
            let gatt = &conn.gatt.as_ref(env);
            let gatt = Monitor::new(gatt);
            gatt.readCharacteristic(inner.char.as_ref(env))
                .map_err(|e| e.into())
                .and_then(|b| b.non_false())
        })?;
        Ok(read_lock.wait_unlock().await.ok_or_check_conn(&self.dev_id)??)
    }

    pub async fn write(&self, value: &[u8]) -> Result<()> {
        self.write_internal(value, true).await
    }

    pub async fn write_without_response(&self, value: &[u8]) -> Result<()> {
        self.write_internal(value, false).await
    }

    async fn write_internal(&self, value: &[u8], with_response: bool) -> Result<()> {
        let conn = GattTree::find_connection(&self.dev_id).ok_or_check_conn(&self.dev_id)?;
        let inner = self.get_inner()?;
        let _read_lock = inner.read.lock().await;
        let write_lock = inner.write.lock().await;
        jni_with_env(|env| {
            let gatt = conn.gatt.as_ref(env);
            let gatt = Monitor::new(&gatt);
            let char = inner.char.as_ref(env);
            let array = ByteArray::from_slice(env, value);
            let write_type = if with_response {
                BluetoothGattCharacteristic::WRITE_TYPE_DEFAULT
            } else {
                BluetoothGattCharacteristic::WRITE_TYPE_NO_RESPONSE
            };
            char.setWriteType(write_type)?;
            if android_api_level() >= 33 {
                gatt.writeCharacteristic_BluetoothGattCharacteristic_byte_array_int(char, array, write_type)?
                    .check_status_code()
            } else {
                #[allow(deprecated)]
                char.setValue_byte_array(array)?;
                #[allow(deprecated)]
                gatt.writeCharacteristic_BluetoothGattCharacteristic(char)
                    .map_err(|e| e.into())
                    .and_then(|b| b.non_false())
            }
        })?;
        Ok(write_lock.wait_unlock().await.ok_or_check_conn(&self.dev_id)??)
    }

    pub fn max_write_len(&self) -> Result<usize> {
        // XXX: call `BluetoothGatt.requetMtu(int)` on connection and get the value in `onMtuChanged`.
        // See <https://developer.android.com/about/versions/14/behavior-changes-all#mtu-set-to-517>.
        Ok(23 - 5)
    }

    pub async fn max_write_len_async(&self) -> Result<usize> {
        self.max_write_len()
    }

    pub async fn notify(&self) -> Result<impl Stream<Item = Result<Vec<u8>>> + Send + Unpin + '_> {
        let conn = GattTree::find_connection(&self.dev_id).ok_or_check_conn(&self.dev_id)?;
        let inner = self.get_inner()?;
        let inner_2 = inner.clone();
        let (gatt_for_stop, char_for_stop) = (conn.gatt.clone(), inner.char.clone());
        inner
            .notify
            .subscribe(
                move || {
                    jni_with_env(|env| {
                        let gatt = conn.gatt.as_ref(env);
                        let gatt = Monitor::new(&gatt);
                        let result = gatt.setCharacteristicNotification(inner_2.char.as_ref(env), true)?;
                        result.non_false()
                    })
                },
                move || {
                    jni_with_env(|env| {
                        let gatt = gatt_for_stop.as_ref(env);
                        let gatt = Monitor::new(&gatt);
                        let _ = gatt.setCharacteristicNotification(char_for_stop.as_ref(env), false);
                    })
                },
            )
            .await
            .map(|fut| fut.map(Ok))
    }

    pub async fn is_notifying(&self) -> Result<bool> {
        Ok(self.get_inner()?.notify.is_notifying())
    }

    pub async fn discover_descriptors(&self) -> Result<Vec<Descriptor>> {
        self.descriptors().await
    }

    pub async fn descriptors(&self) -> Result<Vec<Descriptor>> {
        Ok(self
            .get_inner()?
            .descs
            .keys()
            .map(|id| {
                Descriptor(DescriptorImpl::new(
                    self.dev_id.clone(),
                    self.service_id,
                    self.char_id,
                    *id,
                ))
            })
            .collect())
    }

    fn get_inner(&self) -> Result<Arc<CharacteristicInner>, crate::Error> {
        self.inner.get_or_find(|| {
            GattTree::find_characteristic(&self.dev_id, self.service_id, self.char_id).ok_or_check_conn(&self.dev_id)
        })
    }
}

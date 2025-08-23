use std::sync::Arc;

use java_spaghetti::ByteArray;

use super::gatt_tree::{CachedWeak, DescriptorInner, GattTree};
use super::jni::{ByteArrayExt, Monitor};
use super::vm_context::{android_api_level, jni_with_env};
use super::{BoolExt, IntExt, OptionExt};
use crate::error::ErrorKind;
use crate::{DeviceId, Result, Uuid};

#[derive(Debug, Clone)]
pub struct DescriptorImpl {
    dev_id: DeviceId,
    service_id: Uuid,
    char_id: Uuid,
    desc_id: Uuid,
    inner: CachedWeak<DescriptorInner>,
}

impl PartialEq for DescriptorImpl {
    fn eq(&self, other: &Self) -> bool {
        self.dev_id == other.dev_id
            && self.service_id == other.service_id
            && self.char_id == other.char_id
            && self.desc_id == other.desc_id
    }
}

impl Eq for DescriptorImpl {}

impl DescriptorImpl {
    pub(crate) fn new(dev_id: DeviceId, service_id: Uuid, char_id: Uuid, desc_id: Uuid) -> Self {
        Self {
            dev_id,
            service_id,
            char_id,
            desc_id,
            inner: CachedWeak::new(),
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.desc_id
    }

    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(self.desc_id)
    }

    pub async fn value(&self) -> Result<Vec<u8>> {
        Ok(self.get_inner()?.read.last_value().ok_or(crate::Error::new(
            ErrorKind::NotReady,
            None,
            "please call `Descriptor::read` at first",
        ))??)
    }

    // NOTE: the sequence of gaining read lock and write lock should be the same
    // in `read` and `write` methods, otherwise deadlock may occur.

    pub async fn read(&self) -> Result<Vec<u8>> {
        let conn = GattTree::find_connection(&self.dev_id).ok_or_check_conn(&self.dev_id)?;
        let inner = self.get_inner()?;
        let read_lock = inner.read.lock().await;
        let _write_lock = inner.write.lock().await;
        jni_with_env(|env| {
            let gatt = &conn.gatt.as_ref(env);
            let gatt = Monitor::new(gatt);
            gatt.readDescriptor(inner.desc.as_ref(env))
                .map_err(|e| e.into())
                .and_then(|b| b.non_false())
        })?;
        Ok(read_lock.wait_unlock().await.ok_or_check_conn(&self.dev_id)??)
    }

    pub async fn write(&self, value: &[u8]) -> Result<()> {
        let conn = GattTree::find_connection(&self.dev_id).ok_or_check_conn(&self.dev_id)?;
        let inner = self.get_inner()?;
        let _read_lock = inner.read.lock().await;
        let write_lock = inner.write.lock().await;
        jni_with_env(|env| {
            let gatt = conn.gatt.as_ref(env);
            let gatt = Monitor::new(&gatt);
            let desc = inner.desc.as_ref(env);
            let array = ByteArray::from_slice(env, value);
            if android_api_level() >= 33 {
                gatt.writeDescriptor_BluetoothGattDescriptor_byte_array(desc, array)?
                    .check_status_code()
            } else {
                #[allow(deprecated)]
                desc.setValue(array)?;
                #[allow(deprecated)]
                gatt.writeDescriptor_BluetoothGattDescriptor(desc)
                    .map_err(|e| e.into())
                    .and_then(|b| b.non_false())
            }
        })?;
        Ok(write_lock.wait_unlock().await.ok_or_check_conn(&self.dev_id)??)
    }

    fn get_inner(&self) -> Result<Arc<DescriptorInner>, crate::Error> {
        self.inner.get_or_find(|| {
            GattTree::find_descriptor(&self.dev_id, self.service_id, self.char_id, self.desc_id)
                .ok_or_check_conn(&self.dev_id)
        })
    }
}

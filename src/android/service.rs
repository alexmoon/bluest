use std::sync::Arc;

use super::bindings::android::bluetooth::BluetoothGattService;
use super::characteristic::CharacteristicImpl;
use super::gatt_tree::{CachedWeak, GattTree, ServiceInner};
use super::vm_context::jni_with_env;
use super::{DeviceId, JavaIterator, OptionExt, UuidExt};
use crate::{Characteristic, Result, Service, Uuid};

#[derive(Debug, Clone)]
pub struct ServiceImpl {
    dev_id: DeviceId,
    service_id: Uuid,
    inner: CachedWeak<ServiceInner>,
}

impl PartialEq for ServiceImpl {
    fn eq(&self, other: &Self) -> bool {
        self.dev_id == other.dev_id && self.service_id == other.service_id
    }
}

impl Eq for ServiceImpl {}

impl std::hash::Hash for ServiceImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.dev_id.hash(state);
        self.service_id.hash(state);
    }
}

impl ServiceImpl {
    pub(crate) fn new(dev_id: DeviceId, service_id: Uuid) -> Self {
        Self {
            dev_id,
            service_id,
            inner: CachedWeak::new(),
        }
    }

    pub fn uuid(&self) -> Uuid {
        self.service_id
    }

    pub async fn uuid_async(&self) -> Result<Uuid> {
        Ok(self.service_id)
    }

    pub async fn is_primary(&self) -> Result<bool> {
        jni_with_env(|env| {
            Ok(self.get_inner()?.service.as_ref(env).getType()? == BluetoothGattService::SERVICE_TYPE_PRIMARY)
        })
    }

    pub async fn discover_characteristics(&self) -> Result<Vec<Characteristic>> {
        self.characteristics().await
    }

    pub async fn discover_characteristics_with_uuid(&self, uuid: Uuid) -> Result<Vec<Characteristic>> {
        Ok(self
            .characteristics()
            .await?
            .into_iter()
            .filter(|ch| ch.0.uuid() == uuid)
            .collect())
    }

    pub async fn characteristics(&self) -> Result<Vec<Characteristic>> {
        Ok(self
            .get_inner()?
            .chars
            .keys()
            .map(|id| Characteristic(CharacteristicImpl::new(self.dev_id.clone(), self.service_id, *id)))
            .collect())
    }

    pub async fn discover_included_services(&self) -> Result<Vec<Service>> {
        self.included_services().await
    }

    pub async fn discover_included_services_with_uuid(&self, uuid: Uuid) -> Result<Vec<Service>> {
        Ok(self
            .included_services()
            .await?
            .into_iter()
            .filter(|ch| ch.0.uuid() == uuid)
            .collect())
    }

    pub async fn included_services(&self) -> Result<Vec<Service>> {
        jni_with_env(|env| {
            let inner = self.get_inner()?;
            let service = inner.service.as_ref(env);
            let includes = service.getIncludedServices()?.non_null()?;
            let vec = JavaIterator(includes.iterator()?.non_null()?)
                .filter_map(|serv| {
                    serv.cast::<BluetoothGattService>()
                        .ok()
                        .and_then(|serv| Uuid::from_java(serv.getUuid().ok()??.as_ref()).ok())
                })
                .map(|uuid| Service(ServiceImpl::new(self.dev_id.clone(), uuid)))
                .collect();
            Ok(vec)
        })
    }

    fn get_inner(&self) -> Result<Arc<ServiceInner>, crate::Error> {
        self.inner
            .get_or_find(|| GattTree::find_service(&self.dev_id, self.service_id).ok_or_check_conn(&self.dev_id))
    }
}

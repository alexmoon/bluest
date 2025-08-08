use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};

use java_spaghetti::{ByteArray, Env, Global, Ref};
use tracing::{error, info};

use super::async_util::{Excluder, ExcluderLock, Notifier};
use super::bindings::android::bluetooth::{
    BluetoothGatt, BluetoothGattCharacteristic, BluetoothGattDescriptor, BluetoothGattService, BluetoothProfile,
};
use super::device::DeviceImpl;
use super::event_receiver::EventReceiver;
use super::jni::{ByteArrayExt, Monitor};
use super::vm_context::{android_api_level, jni_with_env};
use super::{BoolExt, JavaIterator, OptionExt, UuidExt};
use crate::error::AttError;
use crate::{DeviceId, Uuid};

static GATT_CONNECTIONS: LazyLock<Mutex<HashMap<DeviceId, Arc<GattConnection>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) use cached_weak::CachedWeak;
mod cached_weak {
    use std::fmt::Debug;
    use std::sync::atomic::{AtomicPtr, Ordering};
    use std::sync::{Arc, Weak};

    pub struct CachedWeak<T> {
        ptr: AtomicPtr<T>,
    }

    impl<T> CachedWeak<T> {
        fn get_raw(&self) -> *mut T {
            self.ptr.load(Ordering::SeqCst)
        }
        fn get_weak(&self) -> Weak<T> {
            // Safety: the raw pointer is got from `Weak::into_raw`
            let weak = unsafe { Weak::from_raw(self.get_raw()) };
            let weak_cloned = weak.clone();
            let _ = weak.into_raw(); // preserve the ownership of the stored weak
            weak_cloned
        }
        pub fn new() -> Self {
            Self {
                ptr: AtomicPtr::new(Weak::<T>::new().into_raw().cast_mut()),
            }
        }
        pub fn get(&self) -> Option<Arc<T>> {
            self.get_weak().upgrade()
        }
        pub fn get_or_find<E>(&self, finder: impl FnOnce() -> Result<Arc<T>, E>) -> Result<Arc<T>, E> {
            if let Some(arc) = self.get() {
                return Ok(arc);
            }
            let arc = finder()?;
            self.ptr
                .store(Arc::downgrade(&arc).into_raw().cast_mut(), Ordering::SeqCst);
            Ok(arc)
        }
    }

    impl<T> Clone for CachedWeak<T> {
        fn clone(&self) -> Self {
            Self {
                ptr: AtomicPtr::new(self.get_weak().into_raw().cast_mut()),
            }
        }
    }

    impl<T> Debug for CachedWeak<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_fmt(format_args!("CachedWeak {{ ptr: {:?} }}", self.get_raw()))
        }
    }
}

// TODO: remove `E: Clone` in `Excluder`, or make `crate::Error` clonable,
// so that errors other than `AttError` may be returned from the callback.

pub(crate) struct GattConnection {
    pub(super) gatt: Global<BluetoothGatt>,
    pub(super) callback_hdl_weak: Weak<BluetoothGattCallbackProxy>,
    pub(super) global_event_receiver: Arc<EventReceiver>,
    pub(super) services: Mutex<HashMap<Uuid, Arc<ServiceInner>>>,
    pub(super) discover_services: Excluder<Result<(), AttError>>,
    pub(super) read_rssi: Excluder<Result<i16, AttError>>,
    pub(super) services_changes: Notifier<()>,
}

pub(crate) struct ServiceInner {
    pub(super) service: Global<BluetoothGattService>,
    pub(super) chars: HashMap<Uuid, Arc<CharacteristicInner>>,
}

pub(crate) struct CharacteristicInner {
    pub(super) char: Global<BluetoothGattCharacteristic>,
    pub(super) descs: HashMap<Uuid, Arc<DescriptorInner>>,
    pub(super) notify: Notifier<Vec<u8>>,
    pub(super) read: Excluder<Result<Vec<u8>, AttError>>,
    pub(super) write: Excluder<Result<(), AttError>>,
}

pub(crate) struct DescriptorInner {
    pub(super) desc: Global<BluetoothGattDescriptor>,
    pub(super) read: Excluder<Result<Vec<u8>, AttError>>,
    pub(super) write: Excluder<Result<(), AttError>>,
}

/// Manages all existing GATT connections handled by this crate.
pub(crate) struct GattTree;

impl GattTree {
    /// Gets all devices registered here.
    pub fn registered_devices() -> Result<Vec<crate::Device>, crate::Error> {
        let connections = GATT_CONNECTIONS.lock().unwrap();
        let mut devices = Vec::with_capacity(connections.len());
        jni_with_env(|env| {
            for (id, conn) in connections.iter() {
                let cached_weak = CachedWeak::new();
                let _ = cached_weak.get_or_find(|| Ok::<_, ()>(conn.clone()));
                devices.push(crate::Device(DeviceImpl {
                    id: id.clone(),
                    device: conn.gatt.as_ref(env).getDevice()?.non_null()?.as_global(),
                    connection: cached_weak,
                    once_connected: Arc::new(OnceLock::from(())),
                }));
            }
            Ok(devices)
        })
    }

    /// Called from `Adapter::connect_device`.
    pub fn register_connection(
        dev_id: &DeviceId,
        gatt: Global<BluetoothGatt>,
        callback_hdl: &Arc<BluetoothGattCallbackProxy>,
        event_receiver: &Arc<EventReceiver>,
    ) {
        let _ = GATT_CONNECTIONS.lock().unwrap().insert(
            dev_id.clone(),
            Arc::new(GattConnection {
                gatt,
                callback_hdl_weak: Arc::downgrade(callback_hdl),
                global_event_receiver: event_receiver.clone(),
                services: Mutex::new(HashMap::new()),
                discover_services: Excluder::new(),
                read_rssi: Excluder::new(),
                services_changes: Notifier::new(16),
            }),
        );
    }

    /// Call this when the actual disconnection is realized.
    pub fn deregister_connection(dev_id: &DeviceId) -> bool {
        GATT_CONNECTIONS.lock().unwrap().remove(dev_id).is_some()
    }

    /// Call this on adapter disabling event.
    pub fn clear_connections() -> bool {
        let mut conns = GATT_CONNECTIONS.lock().unwrap();
        if !conns.is_empty() {
            conns.clear();
            true
        } else {
            false
        }
    }

    pub fn find_connection(dev_id: &DeviceId) -> Option<Arc<GattConnection>> {
        let conn = GATT_CONNECTIONS.lock().unwrap().get(dev_id).cloned()?;
        if conn.callback_hdl_weak.strong_count() > 0 {
            Some(conn)
        } else {
            Self::deregister_connection(dev_id);
            info!("deregistered connection with {dev_id} in find_connection()");
            None
        }
    }

    pub fn find_service(dev_id: &DeviceId, service_id: Uuid) -> Option<Arc<ServiceInner>> {
        Self::find_connection(dev_id).and_then(|conn| conn.services.lock().unwrap().get(&service_id).cloned())
    }

    pub fn find_characteristic(dev_id: &DeviceId, service_id: Uuid, char_id: Uuid) -> Option<Arc<CharacteristicInner>> {
        Self::find_service(dev_id, service_id).and_then(|service| service.chars.get(&char_id).cloned())
    }

    pub fn find_descriptor(
        dev_id: &DeviceId,
        service_id: Uuid,
        char_id: Uuid,
        desc_id: Uuid,
    ) -> Option<Arc<DescriptorInner>> {
        Self::find_characteristic(dev_id, service_id, char_id).and_then(|char| char.descs.get(&desc_id).cloned())
    }
}

impl GattConnection {
    /// Refresh available services according to the result of `BluetoothGatt.getServices()`.
    /// This does not perform real device discovering.
    pub fn refresh_services(&self) -> Result<(), crate::Error> {
        let mut services = self.services.lock().unwrap();
        let mut current_services_ids = Vec::new();
        jni_with_env(|env| {
            let gatt = self.gatt.as_ref(env);
            let services_obj = gatt.getServices()?.non_null()?;
            let iter = JavaIterator(services_obj.iterator()?.non_null()?);
            for service_obj in iter.filter_map(|o| o.cast::<BluetoothGattService>().ok()) {
                let service_id = Uuid::from_java(service_obj.getUuid()?.non_null()?.as_ref())?;
                current_services_ids.push(service_id);
                if services.get(&service_id).is_none() {
                    services.insert(service_id, Arc::new(construct_service_tree(&service_obj.as_ref())?));
                }
            }
            services.retain(|id, _| current_services_ids.contains(id));
            Ok(())
        })
    }
}

fn construct_service_tree<'env>(service_obj: &Ref<'env, BluetoothGattService>) -> Result<ServiceInner, crate::Error> {
    let chars_obj = service_obj.getCharacteristics()?.non_null()?;
    let iter = JavaIterator(chars_obj.iterator()?.non_null()?);
    let mut chars = HashMap::new();
    for char_obj in iter.filter_map(|o| o.cast::<BluetoothGattCharacteristic>().ok()) {
        let char_id = Uuid::from_java(char_obj.getUuid()?.non_null()?.as_ref())?;
        let descs_obj = char_obj.getDescriptors()?.non_null()?;
        let iter = JavaIterator(descs_obj.iterator()?.non_null()?);
        let mut descs = HashMap::new();
        for desc_obj in iter.filter_map(|o| o.cast::<BluetoothGattDescriptor>().ok()) {
            let desc_id = Uuid::from_java(desc_obj.getUuid()?.non_null()?.as_ref())?;
            descs.insert(
                desc_id,
                Arc::new(DescriptorInner {
                    desc: desc_obj.as_global(),
                    read: Excluder::new(),
                    write: Excluder::new(),
                }),
            );
        }
        chars.insert(
            char_id,
            Arc::new(CharacteristicInner {
                char: char_obj.as_global(),
                descs,
                notify: Notifier::new(128),
                read: Excluder::new(),
                write: Excluder::new(),
            }),
        );
    }
    Ok(ServiceInner {
        service: service_obj.as_global(),
        chars,
    })
}

fn callback_find_char(
    dev_id: &DeviceId,
    char: &Option<Ref<'_, BluetoothGattCharacteristic>>,
) -> Option<Arc<CharacteristicInner>> {
    let service_id = Uuid::from_java(char.as_ref()?.getService().ok()??.getUuid().ok()??.as_ref()).ok()?;
    let char_id = Uuid::from_java(char.as_ref()?.getUuid().ok()??.as_ref()).ok()?;
    GattTree::find_characteristic(dev_id, service_id, char_id)
}

fn callback_find_desc(
    dev_id: &DeviceId,
    desc: &Option<Ref<'_, BluetoothGattDescriptor>>,
) -> Option<Arc<DescriptorInner>> {
    let char = desc.as_ref()?.getCharacteristic().ok()??;
    let char = callback_find_char(dev_id, &Some(char.as_ref()))?;
    let desc_id = Uuid::from_java(desc.as_ref()?.getUuid().ok()??.as_ref()).ok()?;
    char.descs.get(&desc_id).cloned()
}

pub struct BluetoothGattCallbackProxy {
    dev_id: DeviceId,
    discover_services_on_change: Mutex<Option<ExcluderLock<Result<(), AttError>>>>,
}

impl BluetoothGattCallbackProxy {
    pub fn new(dev_id: DeviceId) -> Arc<Self> {
        Arc::new(Self {
            dev_id,
            discover_services_on_change: Mutex::new(None),
        })
    }
}

impl super::callback::BluetoothGattCallbackProxy for BluetoothGattCallbackProxy {
    fn onPhyUpdate<'env>(&self, _: Env<'env>, _: Option<Ref<'env, BluetoothGatt>>, _: i32, _: i32, _: i32) {}
    fn onPhyRead<'env>(&self, _: Env<'env>, _: Option<Ref<'env, BluetoothGatt>>, _: i32, _: i32, _: i32) {}

    fn onConnectionStateChange<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        _status: i32,
        new_state: i32,
    ) {
        if new_state == BluetoothProfile::STATE_DISCONNECTED {
            // no reconnection with the same BluetoothGatt object
            if GattTree::deregister_connection(&self.dev_id) {
                info!(
                    "deregistered connection with {} in onConnectionStateChange()",
                    &self.dev_id
                );
            }
        }
    }

    fn onServicesDiscovered<'env>(&self, _env: Env<'env>, _gatt: Option<Ref<'env, BluetoothGatt>>, status: i32) {
        let Some(conn) = GattTree::find_connection(&self.dev_id) else {
            return;
        };
        if let Err(e) = conn.refresh_services() {
            error!("refresh_services failed during onServicesDiscovered(): {e}");
        }
        let status = gatt_error_check(status);
        if let Err(e) = status {
            error!("onServicesDiscovered() with error status: {e}");
        }
        conn.discover_services.unlock(status);

        // see onServiceChanged().
        let _ = self.discover_services_on_change.lock().unwrap().take();
        conn.services_changes.notify(());
    }

    fn onCharacteristicRead_BluetoothGatt_BluetoothGattCharacteristic_int<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        char: Option<Ref<'env, BluetoothGattCharacteristic>>,
        status: i32,
    ) {
        if android_api_level() >= 33 {
            return;
        }
        // XXX: is this thread-safe?
        #[allow(deprecated)]
        let get_data = || Some(char.as_ref()?.getValue().ok()??.as_vec_u8());
        let data = get_data().unwrap_or_default();
        let Some(char) = callback_find_char(&self.dev_id, &char) else {
            return;
        };
        char.read.unlock(gatt_error_check(status).map(|_| data));
    }

    fn onCharacteristicRead_BluetoothGatt_BluetoothGattCharacteristic_byte_array_int<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        char: Option<Ref<'env, BluetoothGattCharacteristic>>,
        data: Option<Ref<'env, ByteArray>>,
        status: i32,
    ) {
        let Some(char) = callback_find_char(&self.dev_id, &char) else {
            return;
        };
        char.read
            .unlock(gatt_error_check(status).map(|_| data.map(|jarr| jarr.as_vec_u8()).unwrap_or_default()));
    }

    fn onCharacteristicWrite<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        char: Option<Ref<'env, BluetoothGattCharacteristic>>,
        status: i32,
    ) {
        let Some(char) = callback_find_char(&self.dev_id, &char) else {
            return;
        };
        char.write.unlock(gatt_error_check(status));
    }

    fn onCharacteristicChanged_BluetoothGatt_BluetoothGattCharacteristic<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        char: Option<Ref<'env, BluetoothGattCharacteristic>>,
    ) {
        if android_api_level() >= 33 {
            return;
        }
        // XXX: is this thread-safe?
        #[allow(deprecated)]
        let get_data = || Some(char.as_ref()?.getValue().ok()??.as_vec_u8());
        let data = get_data().unwrap_or_default();
        let Some(char) = callback_find_char(&self.dev_id, &char) else {
            return;
        };
        char.notify.notify(data);
    }

    fn onCharacteristicChanged_BluetoothGatt_BluetoothGattCharacteristic_byte_array<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        char: Option<Ref<'env, BluetoothGattCharacteristic>>,
        data: Option<Ref<'env, ByteArray>>,
    ) {
        let Some(char) = callback_find_char(&self.dev_id, &char) else {
            return;
        };
        char.notify
            .notify(data.map(|jarr| jarr.as_vec_u8()).unwrap_or_default());
    }

    fn onDescriptorRead_BluetoothGatt_BluetoothGattDescriptor_int<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        desc: Option<Ref<'env, BluetoothGattDescriptor>>,
        status: i32,
    ) {
        if android_api_level() >= 33 {
            return;
        }
        // XXX: is this thread-safe?
        #[allow(deprecated)]
        let get_data = || Some(desc.as_ref()?.getValue().ok()??.as_vec_u8());
        let data = get_data().unwrap_or_default();
        let Some(desc) = callback_find_desc(&self.dev_id, &desc) else {
            return;
        };
        desc.read.unlock(gatt_error_check(status).map(|_| data));
    }

    fn onDescriptorRead_BluetoothGatt_BluetoothGattDescriptor_int_byte_array<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        desc: Option<Ref<'env, BluetoothGattDescriptor>>,
        status: i32,
        data: Option<Ref<'env, ByteArray>>,
    ) {
        let Some(desc) = callback_find_desc(&self.dev_id, &desc) else {
            return;
        };
        desc.read
            .unlock(gatt_error_check(status).map(|_| data.map(|jarr| jarr.as_vec_u8()).unwrap_or_default()));
    }

    fn onDescriptorWrite<'env>(
        &self,
        _env: Env<'env>,
        _gatt: Option<Ref<'env, BluetoothGatt>>,
        desc: Option<Ref<'env, BluetoothGattDescriptor>>,
        status: i32,
    ) {
        let Some(desc) = callback_find_desc(&self.dev_id, &desc) else {
            return;
        };
        desc.write.unlock(gatt_error_check(status));
    }

    fn onReliableWriteCompleted<'env>(&self, _env: Env<'env>, _arg0: Option<Ref<'env, BluetoothGatt>>, _arg1: i32) {}

    fn onReadRemoteRssi<'env>(&self, _env: Env<'env>, _gatt: Option<Ref<'env, BluetoothGatt>>, rssi: i32, status: i32) {
        let Some(conn) = GattTree::find_connection(&self.dev_id) else {
            return;
        };
        conn.read_rssi.unlock(gatt_error_check(status).map(|_| rssi as _));
    }

    fn onMtuChanged<'env>(&self, _env: Env<'env>, _arg0: Option<Ref<'env, BluetoothGatt>>, _arg1: i32, _arg2: i32) {}

    fn onServiceChanged<'env>(&self, _env: Env<'env>, gatt: Option<Ref<'env, BluetoothGatt>>) {
        let Some(conn) = GattTree::find_connection(&self.dev_id) else {
            return;
        };
        if let Some(disc_lock) = conn.discover_services.try_lock() {
            let gatt = Monitor::new(gatt.as_ref().unwrap());
            if let Err(e) = gatt
                .discoverServices()
                .map_err(|e| e.into())
                .and_then(|res| res.non_false())
            {
                error!("failed to call BluetoothGatt.discoverServices() on onServiceChanged: {e}");
                return;
            }
            // see onServicesDiscovered().
            self.discover_services_on_change.lock().unwrap().replace(disc_lock);
        }
    }
}

fn gatt_error_check(status: i32) -> Result<(), AttError> {
    if status == AttError::SUCCESS.as_u8() as i32 {
        Ok(())
    } else if let Ok(status) = u8::try_from(status) {
        Err(AttError::from_u8(status))
    } else {
        Err(AttError::UNLIKELY_ERROR)
    }
}

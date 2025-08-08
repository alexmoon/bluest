use std::sync::Arc;
use std::{collections::HashMap, sync::OnceLock};

use futures_core::Stream;
use futures_lite::{stream, StreamExt};
use java_spaghetti::{ByteArray, Env, Global, Local, Null, Ref};
use tracing::{debug, warn};
use uuid::Uuid;

use super::bindings::{
    android::{
        bluetooth::{
            le::{ScanCallback, ScanFilter_Builder, ScanResult, ScanSettings, ScanSettings_Builder},
            BluetoothAdapter, BluetoothGattCallback, BluetoothManager,
        },
        content::Context as AndroidContext,
        os::ParcelUuid,
    },
    java::{self, lang::String as JString, util::Map_Entry},
};
use super::{
    device::DeviceImpl,
    event_receiver::{EventReceiver, GlobalEvent},
    gatt_tree::{BluetoothGattCallbackProxy, CachedWeak, GattTree},
    jni::{ByteArrayExt, Monitor, VM},
    vm_context::{android_api_level, android_context, jni_get_vm, jni_set_vm, jni_with_env},
    JavaIterator, OptionExt,
};
use crate::{
    error::ErrorKind, util::defer, AdapterEvent, AdvertisementData, AdvertisingDevice, ConnectionEvent, Device,
    DeviceId, Error, ManufacturerData, Result,
};

struct AdapterInner {
    #[allow(unused)]
    manager: Global<BluetoothManager>,
    adapter: Global<BluetoothAdapter>,
    global_event_receiver: Arc<EventReceiver>,
}

#[derive(Clone)]
pub struct AdapterImpl {
    inner: Arc<AdapterInner>,
}

/// Configuration for creating an interface to the default Bluetooth adapter of the system.
pub struct AdapterConfig {
    /// - `vm` must be a valid JNI `JavaVM` pointer to a VM that will stay alive for the current
    ///   native library's lifetime. This is true for any library used by an Android application.
    vm: *mut java_spaghetti::sys::JavaVM,
    /// `manager` must be a valid global reference to an `android.bluetooth.BluetoothManager`
    /// instance, from the `java_vm` VM.
    manager: java_spaghetti::sys::jobject,
}

impl AdapterConfig {
    /// Creates a config for the default Bluetooth adapter for the system.
    ///
    /// # Safety
    ///
    /// - `java_vm` must be a valid JNI `JavaVM` pointer to a VM that will stay alive for the current native
    ///   library's lifetime. This is true for any library used by an Android application.
    /// - `bluetooth_manager` must be a valid global reference to an `android.bluetooth.BluetoothManager`
    ///   instance, from the `java_vm` VM.
    /// - The `Adapter` takes ownership of the global reference and will delete it with the `DeleteGlobalRef`
    ///   JNI call when dropped. You must not do that yourself.
    pub unsafe fn new(
        java_vm: *mut java_spaghetti::sys::JavaVM,
        bluetooth_manager: java_spaghetti::sys::jobject,
    ) -> Self {
        Self {
            vm: java_vm,
            manager: bluetooth_manager,
        }
    }
}

impl Default for AdapterConfig {
    fn default() -> Self {
        jni_get_vm()
            .with_env(|env| {
                let context = android_context().as_local(env);
                let service_name = JString::from_env_str(env, AndroidContext::BLUETOOTH_SERVICE);
                let manager = context
                    .getSystemService_String(service_name)
                    .unwrap()
                    .expect("Context.getSystemService() returned null for BLUETOOTH_SERVICE")
                    .cast::<BluetoothManager>()?
                    .as_global();
                Ok::<_, Box<dyn std::error::Error>>(Self {
                    vm: jni_get_vm().as_raw(),
                    manager: manager.into_raw(),
                })
            })
            .unwrap()
    }
}

impl AdapterImpl {
    /// Creates an interface to a Bluetooth adapter.
    pub async fn with_config(config: AdapterConfig) -> Result<Self> {
        unsafe {
            let vm = VM::from_raw(config.vm);
            let _ = jni_set_vm(vm);

            let manager: Global<BluetoothManager> = Global::from_raw(vm.into(), config.manager);

            vm.with_env(|env| {
                let local_manager = manager.as_ref(env);
                let adapter = local_manager.getAdapter()?.non_null()?;

                Ok(Self {
                    inner: Arc::new(AdapterInner {
                        adapter: adapter.as_global(),
                        manager: manager.clone(),
                        global_event_receiver: EventReceiver::build()?,
                    }),
                })
            })
        }
    }

    pub(crate) async fn events(&self) -> Result<impl Stream<Item = Result<AdapterEvent>> + Send + Unpin + '_> {
        Ok(self
            .inner
            .global_event_receiver
            .subscribe()
            .await?
            .filter_map(|event| {
                if let GlobalEvent::AdapterStateChanged(val) = event {
                    match val {
                        BluetoothAdapter::STATE_ON => Some(AdapterEvent::Available),
                        BluetoothAdapter::STATE_OFF => Some(AdapterEvent::Unavailable),
                        _ => None, // XXX: process "turning on" and "turning off" events
                    }
                } else {
                    None
                }
            })
            .map(Ok))
    }

    pub async fn wait_available(&self) -> Result<()> {
        while !self.is_available().await? {
            let mut events = self.events().await?;
            while let Some(Ok(event)) = events.next().await {
                if event == AdapterEvent::Available {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Check if the adapter is available
    pub async fn is_available(&self) -> Result<bool> {
        jni_with_env(|env| {
            let adapter = self.inner.adapter.as_local(env);
            adapter
                .isEnabled()
                .map_err(|e| Error::new(ErrorKind::Internal, None, format!("isEnabled threw: {e:?}")))
        })
    }

    pub async fn open_device(&self, id: &DeviceId) -> Result<Device> {
        if let Some(dev) = self.connected_devices().await?.into_iter().find(|d| &d.id() == id) {
            return Ok(dev);
        }
        jni_with_env(|env| {
            let adapter = self.inner.adapter.as_local(env);
            let device = adapter
                .getRemoteDevice_String(JString::from_env_str(env, &id.0))
                .map_err(|e| Error::new(ErrorKind::Internal, None, format!("getRemoteDevice threw: {e:?}")))?
                .non_null()?;
            Ok(Device(DeviceImpl {
                id: id.clone(),
                device: device.as_global(),
                connection: CachedWeak::new(),
                once_connected: Arc::new(OnceLock::new()),
            }))
        })
    }

    pub async fn connected_devices(&self) -> Result<Vec<Device>> {
        // XXX: there might be BLE devices connected outside `bluest`, currently they are ignored here.
        // is it possible to have multiple connections with different `BluetoothGattCallback`s to the same device?
        GattTree::registered_devices()
    }

    pub async fn connected_devices_with_services(&self, service_ids: &[Uuid]) -> Result<Vec<Device>> {
        let mut devices_found = Vec::new();
        for device in self.connected_devices().await? {
            device.discover_services().await?;
            let device_services = device.services().await?;
            if service_ids
                .iter()
                .any(|&id| device_services.iter().any(|serv| serv.uuid() == id))
            {
                devices_found.push(device);
            }
        }
        Ok(devices_found)
    }

    pub async fn scan<'a>(
        &'a self,
        services: &'a [Uuid],
    ) -> Result<impl Stream<Item = AdvertisingDevice> + Send + Unpin + 'a> {
        let (start_receiver, stream) = jni_with_env(|env| {
            let (start_sender, start_receiver) = async_channel::bounded(1);
            let (device_sender, device_receiver) = async_channel::bounded(16);

            let callback = ScanCallback::new_proxy(
                env,
                Arc::new(ScanCallbackProxy {
                    device_sender,
                    start_sender,
                }),
            )?;
            let callback_global = callback.as_global();

            let adapter = self.inner.adapter.as_ref(env);
            let adapter = Monitor::new(&adapter);
            let scanner = adapter.getBluetoothLeScanner()?.non_null()?;
            let scanner_global = scanner.as_global();

            let settings_builder = ScanSettings_Builder::new(env)?;
            settings_builder.setScanMode(ScanSettings::SCAN_MODE_LOW_LATENCY)?;
            let settings = settings_builder.build()?.non_null()?;

            if !services.is_empty() {
                let filter_builder = ScanFilter_Builder::new(env)?;
                let filter_list = java::util::ArrayList::new(env)?;
                for uuid in services {
                    let uuid_string = JString::from_env_str(env, uuid.to_string());
                    let parcel_uuid = ParcelUuid::fromString(env, uuid_string)?;
                    filter_builder.setServiceUuid_ParcelUuid(parcel_uuid)?;
                    let filter = filter_builder.build()?.non_null()?;
                    filter_list.add_Object(filter)?;
                }
                scanner.startScan_List_ScanSettings_ScanCallback(filter_list, settings, callback)?;
            } else {
                scanner.startScan_List_ScanSettings_ScanCallback(Null, settings, callback)?;
            };

            let guard = defer(move || {
                jni_with_env(|env| {
                    let callback = callback_global.as_ref(env);
                    let scanner = scanner_global.as_ref(env);
                    match scanner.stopScan_ScanCallback(callback) {
                        Ok(()) => debug!("stopped scan"),
                        Err(e) => warn!("failed to stop scan: {:?}", e),
                    };
                });
            });

            Ok::<_, crate::Error>((
                start_receiver,
                Box::pin(device_receiver).map(move |x| {
                    let _guard = &guard;
                    x
                }),
            ))
        })?;

        struct UntilAdapterDisabled<T>(T);
        impl<T> futures_core::Stream for UntilAdapterDisabled<T>
        where
            T: Stream<Item = Result<AdapterEvent>> + Send + Unpin,
        {
            type Item = AdvertisingDevice;
            fn poll_next(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Option<Self::Item>> {
                use futures_core::task::Poll;
                if let Poll::Ready(Some(Ok(AdapterEvent::Unavailable))) = self.0.poll_next(cx) {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            }
        }

        // Wait for scan started or failed.
        match start_receiver.recv().await {
            Ok(Ok(())) => Ok(stream.or(UntilAdapterDisabled(self.events().await?)).fuse()),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(Error::new(
                ErrorKind::Internal,
                None,
                format!("receiving failed while waiting for start: {e:?}"),
            )),
        }
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

    pub async fn connect_device(&self, device: &Device) -> Result<()> {
        if GattTree::find_connection(&device.id()).is_some() {
            return Ok(());
        }
        jni_with_env(|env| {
            let adapter = self.inner.adapter.as_ref(env);
            let _lock = Monitor::new(&adapter);
            let device_obj = device.0.device.as_local(env);
            let callback_hdl = BluetoothGattCallbackProxy::new(device.id());
            let callback = BluetoothGattCallback::new_proxy(env, callback_hdl.clone())?;
            let gatt = device_obj
                .connectGatt_Context_boolean_BluetoothGattCallback(android_context().as_ref(env), false, callback)
                .map_err(|e| Error::new(ErrorKind::Internal, None, format!("connectGatt threw: {e:?}")))?
                .non_null()?
                .as_global();
            GattTree::register_connection(&device.id(), gatt, &callback_hdl, &self.inner.global_event_receiver);
            Ok::<_, crate::Error>(())
        })?;
        // validates GATT tree API objects again upon reconnection
        if device.0.once_connected.get().is_some() {
            let _ = device.discover_services().await?;
        }
        Ok(())
    }

    // XXX: manage to call this automatically when all items belonging to the device are dropped.
    pub async fn disconnect_device(&self, device: &Device) -> Result<()> {
        let Some(conn) = device.0.connection.get() else {
            return Ok(());
        };
        jni_with_env(|env| {
            let adapter = self.inner.adapter.as_ref(env);
            let _lock = Monitor::new(&adapter);
            conn.gatt
                .as_ref(env)
                .disconnect() // XXX: is `close()` also needed?
                .map_err(|e| Error::new(ErrorKind::Internal, None, format!("disconnect threw: {e:?}")))
        })?;
        GattTree::deregister_connection(&device.id());
        Ok(())
    }

    // NOTE: currently this doesn't work with random address devices.
    pub async fn device_connection_events<'a>(
        &'a self,
        device: &'a Device,
    ) -> Result<impl Stream<Item = ConnectionEvent> + Send + Unpin + 'a> {
        Ok(self
            .inner
            .global_event_receiver
            .subscribe()
            .await?
            .filter_map(|event| match event {
                GlobalEvent::ConnectionStateChanged(dev_id, val) if dev_id == device.id() => {
                    match val {
                        BluetoothAdapter::STATE_CONNECTED => Some(ConnectionEvent::Connected),
                        BluetoothAdapter::STATE_DISCONNECTED => Some(ConnectionEvent::Disconnected),
                        _ => None, // XXX: process "connecting" and "disconnecting" events
                    }
                }
                _ => None,
            }))
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

fn convert_uuid(uuid: Local<'_, ParcelUuid>) -> Result<Uuid> {
    // doing 1 JNI method call, probably faster than 3 method calls:
    // getUuid(), getLeastSignificantBits(), getMostSignificantBits()
    uuid::Uuid::parse_str(uuid.toString()?.non_null()?.to_string_lossy().trim())
        .map_err(|e| crate::Error::new(ErrorKind::Internal, Some(Box::new(e)), "Uuid::parse_str failed"))
}

struct ScanCallbackProxy {
    start_sender: async_channel::Sender<Result<()>>,
    device_sender: async_channel::Sender<AdvertisingDevice>,
}

impl super::callback::ScanCallbackProxy for ScanCallbackProxy {
    fn onScanFailed<'env>(&self, _env: Env<'env>, error_code: i32) {
        let e = Error::new(
            ErrorKind::Internal,
            None,
            format!("Scan failed to start with error code {error_code}"),
        );
        if let Err(e) = self.start_sender.try_send(Err(e)) {
            warn!("onScanFailed failed to send error: {e:?}");
        }
    }

    fn onBatchScanResults<'env>(
        &self,
        env: Env<'env>,
        scan_results: Option<Ref<'env, super::bindings::java::util::List>>,
    ) {
        let Some(scan_results) = scan_results else {
            warn!("onBatchScanResults: ignoring null scan_results");
            return;
        };

        if let Err(e) = self.on_scan_result_list(env, &scan_results) {
            warn!("onBatchScanResults failed: {e:?}");
        }
    }

    fn onScanResult<'env>(&self, env: Env<'env>, _callback_type: i32, scan_result: Option<Ref<'env, ScanResult>>) {
        let Some(scan_result) = scan_result else {
            warn!("onScanResult: ignoring null scan_result");
            return;
        };

        if let Err(e) = self.on_scan_result(env, &scan_result) {
            warn!("onScanResult failed: {e:?}");
        }
    }
}

impl ScanCallbackProxy {
    fn on_scan_result_list(&self, env: Env<'_>, scan_results: &Ref<super::bindings::java::util::List>) -> Result<()> {
        for scan_result in JavaIterator(scan_results.iterator()?.non_null()?) {
            let scan_result: Local<ScanResult> = scan_result.cast()?;
            self.on_scan_result(env, &scan_result.as_ref())?;
        }
        Ok(())
    }

    fn on_scan_result(&self, _env: Env<'_>, scan_result: &Ref<ScanResult>) -> Result<()> {
        let scan_record = scan_result.getScanRecord()?.non_null()?;
        let device = scan_result.getDevice()?.non_null()?;

        let address = device.getAddress()?.non_null()?.to_string_lossy();
        let rssi = scan_result.getRssi()?;
        let is_connectable = if android_api_level() >= 26 {
            scan_result.isConnectable()?
        } else {
            true // XXX: try to check `eventType` via `ScanResult.toString()`
        };
        let local_name = scan_record.getDeviceName()?.map(|s| s.to_string_lossy());
        let tx_power_level = scan_record.getTxPowerLevel()?;

        // Services
        let mut services = Vec::new();
        if let Some(uuids) = scan_record.getServiceUuids()? {
            for uuid in JavaIterator(uuids.iterator()?.non_null()?) {
                services.push(convert_uuid(uuid.cast()?)?)
            }
        }

        // Service data
        let mut service_data = HashMap::new();
        let sd = scan_record.getServiceData()?.non_null()?;
        let sd = sd.entrySet()?.non_null()?;
        for entry in JavaIterator(sd.iterator()?.non_null()?) {
            let entry: Local<Map_Entry> = entry.cast()?;
            let key: Local<ParcelUuid> = entry.getKey()?.non_null()?.cast()?;
            let val: Local<ByteArray> = entry.getValue()?.non_null()?.cast()?;
            service_data.insert(convert_uuid(key)?, val.as_vec_u8());
        }

        // Manufacturer data
        let mut manufacturer_data = None;
        let msd = scan_record.getManufacturerSpecificData()?.non_null()?;
        // TODO: there can be multiple manufacturer data entries, but the bluest API only supports one. So grab just the first.
        if msd.size()? != 0 {
            let val: Local<'_, ByteArray> = msd.valueAt(0)?.non_null()?.cast()?;
            manufacturer_data = Some(ManufacturerData {
                company_id: msd.keyAt(0)? as _,
                data: val.as_vec_u8(),
            });
        }

        let device_id = DeviceId(address);

        let d = AdvertisingDevice {
            device: Device(DeviceImpl {
                id: device_id.clone(),
                device: device.as_global(),
                connection: CachedWeak::new(),
                once_connected: Arc::new(if GattTree::find_connection(&device_id).is_none() {
                    OnceLock::new()
                } else {
                    OnceLock::from(()) // NOTE: this is unlikely to happen
                }),
            }),
            adv_data: AdvertisementData {
                is_connectable,
                local_name,
                manufacturer_data, // TODO, SparseArray is cursed.
                service_data,
                services,
                tx_power_level: Some(tx_power_level as _),
            },
            rssi: Some(rssi as _),
        };

        self.start_sender.try_send(Ok(())).ok();
        self.device_sender.try_send(d).ok();

        Ok(())
    }
}

use std::collections::HashMap;
use std::sync::Arc;

use async_channel::Sender;
use futures_core::Stream;
use futures_lite::{stream, StreamExt};
use java_spaghetti::{ByteArray, Env, Global, Local, Null, PrimitiveArray, Ref, VM};
use tracing::{debug, warn};
use uuid::Uuid;

use super::bindings::android::bluetooth::le::{
    BluetoothLeScanner, ScanCallback, ScanResult, ScanSettings, ScanSettings_Builder,
};
use super::bindings::android::bluetooth::{BluetoothAdapter, BluetoothManager};
use super::bindings::android::os::ParcelUuid;
use super::device::DeviceImpl;
use super::{JavaIterator, OptionExt};
use crate::android::bindings::java::util::Map_Entry;
use crate::util::defer;
use crate::{
    AdapterEvent, AdvertisementData, AdvertisingDevice, ConnectionEvent, Device, DeviceId, ManufacturerData, Result,
};

struct AdapterInner {
    manager: Global<BluetoothManager>,
    _adapter: Global<BluetoothAdapter>,
    le_scanner: Global<BluetoothLeScanner>,
}

#[derive(Clone)]
pub struct AdapterImpl {
    inner: Arc<AdapterInner>,
}

/// Creates an interface to the default Bluetooth adapter for the system.
///
/// # Safety
///
/// - The `Adapter` takes ownership of the global reference and will delete it with the `DeleteGlobalRef` JNI call when dropped. You must not do that yourself.
pub struct AdapterConfig {
    /// - `vm` must be a valid JNI `JavaVM` pointer to a VM that will stay alive for the entire duration the `Adapter` or any structs obtained from it are live.
    vm: *mut java_spaghetti::sys::JavaVM,
    /// - `manager` must be a valid global reference to an `android.bluetooth.BluetoothManager` instance, from the `java_vm` VM.
    manager: java_spaghetti::sys::jobject,
}

impl AdapterConfig {
    /// Creates a config for the default Bluetooth adapter for the system.
    ///
    /// # Safety
    ///
    /// - `java_vm` must be a valid JNI `JavaVM` pointer to a VM that will stay alive for the entire duration the `Adapter` or any structs obtained from it are live.
    /// - `bluetooth_manager` must be a valid global reference to an `android.bluetooth.BluetoothManager` instance, from the `java_vm` VM.
    /// - The `Adapter` takes ownership of the global reference and will delete it with the `DeleteGlobalRef` JNI call when dropped. You must not do that yourself.
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

impl AdapterImpl {
    /// Creates an interface to a Bluetooth adapter.
    ///
    /// # Safety
    ///
    /// In the config object:
    ///
    /// - `vm` must be a valid JNI `JavaVM` pointer to a VM that will stay alive for the entire duration the `Adapter` or any structs obtained from it are live.
    /// - `manager` must be a valid global reference to an `android.bluetooth.BluetoothManager` instance, from the `java_vm` VM.
    /// - The `Adapter` takes ownership of the global reference and will delete it with the `DeleteGlobalRef` JNI call when dropped. You must not do that yourself.
    pub async fn with_config(config: AdapterConfig) -> Result<Self> {
        unsafe {
            let vm = VM::from_raw(config.vm);
            let manager: Global<BluetoothManager> = Global::from_raw(vm, config.manager);

            vm.with_env(|env| {
                let local_manager = manager.as_ref(env);
                let adapter = local_manager.getAdapter()?.non_null()?;
                let le_scanner = adapter.getBluetoothLeScanner()?.non_null()?;

                Ok(Self {
                    inner: Arc::new(AdapterInner {
                        _adapter: adapter.as_global(),
                        le_scanner: le_scanner.as_global(),
                        manager: manager.clone(),
                    }),
                })
            })
        }
    }

    pub(crate) async fn events(&self) -> Result<impl Stream<Item = Result<AdapterEvent>> + Send + Unpin + '_> {
        Ok(stream::empty()) // TODO
    }

    pub async fn wait_available(&self) -> Result<()> {
        Ok(())
    }

    /// Check if the adapter is available
    pub async fn is_available(&self) -> Result<bool> {
        Ok(true)
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
        self.inner.manager.vm().with_env(|env| {
            let (device_sender, device_receiver) = async_channel::bounded(16);
            let callback = ScanCallback::new_proxy(env, Arc::new(ScanCallbackProxy { device_sender }))?;

            let callback_global = callback.as_global();
            let scanner = self.inner.le_scanner.as_ref(env);
            let settings = ScanSettings_Builder::new(env)?;
            settings.setScanMode(ScanSettings::SCAN_MODE_LOW_LATENCY)?;
            let settings = settings.build()?.non_null()?;
            scanner.startScan_List_ScanSettings_ScanCallback(Null, settings, callback)?;

            let guard = defer(move || {
                self.inner.manager.vm().with_env(|env| {
                    let callback = callback_global.as_ref(env);
                    let scanner = self.inner.le_scanner.as_ref(env);
                    match scanner.stopScan_ScanCallback(callback) {
                        Ok(()) => debug!("stopped scan"),
                        Err(e) => warn!("failed to stop scan: {:?}", e),
                    };
                });
            });

            Ok(Box::pin(device_receiver).map(move |x| {
                let _guard = &guard;
                x
            }))
        })
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

fn convert_uuid(uuid: Local<'_, ParcelUuid>) -> Result<Uuid> {
    let uuid = uuid.getUuid()?.non_null()?;
    let lsb = uuid.getLeastSignificantBits()? as u64;
    let msb = uuid.getMostSignificantBits()? as u64;
    Ok(Uuid::from_u64_pair(msb, lsb))
}

struct ScanCallbackProxy {
    device_sender: Sender<AdvertisingDevice>,
}

impl super::bindings::android::bluetooth::le::ScanCallbackProxy for ScanCallbackProxy {
    fn onScanFailed<'env>(&self, _env: Env<'env>, error_code: i32) -> () {
        tracing::error!("got scan fail! {}", error_code);
        todo!()
    }

    fn onBatchScanResults<'env>(
        &self,
        env: Env<'env>,
        scan_results: Option<Ref<'env, super::bindings::java::util::List>>,
    ) -> () {
        let Some(scan_results) = scan_results else {
            warn!("onBatchScanResults: ignoring null scan_results");
            return;
        };

        if let Err(e) = self.on_scan_result_list(env, &scan_results) {
            warn!("onBatchScanResults failed: {e:?}");
        }
    }

    fn onScanResult<'env>(
        &self,
        env: Env<'env>,
        _callback_type: i32,
        scan_result: Option<Ref<'env, ScanResult>>,
    ) -> () {
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
            self.on_scan_result(env, &scan_result)?;
        }
        Ok(())
    }

    fn on_scan_result(&self, _env: Env<'_>, scan_result: &Ref<ScanResult>) -> Result<()> {
        let scan_record = scan_result.getScanRecord()?.non_null()?;
        let device = scan_result.getDevice()?.non_null()?;

        let address = device.getAddress()?.non_null()?.to_string_lossy();
        let rssi = scan_result.getRssi()?;
        let is_connectable = scan_result.isConnectable()?;
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
            service_data.insert(convert_uuid(key)?, val.as_vec().into_iter().map(|i| i as u8).collect());
        }

        // Manufacturer data
        let mut manufacturer_data = None;
        let msd = scan_record.getManufacturerSpecificData()?.non_null()?;
        // TODO there can be multiple manufacturer data entries, but the bluest API only supports one. So grab just the first.
        if msd.size()? != 0 {
            let val: Local<'_, ByteArray> = msd.valueAt(0)?.non_null()?.cast()?;
            manufacturer_data = Some(ManufacturerData {
                company_id: msd.keyAt(0)? as _,
                data: val.as_vec().into_iter().map(|i| i as u8).collect(),
            });
        }

        let device_id = DeviceId(address);

        let d = AdvertisingDevice {
            device: Device(DeviceImpl {
                id: device_id,
                device: device.as_global(),
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

        self.device_sender.try_send(d).ok();

        Ok(())
    }
}

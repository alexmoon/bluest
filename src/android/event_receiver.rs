use std::sync::{Arc, Mutex, OnceLock, Weak};

use java_spaghetti::{Env, Global, Ref};
use tracing::{error, info}; // TODO: make it working reliably in the thread of Java callbacks

use super::async_util::{Notifier, NotifierReceiver};
use super::bindings::android::bluetooth::{BluetoothAdapter, BluetoothDevice};
use super::bindings::android::content::{BroadcastReceiver, Context, Intent, IntentFilter};
use super::bindings::java::lang::{Class, String as JString};
use super::gatt_tree::GattTree;
use super::vm_context::{android_api_level, android_context, jni_with_env};
use super::{DeviceId, OptionExt};

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub enum GlobalEvent {
    /// contains EXTRA_STATE
    AdapterStateChanged(i32),
    /// contains device address
    #[allow(unused)] // NOTE: this may not be received; this can be removed.
    AclConnectionStateChanged(DeviceId, bool),
    /// contains device address, EXTRA_PREVIOUS_BOND_STATE, and EXTRA_BOND_STATE
    BondStateChanged(DeviceId, i32, i32),
}

static GLOBAL_RECEIVER: Mutex<Weak<EventReceiver>> = Mutex::new(Weak::new());

pub struct EventReceiver {
    notifier: Notifier<GlobalEvent>,
    java_receiver: OnceLock<Global<BroadcastReceiver>>,
}

impl EventReceiver {
    pub fn build() -> Result<Arc<Self>, crate::Error> {
        let mut global_rec = GLOBAL_RECEIVER.lock().unwrap();
        if let Some(rec) = global_rec.upgrade() {
            return Ok(rec);
        }
        let event_receiver = Arc::new(Self {
            notifier: Notifier::new(128),
            java_receiver: OnceLock::new(),
        });
        let event_receiver_weak = Arc::downgrade(&event_receiver);
        let proxy = Arc::new(BroadcastReceiverProxy {
            rec_hdl: event_receiver_weak,
        });
        let java_receiver =
            jni_with_env(|env| Ok::<_, crate::Error>(BroadcastReceiver::new_proxy(env, proxy)?.as_global()))?;
        let _ = event_receiver.java_receiver.set(java_receiver);
        *global_rec = Arc::downgrade(&event_receiver);
        Ok(event_receiver)
    }

    pub async fn subscribe(&self) -> Result<NotifierReceiver<GlobalEvent>, crate::Error> {
        let java_receiver = self.java_receiver.get().unwrap().clone();
        let java_receiver_2 = self.java_receiver.get().unwrap().clone();
        self.notifier
            .subscribe(
                move || {
                    jni_with_env(|env| {
                        let filter = IntentFilter::new(env)?;
                        for action in [
                            BluetoothAdapter::ACTION_STATE_CHANGED,
                            BluetoothDevice::ACTION_ACL_CONNECTED,
                            BluetoothDevice::ACTION_ACL_DISCONNECTED,
                            BluetoothDevice::ACTION_BOND_STATE_CHANGED,
                        ] {
                            let action_jstring = JString::from_env_str(env, action);
                            filter.addAction(&action_jstring)?;
                        }
                        info!("registering the global bluetooth event broadcast receiver.");
                        android_context()
                            .as_ref(env)
                            .registerReceiver_BroadcastReceiver_IntentFilter(java_receiver.as_ref(env), &filter)
                            .map_err(|e| e.into())
                            .map(|_| ())
                    })
                },
                move || {
                    jni_with_env(|env| {
                        info!("deregistering the global bluetooth event broadcast receiver.");
                        let _ = android_context()
                            .as_ref(env)
                            .unregisterReceiver(java_receiver_2.as_ref(env));
                    })
                },
            )
            .await
    }
}

struct BroadcastReceiverProxy {
    rec_hdl: Weak<EventReceiver>,
}

impl super::callback::BroadcastReceiverProxy for BroadcastReceiverProxy {
    // NOTE: events for non-GATT profile devices may be received.
    fn onReceive<'env>(&self, env: Env<'env>, _context: Option<Ref<'env, Context>>, intent: Option<Ref<'env, Intent>>) {
        let Some(rec_hdl) = self.rec_hdl.upgrade() else {
            return;
        };
        let Some(intent) = intent else {
            return;
        };
        let get_action =
            |intent: &Ref<'_, Intent>| Ok::<_, crate::Error>(intent.getAction()?.non_null()?.to_string_lossy());
        let Ok(action) = get_action(&intent) else {
            error!("failed to get the action string of the received intent");
            return;
        };
        let process_intent = move || match action.trim() {
            BluetoothAdapter::ACTION_STATE_CHANGED => {
                let extra_state = JString::from_env_str(env, BluetoothAdapter::EXTRA_STATE);
                let val = intent.getIntExtra(&extra_state, 0)?;
                if val == BluetoothAdapter::STATE_OFF {
                    // XXX: or STATE_TURNING_OFF?
                    if GattTree::clear_connections() {
                        info!("deregistered all connections in BroadcastReceiverProxy");
                    }
                }
                rec_hdl.notifier.notify(GlobalEvent::AdapterStateChanged(val));
                Ok::<_, crate::Error>(())
            }
            BluetoothDevice::ACTION_ACL_CONNECTED => {
                let extra_transport = JString::from_env_str(env, BluetoothDevice::EXTRA_TRANSPORT);
                let transport = intent.getIntExtra(&extra_transport, 0)?;
                if transport == BluetoothDevice::TRANSPORT_LE {
                    let dev_id = get_extra_device_id(&intent)?;
                    rec_hdl
                        .notifier
                        .notify(GlobalEvent::AclConnectionStateChanged(dev_id, true));
                }
                Ok(())
            }
            BluetoothDevice::ACTION_ACL_DISCONNECTED => {
                let extra_transport = JString::from_env_str(env, BluetoothDevice::EXTRA_TRANSPORT);
                let transport = intent.getIntExtra(&extra_transport, 0)?;
                if transport == BluetoothDevice::TRANSPORT_LE {
                    let dev_id = get_extra_device_id(&intent)?;
                    if GattTree::deregister_connection(&dev_id) {
                        info!("deregistered connection with {dev_id} in BroadcastReceiverProxy");
                    }
                    rec_hdl
                        .notifier
                        .notify(GlobalEvent::AclConnectionStateChanged(dev_id, false));
                }
                Ok(())
            }
            BluetoothDevice::ACTION_BOND_STATE_CHANGED => {
                let dev_id = get_extra_device_id(&intent)?;
                let extra_prev_bond_state = JString::from_env_str(env, BluetoothDevice::EXTRA_PREVIOUS_BOND_STATE);
                let prev_bond_state = intent.getIntExtra(&extra_prev_bond_state, 0)?;
                let extra_bond_state = JString::from_env_str(env, BluetoothDevice::EXTRA_BOND_STATE);
                let bond_state = intent.getIntExtra(&extra_bond_state, 0)?;
                rec_hdl
                    .notifier
                    .notify(GlobalEvent::BondStateChanged(dev_id, prev_bond_state, bond_state));
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = process_intent() {
            error!("failed to get the extra value of the received intent: {e}");
        }
    }
}

fn get_extra_device_id(intent: &Ref<'_, Intent>) -> Result<DeviceId, crate::Error> {
    let env = intent.env();
    let extra_device = JString::from_env_str(env, BluetoothDevice::EXTRA_DEVICE);
    let device = if android_api_level() >= 33 {
        let class_device = unsafe {
            java_spaghetti::Local::<Class>::from_raw(env, env.require_class("android/bluetooth/BluetoothDevice\0"))
        };
        intent
            .getParcelableExtra_String_Class(&extra_device, &class_device)?
            .and_then(|o| o.cast::<BluetoothDevice>().ok())
    } else {
        #[allow(deprecated)]
        intent
            .getParcelableExtra_String(&extra_device)?
            .and_then(|o| o.cast::<BluetoothDevice>().ok())
    }
    .non_null()?;
    let addr = device.getAddress()?.non_null()?.to_string_lossy();
    Ok(DeviceId(addr))
}

//! XXX: migrate this module into a seperate helper crate for `java-spaghetti`.

use std::ptr::null;
use std::sync::OnceLock;

use java_spaghetti::{Env, Global, Ref};
use tracing::warn;

use super::bindings::android::content::Context;
use super::bindings::android::os::Build_VERSION;
use super::bindings::dalvik::system::{DexClassLoader, InMemoryDexClassLoader};
use super::bindings::java::lang::{Class, ClassLoader, Object, String as JString, Throwable};
use super::bindings::java::nio;
use super::jni::{ByteArrayExt, VM};

static JAVA_VM: OnceLock<VM> = OnceLock::new();
static ANDROID_CONTEXT: OnceLock<Global<Context>> = OnceLock::new();

pub fn jni_set_vm(vm: VM) -> bool {
    JAVA_VM.set(vm).is_ok()
}

pub fn jni_get_vm() -> VM {
    *JAVA_VM.get_or_init(|| {
        let vm = ndk_context::android_context().vm();
        if vm.is_null() {
            panic!("ndk-context is unconfigured: null JVM pointer, check the glue crate.");
        }
        unsafe { VM::from_raw(vm.cast()) }
    })
}

pub fn jni_with_env<F, R>(callback: F) -> R
where
    F: for<'env> FnOnce(Env<'env>) -> R,
{
    jni_get_vm().with_env(callback)
}

pub fn android_context() -> Global<Context> {
    ANDROID_CONTEXT
        .get_or_init(|| {
            let ctx = ndk_context::android_context().context();
            jni_with_env(|env| {
                if ctx.is_null() {
                    // `ActivityThread` is public but hidden, `java-spaghetti-gen` just ignores it.
                    warn!("`ndk_context::android_context().context()` is null, using `Application` as context.");
                    unsafe {
                        let (class, method) = env.require_class_static_method(
                            "android/app/ActivityThread\0",
                            "currentActivityThread\0",
                            "()Landroid/app/ActivityThread;\0",
                        );
                        let activity_thread = env
                            .call_static_object_method_a::<Object, Throwable>(class, method, null())
                            .unwrap()
                            .unwrap();
                        let method = env.require_method(class, "getApplication\0", "()Landroid/app/Application;\0");
                        env.call_object_method_a::<Context, Throwable>(activity_thread.as_raw(), method, null())
                    }
                    .unwrap()
                    .unwrap()
                    .as_global()
                } else {
                    unsafe { Ref::<'_, Context>::from_raw(env, ctx.cast()) }.as_global()
                }
            })
        })
        .clone()
}

pub fn android_api_level() -> i32 {
    static API_LEVEL: OnceLock<i32> = OnceLock::new();
    *API_LEVEL.get_or_init(|| jni_with_env(Build_VERSION::SDK_INT))
}

/// Note: this will panic if `dex_data` is invalid.
pub fn android_load_dex(dex_data: &[u8]) -> Global<ClassLoader> {
    let vm = jni_get_vm();
    let context = android_context();
    vm.with_env(|env| {
        let context = context.as_ref(env);
        let context_loader = context.getClassLoader().unwrap();
        if android_api_level() >= 26 {
            let byte_array = java_spaghetti::ByteArray::from_slice(env, dex_data);
            let dex_buffer = nio::ByteBuffer::wrap_byte_array(env, byte_array).unwrap();
            let dex_loader =
                InMemoryDexClassLoader::new_ByteBuffer_ClassLoader(env, dex_buffer, context_loader).unwrap();
            dex_loader.cast::<ClassLoader>().unwrap().as_global()
        } else {
            let cache_dir = context.getCacheDir().unwrap().unwrap();
            let path_string = cache_dir.getAbsolutePath().unwrap().unwrap().to_string_lossy();
            let cache_dir_path = std::path::PathBuf::from(path_string);

            let dex_file_path = cache_dir_path.join(env!("CARGO_CRATE_NAME").to_string() + ".dex");
            std::fs::write(&dex_file_path, dex_data).unwrap();

            let oats_dir_path = cache_dir_path.join("oats");
            let _ = std::fs::create_dir(&oats_dir_path);

            let dex_file_jstring = JString::from_env_str(env, dex_file_path.to_string_lossy().as_ref());
            let oats_dir_jstring = JString::from_env_str(env, oats_dir_path.to_string_lossy().as_ref());

            let dex_loader = DexClassLoader::new(
                env,
                &dex_file_jstring,
                &oats_dir_jstring,
                java_spaghetti::Null,
                &context_loader,
            )
            .unwrap();
            dex_loader.cast::<ClassLoader>().unwrap().as_global()
        }
    })
}

pub fn jni_load_class_with<'env>(loader: Ref<'env, ClassLoader>, bin_name: &str) -> Option<Global<Class>> {
    let env = loader.env();
    let bin_name = JString::from_env_str(env, bin_name.replace('/', "."));
    loader
        .loadClass(bin_name.as_ref())
        .ok()
        .flatten()
        .map(|o| o.as_global())
}

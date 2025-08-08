// TODO: remove this module after publishing a new version of `java-spaghetti`.

use std::cell::{Cell, OnceCell};
use std::ptr::null_mut;
use std::slice::from_raw_parts;

use java_spaghetti::{sys::*, ByteArray, Env, Local, PrimitiveArray, Ref, ReferenceType};

/// FFI: Use **&VM** instead of *const JavaVM.  This represents a global, process-wide Java exection environment.
///
/// On Android, there is only one VM per-process, although on desktop it's possible (if rare) to have multiple VMs
/// within the same process.  This library does not support having multiple VMs active simultaniously.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VM(*mut JavaVM);

impl VM {
    pub fn as_raw(&self) -> *mut JavaVM {
        self.0
    }

    /// Constructs `VM` with a *valid* non-null `jni_sys::JavaVM` raw pointer.
    ///
    /// # Safety
    ///
    /// - Make sure the corresponding JVM will keep alive within the lifetime of current native library or application.
    /// - Do not use any class redefinition feature, which may break the validity of method/field IDs to be cached.
    pub unsafe fn from_raw(vm: *mut JavaVM) -> Self {
        Self(vm)
    }

    pub fn with_env<F, R>(&self, callback: F) -> R
    where
        F: for<'env> FnOnce(Env<'env>) -> R,
    {
        let mut env = null_mut();
        let just_attached = match unsafe { ((**self.0).v1_2.GetEnv)(self.0, &mut env, JNI_VERSION_1_2) } {
            JNI_OK => false,
            JNI_EDETACHED => {
                let ret = unsafe { ((**self.0).v1_2.AttachCurrentThread)(self.0, &mut env, null_mut()) };
                if ret != JNI_OK {
                    panic!("AttachCurrentThread returned unknown error: {ret}")
                }
                if !get_thread_exit_flag() {
                    set_thread_attach_flag(self.0);
                }
                true
            }
            JNI_EVERSION => panic!("GetEnv returned JNI_EVERSION"),
            unexpected => panic!("GetEnv returned unknown error: {unexpected}"),
        };

        let result = callback(unsafe { Env::from_raw(env as _) });

        if just_attached && get_thread_exit_flag() {
            unsafe { ((**self.0).v1_2.DetachCurrentThread)(self.0) };
        }

        /* TODO: figure out why this causes null pointer error.
        if just_attached {
            // if `get_thread_exit_flag()` is true, this is *really* needed in case of `with_env`
            // is used on dropping some thread-local instance. However, that flag is ignored here,
            // and the thread is always detached; this is a partial workaround for the local reference
            // leakage bug of `java-spaghetti` 0.2.0, and it's also a permormance compromise.
            // This cannot solve <https://github.com/rust-mobile/android-activity/issues/173>.
            unsafe { ((**self.0).v1_2.DetachCurrentThread)(self.0) };
        }
        */

        result
    }
}

unsafe impl Send for VM {}
unsafe impl Sync for VM {}

impl From<VM> for java_spaghetti::VM {
    fn from(vm: VM) -> Self {
        unsafe { java_spaghetti::VM::from_raw(vm.as_raw()) }
    }
}

thread_local! {
    #[allow(clippy::missing_const_for_thread_local)] // clippy bug?
    static THREAD_ATTACH_FLAG: Cell<Option<AttachFlag>> = const { Cell::new(None) };
    #[allow(clippy::missing_const_for_thread_local)]
    static THREAD_EXIT_FLAG: OnceCell<()> = const { OnceCell::new() };
}

struct AttachFlag {
    raw_vm: *mut JavaVM,
}

impl Drop for AttachFlag {
    fn drop(&mut self) {
        // avoids the fatal error "Native thread exiting without having called DetachCurrentThread"
        unsafe { ((**self.raw_vm).v1_2.DetachCurrentThread)(self.raw_vm) };
        let _ = THREAD_EXIT_FLAG.try_with(|flag| flag.set(()));
    }
}

fn set_thread_attach_flag(raw_vm: *mut JavaVM) {
    THREAD_ATTACH_FLAG.replace(Some(AttachFlag { raw_vm }));
}

fn get_thread_exit_flag() -> bool {
    THREAD_EXIT_FLAG.try_with(|flag| flag.get().is_some()).unwrap_or(true)
}

/// A borrowed [Ref] of a Java object locked with the JNI monitor mechanism, providing *limited* thread safety.
///
/// **It is imposible to be FFI safe.** It is important to drop the monitor or call [Monitor::unlock()] when appropriate.
///
/// Limitations:
///
/// - It merely blocks other native functions from owning the JNI monitor of the same object before it drops.
/// - It may not block other native functions from using the corresponding object without entering monitored mode.
/// - It may not prevent any Java method or block from using this object, even if it is marked as `synchronized`.
/// - While it is a reentrant lock for the current thread, dead lock is still possible under multi-thread conditions.
pub struct Monitor<'env, T: ReferenceType> {
    inner: &'env Ref<'env, T>,
}

impl<'env, T: ReferenceType> Monitor<'env, T> {
    pub fn new(reference: &'env Ref<'env, T>) -> Self {
        let jnienv = reference.env().as_raw();
        let result = unsafe { ((**jnienv).v1_2.MonitorEnter)(jnienv, reference.as_raw()) };
        assert!(result == JNI_OK);
        Self { inner: reference }
    }

    /// Decrements the JNI monitor counter indicating the number of times it has entered this monitor.
    /// If the value of the counter becomes zero, the current thread releases the monitor.
    #[allow(unused)]
    pub fn unlock(self) -> &'env Ref<'env, T> {
        let inner = self.inner;
        drop(self); // this looks clearer than dropping implicitly
        inner
    }
}

impl<'env, T: ReferenceType> std::ops::Deref for Monitor<'env, T> {
    type Target = Ref<'env, T>;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'env, T: ReferenceType> Drop for Monitor<'env, T> {
    fn drop(&mut self) {
        let env = self.inner.env();
        let jnienv = env.as_raw();
        let result = unsafe { ((**jnienv).v1_2.MonitorExit)(jnienv, self.inner.as_raw()) };
        assert!(result == JNI_OK);
        let exception = unsafe { ((**jnienv).v1_2.ExceptionOccurred)(jnienv) };
        assert!(
            exception.is_null(),
            "exception happened calling JNI MonitorExit, the monitor is probably broken previously"
        );
    }
}

pub trait ByteArrayExt {
    fn from_slice<'env>(env: Env<'env>, data: &[u8]) -> Local<'env, ByteArray>;
    fn as_vec_u8(&self) -> Vec<u8>;
}

impl ByteArrayExt for ByteArray {
    fn from_slice<'env>(env: Env<'env>, data: &[u8]) -> Local<'env, ByteArray> {
        let arr = ByteArray::new(env, data.len());
        arr.set_region(0, unsafe { from_raw_parts(data.as_ptr().cast(), data.len()) });
        arr
    }
    fn as_vec_u8(&self) -> Vec<u8> {
        // unsafe { std::mem::transmute(self.as_vec()) }
        self.as_vec().iter().map(|&i| i as u8).collect()
    }
}

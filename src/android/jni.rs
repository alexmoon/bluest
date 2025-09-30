// TODO: remove this module after publishing a new version of `java-spaghetti`.

use std::cell::{Cell, OnceCell, RefCell};
use std::ptr::null_mut;
use std::slice::from_raw_parts;

use java_spaghetti::sys::*;
use java_spaghetti::{ByteArray, Env, Local, PrimitiveArray, Ref, ReferenceType};

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

        let env = unsafe { Env::from_raw(env as _) };
        // pushing/popping local frame is a workaround for the local reference leakage bug in `java-spaghetti` 0.2.0.
        increase_nest_level(env);
        let result = callback(env);
        decrease_nest_level(env);

        if just_attached && get_thread_exit_flag() {
            // this is needed in case of `with_env` is used on dropping some thread-local instance.
            unsafe { ((**self.0).v1_2.DetachCurrentThread)(self.0) };
        }

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
    static THREAD_ATTACH_FLAG: RefCell<Option<AttachFlag>> = const { RefCell::new(None) };
    static WITH_ENV_NEST_LEVEL: Cell<usize> = const { Cell::new(0) };
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

/// Checks if the current thread is attached to the JVM by `VM::with_env` defined above.
fn get_thread_attach_flag() -> bool {
    THREAD_ATTACH_FLAG
        .try_with(|flag| flag.borrow().is_some())
        .unwrap_or(false)
}

fn set_thread_attach_flag(raw_vm: *mut JavaVM) {
    THREAD_ATTACH_FLAG.replace(Some(AttachFlag { raw_vm }));
}

fn get_thread_exit_flag() -> bool {
    THREAD_EXIT_FLAG.try_with(|flag| flag.get().is_some()).unwrap_or(true)
}

// NOTE: It is tested that creating more than 512 local references might not throw a fatal error
// on Android 8.0 and above, and on Android 7.0 the limit is 512 even if this is not done.
// Creating local references that exceeds the specified capacity of the `PushLocalFrame` call
// does not throw a fatal error (tested on Android 7.0, 9.0 and 13.0).

fn increase_nest_level<'env>(env: Env<'env>) {
    let local_frame_size = if get_thread_attach_flag() { 512 } else { 256 };
    let Ok(level) = WITH_ENV_NEST_LEVEL.try_with(|level| level.get()) else {
        return;
    };
    if level == 0 {
        let jnienv = env.as_raw();
        let result = unsafe { ((**jnienv).v1_2.PushLocalFrame)(jnienv, local_frame_size) };
        assert_eq!(result, JNI_OK);
    }
    WITH_ENV_NEST_LEVEL.replace(level + 1);
}

fn decrease_nest_level<'env>(env: Env<'env>) {
    let Ok(level) = WITH_ENV_NEST_LEVEL.try_with(|level| level.get()) else {
        return;
    };
    if level == 1 {
        let jnienv = env.as_raw();
        let _ = unsafe { ((**jnienv).v1_2.PopLocalFrame)(jnienv, null_mut()) };
    }
    if level > 0 {
        WITH_ENV_NEST_LEVEL.replace(level - 1);
    }
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

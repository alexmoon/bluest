#![allow(unused)] // used depending on the target.

use std::mem::ManuallyDrop;

pub struct ScopeGuard<F: FnOnce()> {
    dropfn: ManuallyDrop<F>,
}

impl<F: FnOnce()> ScopeGuard<F> {
    pub fn defuse(mut self) {
        let mut guard = ManuallyDrop::new(self);
        unsafe { ManuallyDrop::drop(&mut guard.dropfn) }
    }
}

impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        // SAFETY: This is OK because `dropfn` is `ManuallyDrop` which will not be dropped by the compiler.
        let dropfn = unsafe { ManuallyDrop::take(&mut self.dropfn) };
        dropfn();
    }
}

pub fn defer<F: FnOnce()>(dropfn: F) -> ScopeGuard<F> {
    ScopeGuard {
        dropfn: ManuallyDrop::new(dropfn),
    }
}

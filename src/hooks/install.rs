use std::mem;

use closure_ffi::traits::{FnPtr, FnThunk};
use winhook::{CConv, HookInstaller};

#[track_caller]
pub unsafe fn hook<F, C, H>(f: F, c: C)
where
    F: FnPtr + CConv + 'static,
    C: FnOnce(F) -> H + 'static,
    H: Send + Sync + 'static,
    (F::CC, H): FnThunk<F> + Send + Sync + 'static,
{
    unsafe {
        HookInstaller::for_function(f)
            .enable(true)
            .install(c)
            .map(mem::forget)
            .unwrap()
    }
}

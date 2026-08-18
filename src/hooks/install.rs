use closure_ffi::traits::{FnPtr, FnThunk};
use diversion::{
    hook::{Static, leak::StaticHook},
    install,
};

#[track_caller]
pub unsafe fn hook<F, C, H>(f: F, c: C) -> eyre::Result<()>
where
    F: FnPtr + 'static,
    C: FnOnce(Static<F>) -> H + 'static,
    H: Send + Sync + 'static,
    (F::CC, H): FnThunk<F> + Send + Sync + 'static,
{
    unsafe {
        install(f)?.static_hook(c);
        Ok(())
    }
}

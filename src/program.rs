use std::{ffi::c_void, mem, sync::LazyLock};

use pelite::pe::PeView;
use windows::{
    Win32::{Foundation::HMODULE, System::LibraryLoader::GetModuleHandleW},
    core::Error,
    core::PCWSTR,
};

use crate::rva::Rva;

#[derive(Clone, Copy, Debug, Hash)]
pub struct Program(*mut c_void);

impl Program {
    pub fn current() -> Self {
        Self::try_current().expect("GetModuleHandleW failed")
    }

    pub fn try_current() -> Result<Self, Error> {
        static CURRENT: LazyLock<Result<Program, Error>> = LazyLock::new(|| unsafe {
            GetModuleHandleW(PCWSTR::null()).map(Program::from_hmodule)
        });
        CURRENT.clone()
    }

    /// # Safety
    ///
    /// Safe, but using the resulting pointer is incredibly unsafe.
    pub fn derva<T>(self, rva: Rva) -> *mut T {
        self.0.wrapping_byte_add(*rva as usize).cast()
    }

    /// # Safety
    ///
    /// Incredibly unsafe and may cause spontaneous burst pipes, gas leaks, explosions, etc.
    pub unsafe fn derva_ptr<P>(self, rva: Rva) -> P {
        assert!(size_of::<P>() == size_of::<*mut ()>());
        unsafe { mem::transmute_copy::<*mut (), P>(&self.derva::<()>(rva)) }
    }

    fn from_hmodule(HMODULE(base): HMODULE) -> Self {
        Self(base)
    }
}

impl From<Program> for PeView<'static> {
    fn from(value: Program) -> Self {
        unsafe { Self::module(value.0 as _) }
    }
}

unsafe impl Send for Program {}

unsafe impl Sync for Program {}

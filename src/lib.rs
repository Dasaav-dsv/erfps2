use std::ffi::c_void;

use windows::{
    Win32::{Foundation::HINSTANCE, System::SystemServices::DLL_PROCESS_ATTACH},
    core::BOOL,
};

use crate::{camera::init_camera_update, program::Program, shaders::hook_shaders};

mod camera;
mod config;
mod logger;
mod player;
mod program;
mod rva;
mod shaders;

fn main() -> eyre::Result<()> {
    let program = Program::try_current()?;

    init_camera_update(program)?;
    hook_shaders(program)?;

    Ok(())
}

#[unsafe(no_mangle)]
unsafe extern "system" fn DllMain(_: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        logger::init();
        logger::set_panic_hook();

        #[cfg(debug_assertions)]
        if libhotpatch::is_hotpatched() {
            return true.into();
        }

        std::thread::spawn(|| main().unwrap());
    }

    true.into()
}

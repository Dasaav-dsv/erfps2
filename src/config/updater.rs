use std::{
    ffi::OsString,
    fs, io,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

use toml::de::Error as TomlError;
use windows::{
    Win32::{
        Foundation::HMODULE,
        System::LibraryLoader::{
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            GetModuleFileNameW, GetModuleHandleExW,
        },
    },
    core::{Error as WinError, PCWSTR},
};

use crate::config::Config;

pub struct ConfigUpdater {
    config_path: Box<Path>,
    config: Config,
    last_update: Instant,
    last_timestamp: Option<SystemTime>,
}

impl ConfigUpdater {
    const CONFIG_NAME: &str = "erfps2.toml";
    const UPDATE_MS: u128 = 100;

    pub fn new() -> eyre::Result<Self> {
        let config_path = {
            let mut path = current_module_path()?;
            path.set_file_name(Self::CONFIG_NAME);
            path.into_boxed_path()
        };

        let config = Self::read_or_default(&config_path);

        let last_update = Instant::now();

        let last_timestamp = fs::metadata(&config_path)
            .and_then(|meta| meta.modified())
            .ok();

        Ok(Self {
            config_path,
            config,
            last_update,
            last_timestamp,
        })
    }

    pub fn get_or_update(&mut self) -> &Config {
        if self.last_update.elapsed().as_millis() > Self::UPDATE_MS
            && let Ok(timestamp) = fs::metadata(&self.config_path)
                .inspect_err(Self::report_fs_error)
                .and_then(|m| m.modified())
            && self.last_timestamp.is_none_or(|last| last != timestamp)
        {
            self.config = Self::read_or_default(&self.config_path);
            self.last_timestamp = Some(timestamp);
        }

        &self.config
    }

    fn read_or_default(path: &Path) -> Config {
        Self::try_read(path)
            .inspect_err(Self::report_fs_error)
            .unwrap_or_default()
    }

    fn try_read(path: &Path) -> Result<Config, io::Error> {
        let toml = fs::read_to_string(path)?;

        let config = toml::from_str(&toml)
            .inspect_err(Self::report_toml_error)
            .map_err(io::Error::other)?;

        Ok(config)
    }

    fn report_fs_error(error: &io::Error) {
        log::error!(
            "failed to update config: {error}. Is it placed in the same directory as erfps2.dll?"
        );
    }

    fn report_toml_error(error: &TomlError) {
        log::error!("error in config {}: {error}", Self::CONFIG_NAME);
    }
}

fn current_module_path() -> Result<PathBuf, WinError> {
    let module_handle = unsafe {
        fn in_module_dummy() {}
        let mut module_handle = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            PCWSTR(in_module_dummy as *const u16),
            &mut module_handle,
        )?;
        module_handle
    };

    // Approx. reasonable max length:
    // https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
    let mut module_filename = vec![0u16; 32767];

    unsafe {
        let len = GetModuleFileNameW(Some(module_handle), &mut module_filename);

        if len == 0 || len == 32767 {
            return Err(WinError::from_thread());
        }

        module_filename.truncate(len as usize);
    }

    let path = PathBuf::from(OsString::from_wide(&module_filename));
    Ok(path)
}

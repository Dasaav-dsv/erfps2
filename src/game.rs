use eldenring::cs::{GameDataMan, HudType};

use crate::{program::Program, rva::GAME_DATA_MAN_RVA};

pub trait GameDataManExt {
    unsafe fn instance() -> Option<&'static mut Self>;

    fn is_hud_enabled(&self) -> bool;
}

impl GameDataManExt for GameDataMan {
    unsafe fn instance() -> Option<&'static mut GameDataMan> {
        unsafe {
            Program::current()
                .derva::<*mut Self>(GAME_DATA_MAN_RVA)
                .as_mut()?
                .as_mut()
        }
    }

    fn is_hud_enabled(&self) -> bool {
        self.game_settings.hud_type != HudType::Off
    }
}

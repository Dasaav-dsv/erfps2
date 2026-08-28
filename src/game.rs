use eldenring::cs::{GameDataMan, HudType};

pub trait GameDataManExt {
    fn is_hud_enabled(&self) -> bool;
}

impl GameDataManExt for GameDataMan {
    fn is_hud_enabled(&self) -> bool {
        self.game_settings.hud_type != HudType::Off
    }
}

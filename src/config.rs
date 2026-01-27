use std::sync::LazyLock;

use serde::Deserialize;

use crate::config::toml::TOML_STR;

mod toml;
pub mod updater;

#[derive(Clone, Debug, Deserialize)]
#[serde(from = "toml::Config")]
pub struct Config {
    pub fov: f32,

    pub angle_limit: [f32; 2],

    pub start_in_first_person: bool,

    pub soft_lock_on: bool,

    pub prioritize_lock_on: bool,

    pub unlocked_movement: bool,

    pub unobtrusive_dodges: bool,

    pub track_dodges: bool,

    pub restricted_sprint: bool,

    pub use_stabilizer: bool,

    pub stabilizer_window: f32,

    pub stabilizer_factor: f32,

    pub crosshair: CrosshairKind,

    pub crosshair_scale: (f32, f32),

    pub use_fov_correction: bool,

    pub use_barrel_correction: bool,

    pub correction_strength: f32,

    pub correction_cylindricity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrosshairKind {
    None,
    Cross,
    Dot,
    Circle,
    CircleDot,
    Angled,
}

impl From<toml::Config> for Config {
    fn from(config: toml::Config) -> Self {
        let degrees = config.fov.horizontal_fov.clamp(45.0, 130.0);
        let fov = degrees.to_radians();
        let stabilizer_window = config.stabilizer.smoothing_window.clamp(0.1, 1.0);
        let stabilizer_factor = config.stabilizer.smoothing_factor.clamp(0.0, 1.0);

        let crosshair_scale_x = config.crosshair.scale_x.clamp(0.1, 4.0);
        let crosshair_scale_y = config.crosshair.scale_y.clamp(0.1, 4.0);

        let correction_strength = config.fov.fov_correction_strength.clamp(0.0, 1.0);
        let correction_cylindricity =
            config.fov.fov_correction_cylindricity.clamp(0.0, 1.0) * 1.5 + 0.5;

        let (use_fov_correction, use_barrel_correction) = match config.fov.fov_correction {
            toml::FovCorrection::None => (false, false),
            toml::FovCorrection::Fisheye => (true, false),
            toml::FovCorrection::Barrel => (true, true),
        };

        Self {
            fov,
            angle_limit: const { [f32::to_radians(-80.0), f32::to_radians(70.0)] },
            start_in_first_person: config.gameplay.start_in_first_person,
            prioritize_lock_on: config.gameplay.prioritize_lock_on,
            soft_lock_on: config.gameplay.soft_lock_on,
            unlocked_movement: config.gameplay.unlocked_movement,
            unobtrusive_dodges: config.gameplay.unobtrusive_dodges,
            track_dodges: config.gameplay.track_dodges,
            restricted_sprint: config.gameplay.restricted_sprint,
            use_stabilizer: config.stabilizer.enabled,
            stabilizer_window,
            stabilizer_factor,
            crosshair: config.crosshair.kind,
            crosshair_scale: (crosshair_scale_x, crosshair_scale_y),
            use_fov_correction,
            use_barrel_correction,
            correction_strength,
            correction_cylindricity,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        static DEFAULT: LazyLock<Config> = LazyLock::new(|| ::toml::from_str(TOML_STR).unwrap());
        DEFAULT.clone()
    }
}

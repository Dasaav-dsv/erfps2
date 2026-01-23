use serde::Deserialize;

pub mod updater;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub fov: Fov,
    pub gameplay: Gameplay,
    pub stabilizer: Stabilizer,
    pub crosshair: Crosshair,
}

#[derive(Debug, Deserialize)]
pub struct Fov {
    pub horizontal_fov: f32,
    pub fov_correction: FovCorrection,
    pub fov_correction_strength: f32,
    pub fov_correction_cylindricity: f32,
}

#[derive(Debug, Deserialize)]
pub struct Gameplay {
    pub start_in_first_person: bool,
    pub prioritize_lock_on: bool,
    pub soft_lock_on: bool,
    pub unlocked_movement: bool,
    pub unobtrusive_dodges: bool,
    pub track_dodges: bool,
    pub restricted_sprint: bool,
}

#[derive(Debug, Deserialize)]
pub struct Stabilizer {
    pub enabled: bool,
    pub smoothing_window: f32,
    pub smoothing_factor: f32,
}

#[derive(Debug, Deserialize)]
pub struct Crosshair {
    pub kind: CrosshairKind,
    pub scale_x: f32,
    pub scale_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FovCorrection {
    None,
    Fisheye,
    Barrel,
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

#[cfg(test)]
#[test]
fn check_dist_config() {
    let _ = toml::from_str::<Config>(include_str!("../dist/erfps2.toml")).unwrap();
}

use serde::Deserialize;

pub mod updater;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub fov: Fov,
    pub stabilizer: Stabilizer,
    pub crosshair: Crosshair,
}

#[derive(Debug, Deserialize)]
pub struct Fov {
    pub horizontal_fov: f32,
    pub fov_correction: FovCorrection,
    pub fov_correction_strength: f32,
}

#[derive(Debug, Deserialize)]
pub struct Stabilizer {
    pub enabled: bool,
    pub smoothing_window: f32,
    pub smoothing_factor: f32,
}

#[derive(Debug, Deserialize)]
pub struct Crosshair {
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FovCorrection {
    None,
    Fisheye,
    Barrel,
}

#[cfg(test)]
#[test]
fn check_dist_config() {
    let _ = toml::from_str::<Config>(include_str!("../dist/erfps2.toml")).unwrap();
}

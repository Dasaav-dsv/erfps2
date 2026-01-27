use serde::Deserialize;

use crate::config::CrosshairKind;

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

const WITH_COMMENTS: &str = include_str!("../../dist/erfps2.toml");

pub const TOML_STR: &str = {
    const STRIPPED_LEN: usize = {
        let bytes = WITH_COMMENTS.as_bytes();
        let mut i = 0;
        let mut len = 0;
        let mut skip = false;
        while i < bytes.len() {
            let byte = bytes[i];
            match (byte, skip) {
                (b'#', _) => skip = true,
                (b'\n', true) => skip = false,
                (_, false) => len += 1,
                _ => {}
            }
            i += 1;
        }
        len
    };
    const STRIPPED: [u8; STRIPPED_LEN] = {
        let bytes = WITH_COMMENTS.as_bytes();
        let mut stripped = [b' '; STRIPPED_LEN];
        let mut i = 0;
        let mut j = 0;
        let mut skip = false;
        while i < bytes.len() {
            let byte = bytes[i];
            match (byte, skip) {
                (b'#', _) => skip = true,
                (b'\n', true) => skip = false,
                (_, false) => {
                    stripped[j] = byte;
                    j += 1;
                }
                _ => {}
            }
            i += 1;
        }
        stripped
    };
    unsafe { str::from_utf8_unchecked(&STRIPPED) }
};

#[cfg(test)]
#[test]
fn check_dist_config() {
    toml::from_str::<Config>(WITH_COMMENTS).unwrap();
    toml::from_str::<Config>(TOML_STR).unwrap();
}

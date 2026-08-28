use std::{ops::Deref, sync::LazyLock};

use fromsoftware_shared::{GameVersion, LANG_ID_EN, LANG_ID_JP};

use crate::program::Program;

mod jp;
mod ww;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ERGameVersion {
    Ww270,
    Jp2701,
}

impl GameVersion for ERGameVersion {
    const NAME: &'static str = "elden ring";

    fn from_lang_version(lang_id: u16, version: &str) -> Option<Self> {
        match (lang_id, version) {
            (LANG_ID_EN, "2.7.0.0") => Some(Self::Ww270),
            (LANG_ID_JP, "2.7.0.1") => Some(Self::Jp2701),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rva {
    ww: u32,
    jp: u32,
}

impl Rva {
    const fn new(ww: u32, jp: u32) -> Self {
        Self { ww, jp }
    }
}

impl Deref for Rva {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        static GAME_VERSION: LazyLock<ERGameVersion> = LazyLock::new(|| {
            let program = Program::current();
            ERGameVersion::detect(&program.into())
                .expect("this game version is not supported; expected ELDEN RING 1.17.0")
        });

        match *GAME_VERSION {
            ERGameVersion::Ww270 => &self.ww,
            ERGameVersion::Jp2701 => &self.jp,
        }
    }
}

macro_rules! rva {
    ($($i:ident),*$(,)*) => {
        $(pub const $i: Rva = Rva::new(ww::$i, jp::$i);)*
    };
}

rva! {
    ADD_PIXEL_SHADER_RVA,
    CAMERA_STEP_UPDATE_RVA,
    CAM_HIT_COLLECTOR_RVA,
    CAM_WALL_RECOVERY_RVA,
    CAST_SHAPE_RVA,
    CB_FISHEYE_HOOK_RVA,
    CHR_CAN_TARGET_RVA,
    CHR_ROOT_MOTION_RVA,
    CHR_TAE_ANIM_EVENT_VMT_RVA,
    FOLLOW_CAM_FOLLOW_RVA,
    GET_DMY_POS_RVA,
    GX_FFX_DRAW_CONTEXT_RVA,
    GX_FFX_DRAW_PASS_RVA,
    HKNP_SPHERE_SHAPE_RVA,
    LOAD_TPF_RES_CAP_RVA,
    MMS_UPDATE_CHR_CAM_RVA,
    POSTURE_CONTROL_RIGHT_RVA,
    SET_WWISE_LISTENER_RVA,
    SHOW_TUTORIAL_POPUP,
    UPDATE_CHR_MODEL_POS_RVA,
    UPDATE_FE_MAN_RVA,
    UPDATE_FOLLOW_CAM_RVA,
    UPDATE_LOCK_TGT_RVA,
    USES_DITHERING_RVA,
}

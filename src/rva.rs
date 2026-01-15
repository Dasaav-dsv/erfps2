use std::{ops::Deref, sync::LazyLock};

use eldenring::version::GameVersion;

use crate::program::Program;

mod jp;
mod ww;

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
        static GAME_VERSION: LazyLock<GameVersion> = LazyLock::new(|| {
            let program = Program::current();
            GameVersion::detect(&program.into())
                .expect("this game version is not supported; expected ELDEN RING 1.16.1")
        });

        match *GAME_VERSION {
            GameVersion::Ww261 => &self.ww,
            GameVersion::Jp2611 => &self.jp,
        }
    }
}

macro_rules! rva {
    ($i:ident) => {
        pub const $i: Rva = Rva::new(ww::$i, jp::$i);
    };
}

rva!(ADD_PIXEL_SHADER_RVA);
rva!(CAMERA_STEP_UPDATE_RVA);
rva!(CAM_WALL_RECOVERY_RVA);
rva!(CB_FISHEYE_HOOK_RVA);
rva!(CHR_ROOT_MOTION_RVA);
rva!(FOLLOW_CAM_FOLLOW_RVA);
rva!(GET_BEH_GRAPH_DATA_RVA);
rva!(GET_DMY_POS_RVA);
rva!(GX_FFX_DRAW_CONTEXT_RVA);
rva!(GX_FFX_DRAW_PASS_RVA);
rva!(IS_CHR_RIDING_RVA);
rva!(MMS_UPDATE_CHR_CAM_RVA);
rva!(POSTURE_CONTROL_RIGHT_RVA);
rva!(PUSH_TAE700_MODIFIER_RVA);
rva!(SET_WWISE_LISTENER_RVA);
rva!(UPDATE_FOLLOW_CAM_RVA);
rva!(UPDATE_LOCK_TGT_RVA);
rva!(USES_DITHERING_RVA);

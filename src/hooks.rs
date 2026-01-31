use std::{ffi::c_void, mem, ptr};

use eldenring::{
    cs::{
        CSCam, CSChrBehaviorDataModule, CSChrPhysicsModule, CSFeManImp, ChrCtrl, ChrExFollowCam,
        ChrIns, PlayerIns,
    },
    fd4::FD4Time,
};
use fromsoftware_shared::{F32Vector4, F32ViewMatrix, FromStatic};
use glam::{Vec2, Vec3Swizzles, Vec4};

use crate::{
    core::{
        BehaviorState, CoreLogic,
        world::{Void, World, WorldState},
    },
    hooks::install::hook,
    player::PlayerExt,
    program::Program,
    rva::{
        CAMERA_STEP_UPDATE_RVA, CHR_ROOT_MOTION_RVA, FOLLOW_CAM_FOLLOW_RVA, MMS_UPDATE_CHR_CAM_RVA,
        POSTURE_CONTROL_RIGHT_RVA, PUSH_TAE700_MODIFIER_RVA, SET_WWISE_LISTENER_RVA,
        UPDATE_CHR_MODEL_POS_RVA, UPDATE_FE_MAN_RVA, UPDATE_FOLLOW_CAM_RVA, UPDATE_LOCK_TGT_RVA,
    },
    shaders::screen::correct_screen_coords,
};

pub mod install;

pub fn init_camera_update(program: Program) -> eyre::Result<()> {
    unsafe {
        let update = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, *const FD4Time)>(CAMERA_STEP_UPDATE_RVA);

        hook(update, |original| {
            move |param_1, param_2| update_camera(&|| original(param_1, param_2))
        });

        let mms_update =
            program.derva_ptr::<unsafe extern "C" fn(*mut c_void)>(MMS_UPDATE_CHR_CAM_RVA);

        hook(mms_update, |original| {
            move |param_1| update_move_map_step(&|| original(param_1))
        });

        let chr_model_pos_update =
            program.derva_ptr::<unsafe extern "C" fn(*mut ChrCtrl)>(UPDATE_CHR_MODEL_POS_RVA);

        hook(chr_model_pos_update, |original| {
            move |param_1| update_chr_model_pos(param_1, &|| original(param_1))
        });

        let lock_tgt_update =
            program.derva_ptr::<unsafe extern "C" fn(*mut c_void, f32)>(UPDATE_LOCK_TGT_RVA);

        hook(lock_tgt_update, |original| {
            move |param_1, param_2| update_lock_tgt(&|| original(param_1, param_2))
        });

        let chr_follow_cam_update =
            program.derva_ptr::<unsafe extern "C" fn(*mut ChrExFollowCam)>(UPDATE_FOLLOW_CAM_RVA);

        hook(chr_follow_cam_update, |original| {
            move |param_1| update_chr_follow_cam(&mut *param_1, &|| original(param_1))
        });

        let fe_man_update = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, f32, *mut bool, *mut c_void)>(
                UPDATE_FE_MAN_RVA,
            );

        hook(fe_man_update, |original| {
            move |param_1, param_2, param_3, param_4| {
                update_fe_man();
                original(param_1, param_2, param_3, param_4);
            }
        });

        let follow_cam_follow = program
            .derva_ptr::<unsafe extern "C" fn(*mut ChrExFollowCam, f32, *mut c_void)>(
                FOLLOW_CAM_FOLLOW_RVA,
            );

        hook(follow_cam_follow, |original| {
            move |param_1, param_2, param_3| {
                // Setting this flag disables position interpolation for the camera attach
                // point in the function below.
                let first_person = CoreLogic::is_first_person();
                let reset_camera = mem::replace(&mut (*param_1).reset_camera, first_person);

                original(param_1, param_2, param_3);

                (*param_1).reset_camera = reset_camera;
            }
        });

        let set_wwise_listener = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, *const CSCam) -> u32>(
                SET_WWISE_LISTENER_RVA,
            );

        hook(set_wwise_listener, |original| {
            move |param_1, param_2| {
                let mut param_2 = param_2.read();

                if let Some(listener) = wwise_listener_for_fp() {
                    param_2.matrix = listener;
                }

                original(param_1, &param_2)
            }
        });

        let push_tae700_modifier = program
            .derva_ptr::<unsafe extern "C" fn(*mut CSChrBehaviorDataModule, *mut [f32; 8])>(
                PUSH_TAE700_MODIFIER_RVA,
            );

        hook(push_tae700_modifier, |original| {
            move |param_1, param_2| {
                if let Some(player) = PlayerIns::main_player()
                    && &raw mut player.chr_ins == (*param_1).owner.as_ptr()
                {
                    tae700_override(&mut *param_2);
                }

                original(param_1, param_2);
            }
        });

        let posture_control_right =
            program
                .derva_ptr::<unsafe extern "C" fn(*mut *mut *mut PlayerIns, u8, i32, i32) -> i32>(
                    POSTURE_CONTROL_RIGHT_RVA,
                );

        hook(posture_control_right, |original| {
            move |param_1, param_2, param_3, param_4| {
                let posture_angle = hand_posture_control(**param_1).unwrap_or(0);
                original(param_1, param_2, param_3, param_4) + posture_angle
            }
        });

        let chr_root_motion = program.derva_ptr::<unsafe extern "C" fn(
            *mut CSChrPhysicsModule,
            *mut F32Vector4,
            *mut F32Vector4,
            *mut c_void,
        )>(CHR_ROOT_MOTION_RVA);

        hook(chr_root_motion, |original| {
            move |param_1, param_2, param_3, param_4| {
                let mut param_3 = *param_3;

                if let Some(root_motion) = root_motion_modifier((*param_1).owner.as_ptr(), param_3)
                {
                    param_3 = root_motion;
                }

                original(param_1, param_2, &mut param_3, param_4);
            }
        });
    }

    Ok(())
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_chr_model_pos(chr_ctrl: *mut ChrCtrl, original: &dyn Fn()) {
    original();

    if CoreLogic::scope::<Void, _>(|context| {
        context
            .get::<PlayerIns>()
            .is_some_and(|player| player.chr_ctrl.as_ptr() == chr_ctrl)
            && context.first_person()
    }) {
        CoreLogic::scope_mut::<World, _>(|context| context.update_chr_model_pos());
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_camera(original: &dyn Fn()) {
    let camera_updated = CoreLogic::scope_mut::<World, _>(|context| {
        context.update_cs_cam();
        context.first_person()
    });

    if matches!(camera_updated, None | Some(false)) {
        original();
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_move_map_step(original: &dyn Fn()) {
    CoreLogic::scope_mut::<Void, _>(|context| context.next_frame());

    CoreLogic::scope_mut::<World, _>(|context| {
        context.update_behavior_states();
        context.try_transition();
        context.update_chr_cam();
    });

    original();
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_lock_tgt(original: &dyn Fn()) {
    original();

    CoreLogic::scope_mut::<World, _>(|context| {
        if context.first_person()
            && !context.can_transition()
            && !context.is_player_sprinting()
            && !context.player.is_riding()
            && !context.player.has_action_request()
            && !context.player.is_in_throw()
            && !context.has_state(BehaviorState::Evasion)
            && !context.has_state(BehaviorState::Gesture)
            && context.player.mimicry_asset < 0
        {
            context.player.set_lock_on(true);
        }
    });
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_chr_follow_cam(follow_cam: &mut ChrExFollowCam, original: &dyn Fn()) {
    CoreLogic::scope_mut::<Void, _>(|context| context.update_follow_cam(follow_cam));

    original();

    if CoreLogic::is_first_person() {
        follow_cam.locked_on_cam_offset = 0.0;
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_fe_man() {
    let Ok(fe_man) = (unsafe { CSFeManImp::instance() }) else {
        return;
    };

    const FE_XY: Vec2 = Vec2::new(1920.0, 1080.0);
    let correct_coords = |coords: &mut F32Vector4| {
        let screen_coords = Vec2::new(coords.0, coords.1);
        let corrected_screen_coords = correct_screen_coords(screen_coords / FE_XY) * FE_XY;

        coords.0 = corrected_screen_coords.x;
        coords.1 = corrected_screen_coords.y;
    };

    correct_coords(&mut fe_man.lock_on_pos);

    for tag in &mut fe_man.enemy_chr_tag_displays {
        if tag.is_visible {
            correct_coords(&mut tag.screen_pos);
        }
    }

    for tag in &mut fe_man.friendly_chr_tag_displays {
        if tag.is_visible {
            correct_coords(&mut tag.screen_pos);
        }
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn wwise_listener_for_fp() -> Option<F32ViewMatrix> {
    if !CoreLogic::is_first_person() {
        return None;
    }

    let player = unsafe { PlayerIns::main_player()? };
    let mut mtx = player.model_mtx();

    mtx.3.1 += 1.5;
    mtx.3 = mtx.3 - mtx.2;

    Some(mtx)
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn tae700_override(args: &mut [f32; 8]) {
    if !CoreLogic::is_first_person() {
        return;
    }

    args[4] = 0.0;
    args[5] = 0.0;
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn hand_posture_control(some_player: *const PlayerIns) -> Option<i32> {
    let main_player = unsafe { PlayerIns::main_player()? };
    let is_main_player = ptr::eq(some_player, main_player);

    if !is_main_player || main_player.is_2h() || !CoreLogic::is_first_person() {
        return None;
    }

    Some(15)
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn root_motion_modifier(
    some_chr: *const ChrIns,
    root_motion: F32Vector4,
) -> Option<F32Vector4> {
    let main_player = unsafe { PlayerIns::main_player()? };
    let is_main_player = ptr::addr_eq(some_chr, main_player);

    if !is_main_player
        || !CoreLogic::scope::<Void, _>(|context| {
            context.config.unlocked_movement
                && context.first_person()
                && context.has_state(BehaviorState::Attack)
        })
    {
        return None;
    }

    let movement_dir = main_player.input_move_dir();
    let movement_magnitude = movement_dir.length();

    if movement_magnitude < 0.01 {
        return None;
    }

    let scaled_root_motion = Vec4::from(root_motion).truncate() * movement_magnitude * 1.25;
    let directional_root_motion = scaled_root_motion.rotate_y(movement_dir.xz().to_angle());

    Some(directional_root_motion.extend(1.0).into())
}

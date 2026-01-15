use std::{
    ffi::{CStr, c_char, c_void},
    mem, ptr,
};

use eldenring::{
    cs::{CSCam, CSChrBehaviorDataModule, CSChrPhysicsModule, ChrExFollowCam, ChrIns, PlayerIns},
    fd4::FD4Time,
};
use fromsoftware_shared::{F32Vector4, F32ViewMatrix};
use glam::{Vec4, Vec4Swizzles};
use winhook::HookInstaller;

use crate::{
    camera::control::CameraControl,
    log_unwind,
    player::PlayerExt,
    program::Program,
    rva::{
        CAMERA_STEP_UPDATE_RVA, CHR_ROOT_MOTION_RVA, FOLLOW_CAM_FOLLOW_RVA, GET_BEH_GRAPH_DATA_RVA,
        MMS_UPDATE_CHR_CAM_RVA, POSTURE_CONTROL_RIGHT_RVA, PUSH_TAE700_MODIFIER_RVA,
        SET_WWISE_LISTENER_RVA, UPDATE_FOLLOW_CAM_RVA, UPDATE_LOCK_TGT_RVA,
    },
};

mod control;

pub fn init_camera_update(program: Program) -> eyre::Result<()> {
    unsafe {
        let update = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, *const FD4Time)>(CAMERA_STEP_UPDATE_RVA);

        HookInstaller::for_function(update)
            .enable(true)
            .install(move |original| {
                move |param_1, param_2| {
                    log_unwind!(update_camera((*param_2).time, &|| original(
                        param_1, param_2
                    )));
                }
            })
            .map(mem::forget)
            .unwrap();

        let mms_update =
            program.derva_ptr::<unsafe extern "C" fn(*mut c_void)>(MMS_UPDATE_CHR_CAM_RVA);

        HookInstaller::for_function(mms_update)
            .enable(true)
            .install(move |original| {
                move |param_1| log_unwind!(update_move_map_step(&|| original(param_1)))
            })
            .map(mem::forget)
            .unwrap();

        let lock_tgt_update =
            program.derva_ptr::<unsafe extern "C" fn(*mut c_void, f32)>(UPDATE_LOCK_TGT_RVA);

        HookInstaller::for_function(lock_tgt_update)
            .enable(true)
            .install(move |original| {
                move |param_1, param_2| log_unwind!(update_lock_tgt(&|| original(param_1, param_2)))
            })
            .map(mem::forget)
            .unwrap();

        let chr_follow_cam_update =
            program.derva_ptr::<unsafe extern "C" fn(*mut ChrExFollowCam)>(UPDATE_FOLLOW_CAM_RVA);

        HookInstaller::for_function(chr_follow_cam_update)
            .enable(true)
            .install(move |original| {
                move |param_1| {
                    log_unwind!(update_chr_follow_cam(&mut *param_1, &|| original(param_1)))
                }
            })
            .map(mem::forget)
            .unwrap();

        let follow_cam_follow = program
            .derva_ptr::<unsafe extern "C" fn(*mut ChrExFollowCam, f32, *mut c_void)>(
                FOLLOW_CAM_FOLLOW_RVA,
            );

        HookInstaller::for_function(follow_cam_follow)
            .enable(true)
            .install(move |original| {
                move |param_1, param_2, param_3| {
                    // Setting this flag disables position interpolation for the camera attach
                    // point in the function below.
                    let first_person = CameraControl::scope(|control| control.first_person());
                    let reset_camera = mem::replace(&mut (*param_1).reset_camera, first_person);

                    original(param_1, param_2, param_3);

                    (*param_1).reset_camera = reset_camera;
                }
            })
            .map(mem::forget)
            .unwrap();

        let set_wwise_listener = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, *const CSCam) -> u32>(
                SET_WWISE_LISTENER_RVA,
            );

        HookInstaller::for_function(set_wwise_listener)
            .enable(true)
            .install(move |original| {
                move |param_1, param_2| {
                    let mut param_2 = param_2.read();

                    log_unwind!({
                        if let Some(listener) = wwise_listener_for_fp() {
                            param_2.matrix = listener;
                        }
                    });

                    original(param_1, &param_2)
                }
            })
            .map(mem::forget)
            .unwrap();

        let push_tae700_modifier = program
            .derva_ptr::<unsafe extern "C" fn(*mut CSChrBehaviorDataModule, *mut [f32; 8])>(
                PUSH_TAE700_MODIFIER_RVA,
            );

        HookInstaller::for_function(push_tae700_modifier)
            .enable(true)
            .install(move |original| {
                move |param_1, param_2| {
                    if let Some(player) = PlayerIns::main_player()
                        && &raw mut player.chr_ins == (*param_1).owner.as_ptr()
                    {
                        log_unwind!(tae700_override(&mut *param_2));
                    }

                    original(param_1, param_2);
                }
            })
            .map(mem::forget)
            .unwrap();

        let posture_control_right =
            program
                .derva_ptr::<unsafe extern "C" fn(*mut *mut *mut PlayerIns, u8, i32, i32) -> i32>(
                    POSTURE_CONTROL_RIGHT_RVA,
                );

        HookInstaller::for_function(posture_control_right)
            .enable(true)
            .install(|original| {
                move |param_1, param_2, param_3, param_4| {
                    let posture_angle = log_unwind!(hand_posture_control(**param_1).unwrap_or(0));
                    original(param_1, param_2, param_3, param_4) + posture_angle
                }
            })
            .map(mem::forget)
            .unwrap();

        let chr_root_motion = program.derva_ptr::<unsafe extern "C" fn(
            *mut CSChrPhysicsModule,
            *mut F32Vector4,
            *mut F32Vector4,
            *mut c_void,
        )>(CHR_ROOT_MOTION_RVA);

        HookInstaller::for_function(chr_root_motion)
            .enable(true)
            .install(|original| {
                move |param_1, param_2, param_3, param_4| {
                    let mut param_3 = *param_3;

                    if let Some(root_motion) =
                        log_unwind!(root_motion_modifier((*param_1).owner.as_ptr(), param_3))
                    {
                        param_3 = root_motion;
                    }

                    original(param_1, param_2, &mut param_3, param_4);
                }
            })
            .map(mem::forget)
            .unwrap();

        let get_beh_graph =
            program
                .derva_ptr::<unsafe extern "C" fn(*mut c_void, *mut c_void, u32) -> *mut c_void>(
                    GET_BEH_GRAPH_DATA_RVA,
                );

        HookInstaller::for_function(get_beh_graph)
            .enable(true)
            .install(|original| {
                move |param_1, param_2, param_3| {
                    let result = original(param_1, param_2, param_3);
                    log_unwind!(update_player_behavior_state(result, param_2));
                    result
                }
            })
            .map(mem::forget)
            .unwrap();
    }

    Ok(())
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_camera(tpf: f32, original: &dyn Fn()) {
    let camera_updated = CameraControl::scope_mut(|control| {
        control.tpf = tpf;

        if control.first_person()
            && let (state, Some(context)) = control.state_and_context()
        {
            context.update_cs_cam(state);
            return true;
        }

        false
    });

    if !camera_updated {
        original();
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_move_map_step(original: &dyn Fn()) {
    CameraControl::scope_mut(|control| {
        control.next_frame();

        if let (state, Some(context)) = control.state_and_context() {
            context.try_transition(state);
            context.update_chr_cam(state);
        }
    });

    original();
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_lock_tgt(original: &dyn Fn()) {
    original();

    CameraControl::scope_mut(|control| {
        if control.first_person()
            && let (state, Some(context)) = control.state_and_context()
            && !state.can_transition()
            && !context.player.is_sprinting()
            && !context.player.is_riding()
            && !context.player.is_on_ladder()
            && !context.player.is_in_throw()
        {
            context.player.set_lock_on(true);
        }
    });
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_chr_follow_cam(follow_cam: &mut ChrExFollowCam, original: &dyn Fn()) {
    CameraControl::scope_mut(|control| control.state_and_context().0.update_follow_cam(follow_cam));

    original();

    if CameraControl::scope(|control| control.first_person()) {
        follow_cam.locked_on_cam_offset = 0.0;
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn wwise_listener_for_fp() -> Option<F32ViewMatrix> {
    if !CameraControl::scope(|control| control.first_person()) {
        return None;
    }

    let player = PlayerIns::main_player()?;
    let mut mtx = player.model_mtx();

    mtx.3.1 += 1.5;
    mtx.3 = mtx.3 - mtx.2;

    Some(mtx)
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn tae700_override(args: &mut [f32; 8]) {
    if !CameraControl::scope(|control| control.first_person()) {
        return;
    }

    args[4] = 0.0;
    args[5] = 0.0;
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn hand_posture_control(some_player: *const PlayerIns) -> Option<i32> {
    let first_person = || CameraControl::scope(|control| control.first_person());

    let main_player = PlayerIns::main_player()?;
    let is_main_player = ptr::eq(some_player, main_player);

    if !is_main_player || main_player.is_2h() || !first_person() {
        return None;
    }

    Some(15)
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn root_motion_modifier(
    some_chr: *const ChrIns,
    root_motion: F32Vector4,
) -> Option<F32Vector4> {
    let main_player = PlayerIns::main_player()?;
    let is_main_player = ptr::addr_eq(some_chr, main_player);

    if !is_main_player
        || !CameraControl::scope_mut(|control| {
            let unlocked_movement = control.unlocked_movement && control.first_person();
            let context = unlocked_movement.then(|| control.state_and_context());
            matches!(context, Some((_, Some(context))) if context.has_state("Attack_SM"))
        })
    {
        return None;
    }

    let movement_dir = -Vec4::from(main_player.chr_ctrl.input_move_dir).zx();
    let movement_magnitude = movement_dir.length();

    if movement_magnitude < 0.01 {
        return None;
    }

    let scaled_root_motion = Vec4::from(root_motion).truncate() * movement_magnitude * 1.25;
    let directional_root_motion = scaled_root_motion.rotate_y(movement_dir.to_angle());

    Some(directional_root_motion.extend(1.0).into())
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_player_behavior_state(state_machine: *mut c_void, behavior_graph: *mut c_void) {
    if !state_machine.is_null()
        && let Some(player) = PlayerIns::main_player()
        && ptr::addr_eq(
            behavior_graph,
            player
                .module_container
                .behavior
                .hkb_context
                .hkb_character
                .behavior_graph,
        )
    {
        let name = unsafe { *state_machine.byte_add(0x48).cast::<*const c_char>() };

        if !name.is_null()
            && let Ok(name) = unsafe { CStr::from_ptr(name).to_str() }
        {
            CameraControl::scope_mut(|control| {
                if let (_, Some(context)) = control.state_and_context() {
                    context.behavior_states.push(name.into());
                }
            });
        }
    }
}

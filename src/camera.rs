use std::{ffi::c_void, mem};

use eldenring::{
    cs::{CSCam, CSChrBehaviorDataModule, ChrExFollowCam, PlayerIns},
    fd4::FD4Time,
};
use fromsoftware_shared::F32ViewMatrix;
use winhook::HookInstaller;

use crate::{
    camera::control::CameraControl,
    log_unwind,
    player::PlayerExt,
    program::Program,
    rva::{
        CAMERA_STEP_UPDATE_RVA, FOLLOW_CAM_FOLLOW_RVA, MMS_UPDATE_CHR_CAM_RVA,
        POSTURE_CONTROL_RIGHT_RVA, PUSH_TAE700_MODIFIER_RVA, SET_WWISE_LISTENER_RVA,
        UPDATE_FOLLOW_CAM_RVA, UPDATE_LOCK_TGT_RVA,
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
                    let first_person = CameraControl::lock().first_person();
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

        let posture_control_right = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, u8, i32, i32) -> i32>(
                POSTURE_CONTROL_RIGHT_RVA,
            );

        HookInstaller::for_function(posture_control_right)
            .enable(true)
            .install(|original| {
                move |param_1, param_2, param_3, param_4| {
                    let posture_angle = hand_posture_control().unwrap_or(0);
                    original(param_1, param_2, param_3, param_4) + posture_angle
                }
            })
            .map(mem::forget)
            .unwrap();
    }

    Ok(())
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_camera(tpf: f32, original: &dyn Fn()) {
    let mut control = CameraControl::lock();
    control.tpf = tpf;

    if !control.first_person() {
        drop(control);
        original();
        return;
    }

    let (state, Some(context)) = control.state_and_context() else {
        drop(control);
        original();
        return;
    };

    context.update_cs_cam(state);
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_move_map_step(original: &dyn Fn()) {
    let mut control = CameraControl::lock();
    control.next_frame();

    if let (state, Some(context)) = control.state_and_context() {
        context.try_transition(state);

        if state.first_person() {
            context.update_chr_cam(state);
        }
    }

    drop(control);
    original();
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_lock_tgt(original: &dyn Fn()) {
    original();

    let mut control = CameraControl::lock();

    if control.first_person()
        && let (_, Some(context)) = control.state_and_context()
        && !context.player.is_sprinting()
        && !context.player.is_riding()
        && !context.player.is_on_ladder()
        && !context.player.is_in_throw()
    {
        context.player.set_lock_on(true);
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn update_chr_follow_cam(follow_cam: &mut ChrExFollowCam, original: &dyn Fn()) {
    let mut control = CameraControl::lock();
    let (state, _) = control.state_and_context();

    state.update_follow_cam(follow_cam);

    drop(control);
    original();

    if CameraControl::lock().first_person() {
        follow_cam.locked_on_cam_offset = 0.0;
    }
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn wwise_listener_for_fp() -> Option<F32ViewMatrix> {
    if !CameraControl::lock().first_person() {
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
    if !CameraControl::lock().first_person() {
        return;
    }

    args[4] = 0.0;
    args[5] = 0.0;
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn hand_posture_control() -> Option<i32> {
    let control = CameraControl::lock();
    let player = PlayerIns::main_player()?;

    if !control.first_person() || player.is_2h() {
        return None;
    }

    Some(15)
}

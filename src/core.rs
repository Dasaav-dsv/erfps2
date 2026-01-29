use std::{
    f32::consts::PI,
    ffi::{CStr, c_char},
    mem,
    ops::{Deref, DerefMut},
    sync::{LazyLock, Once, RwLock},
};

use eldenring::cs::{
    CSActionButtonMan, CSEventFlagMan, CSRemo, ChrExFollowCam, ChrIns, FieldInsHandle,
    FieldInsType, GameDataMan, LockTgtMan, PlayerIns,
};
use fromsoftware_shared::{F32ViewMatrix, FromStatic};
use glam::{EulerRot, Mat3A, Mat4, Quat, Vec3, Vec4};

use crate::{
    config::{Config, CrosshairKind, updater::ConfigUpdater},
    core::{
        behavior::{BehaviorStateSet, BehaviorStates},
        head_tracker::HeadTracker,
        stabilizer::CameraStabilizer,
        world::{World, WorldState},
    },
    game::GameDataManExt,
    player::PlayerExt,
    program::Program,
    raycast::cast_sphere,
    rva::CAM_WALL_RECOVERY_RVA,
    shaders::{enable_dithering, enable_fov_correction, enable_vfx_fade, set_crosshair},
    tutorial::{TUTORIAL_EVENT_FLAG_ID, show_tutorial},
};

pub use behavior::BehaviorState;

pub mod world;

mod behavior;
mod head_tracker;
mod stabilizer;

pub struct CoreLogic {
    config: ConfigUpdater,
    state: State,
}

pub struct CoreLogicContext<'s, W> {
    pub config: &'s Config,
    pub world: W,
}

pub struct State {
    first_person: bool,
    should_transition: bool,
    frame: u64,
    tpf: f32,
    trans_time: f32,
    saved_angle_limit: Option<f32>,
    stabilizer: CameraStabilizer,
    head_tracker: HeadTracker,
    behavior_states: BehaviorStates,
}

impl CoreLogic {
    pub fn scope<W: WorldState, R>(
        f: impl for<'lt> FnOnce(CoreLogicContext<'_, W::With<'lt>>) -> R,
    ) -> W::Result<R> {
        let scoped = CoreLogic::get();

        let mut scoped = scoped.write().unwrap();
        let scoped = &mut *scoped;

        let config = scoped.config.get_or_update();
        let state = &mut scoped.state;

        W::in_world(state, move |world| f(CoreLogicContext { config, world }))
    }

    pub fn is_first_person() -> bool {
        CoreLogic::get().read().unwrap().state.first_person
    }

    fn get() -> &'static RwLock<CoreLogic> {
        static S: LazyLock<RwLock<CoreLogic>> = LazyLock::new(RwLock::default);
        &S
    }
}

impl Default for CoreLogic {
    fn default() -> Self {
        let mut config = ConfigUpdater::new().unwrap();
        let state = State::from_config(config.get_or_update());

        Self { config, state }
    }
}

impl State {
    fn from_config(config: &Config) -> Self {
        let tpf = const { 1.0 / 60.0 };
        let samples = (config.stabilizer_window / tpf).ceil() as u32;

        Self {
            first_person: false,
            should_transition: config.start_in_first_person,
            frame: 0,
            tpf: const { 1.0 / 60.0 },
            trans_time: 0.0,
            saved_angle_limit: None,
            stabilizer: CameraStabilizer::new(samples),
            head_tracker: HeadTracker::default(),
            behavior_states: BehaviorStates::default(),
        }
    }
}

impl<'s, W: WorldState> CoreLogicContext<'s, W> {
    pub fn first_person(&self) -> bool {
        let remo = unsafe { CSRemo::instance().ok() };
        let in_cutscene = remo
            .and_then(|remo| remo.remo_man.as_ref())
            .is_some_and(|ptr| ptr.state != 1);

        self.first_person && !in_cutscene
    }

    pub fn next_frame(&mut self) {
        self.frame += 1;
        self.update();
    }

    pub fn update_tpf(&mut self, tpf: f32) {
        self.tpf = tpf;
    }

    fn update(&mut self) {
        let samples = (self.config.stabilizer_window / self.tpf).ceil() as u32;
        self.stabilizer.set_sample_count(samples);

        self.update_fov_correction(self.config.fov);
    }

    fn update_fov_correction(&self, fov: f32) {
        enable_fov_correction(
            self.first_person && self.config.use_fov_correction,
            self.config.correction_strength,
            self.config.correction_cylindricity,
            self.config.use_barrel_correction,
            fov,
        );
    }

    pub fn update_follow_cam(&mut self, follow_cam: &mut ChrExFollowCam) {
        let first_person = self.first_person();

        unsafe {
            *Program::current().derva::<bool>(CAM_WALL_RECOVERY_RVA) &= !first_person;
        }

        follow_cam.camera_auto_rotation &= !first_person;

        if !first_person {
            if let Some(saved_angle_limit) = self.saved_angle_limit.take() {
                follow_cam.angle_limit[1] = saved_angle_limit;
            }

            return;
        }

        let angle_limit = mem::replace(&mut follow_cam.angle_limit, self.config.angle_limit);

        if angle_limit[1] != self.config.angle_limit[1] {
            self.saved_angle_limit = Some(angle_limit[1]);
        }

        if let Some(player) = PlayerIns::main_player()
            && player.is_approaching_ladder()
        {
            follow_cam.reset_camera_y = true;
            follow_cam.reset_camera_x = true;
        }

        if let Ok(lock_tgt) = unsafe { LockTgtMan::instance() } {
            let lock_chase_rate = &mut follow_cam.lock_chase_rate;

            if lock_tgt.is_locked_on && *lock_chase_rate <= 1.0 {
                *lock_chase_rate = f32::min(*lock_chase_rate + self.tpf, 1.0);
            } else if *lock_chase_rate > 0.3 {
                *lock_chase_rate = f32::max(*lock_chase_rate - self.tpf, 0.3);
            }
        }
    }
}

impl<'s> CoreLogicContext<'_, World<'s>> {
    pub fn can_transition(&self) -> bool {
        const STATE_TRANS_TIME: f32 = 0.233;
        self.trans_time > STATE_TRANS_TIME
    }

    pub fn try_transition(&mut self) {
        let Ok(action_button_man) = (unsafe { CSActionButtonMan::instance() }) else {
            return;
        };

        match action_button_man.is_use_pressed {
            true => self.trans_time += self.tpf,
            false => self.trans_time = 0.0,
        }

        let should_transition = self.should_transition;
        self.should_transition = false;

        if should_transition && (!self.config.prioritize_lock_on || !self.lock_tgt.is_locked_on) {
            self.first_person = !self.first_person;

            self.lock_tgt.is_lock_on_requested = false;

            self.update_fov_correction(self.fov());

            let first_person = self.first_person();

            enable_dithering(!first_person);
            enable_vfx_fade(!first_person);

            self.set_crosshair_if(first_person);

            self.player.enable_face_model(!first_person);
            self.player.enable_sheathed_weapons(!first_person);

            if !first_person {
                self.player.make_transparent(false);
                self.lock_tgt.lock_camera = true;

                self.chr_cam.ex_follow_cam.lock_chase_rate = 0.3;
                self.chr_cam.ex_follow_cam.max_lock_target_offset = 0.05;
            } else {
                self.chr_cam.ex_follow_cam.max_lock_target_offset = 0.0;
            }
        }

        if self.first_person()
            && self
                .player
                .module_container
                .action_request
                .movement_request_duration
                > 0.1
        {
            self.show_tutorial();
        }

        let is_lock_on_toggled = self.lock_tgt.is_locked_on != self.lock_tgt.is_lock_on_requested;
        let should_not_lock_on =
            (!self.lock_tgt.is_locked_on || !self.config.prioritize_lock_on) && is_lock_on_toggled;

        if self.can_transition()
            && should_not_lock_on
            && self.player.module_container.action_request.action_timers.r3 > 0.0
        {
            self.should_transition = true;

            if !self.config.prioritize_lock_on {
                self.lock_tgt.is_lock_on_requested = false;
            }
        }
    }

    pub fn camera_position(&mut self) -> F32ViewMatrix {
        let frame = self.frame;

        let head_mtx = self.player.head_position();

        let head_rotation = head_mtx.rotation();
        let camera_rotation = Quat::from_mat3a(&self.chr_cam.pers_cam.matrix.rotation());

        let mut head_pos = head_mtx.translation();

        if self.config.use_stabilizer {
            let player_mtx = Mat4::from(self.player.chr_ctrl.model_matrix);
            let local_head_pos = player_mtx.inverse().transform_point3(head_pos);

            let stabilized = self.stabilizer.next(frame, local_head_pos);
            let delta = stabilized - local_head_pos;

            head_pos = player_mtx.transform_point3(
                local_head_pos + delta.clamp_length_max(self.config.stabilizer_factor * 0.1),
            );
        }

        let frame_time = self.tpf;
        let tracking_rotation = if self.player.is_in_throw()
            || (self.config.track_damage && self.has_state(BehaviorState::Damage))
            || (self.config.track_dodges && self.has_state(BehaviorState::Evasion))
        {
            self.head_tracker.next_tracked(frame, frame_time, head_rotation)
        } else {
            self.head_tracker.next_untracked(frame, frame_time, head_rotation)
        };

        let camera_rotation = camera_rotation * tracking_rotation;

        let cam_pitch = camera_rotation.to_euler(EulerRot::ZXY).1;
        let cam_pitch_exp = (cam_pitch.abs() / 3.0).powi(2);

        let head_pitch = head_rotation.to_euler(EulerRot::ZXY).1;
        let head_upright = (1.0 - head_pitch.abs() / PI / 2.0).max(0.0).sqrt();

        let world_contrib = Vec3::new(0.0, 0.1, 0.0);
        let head_contrib = Vec3::new(0.0, -0.1 * head_upright, -0.05);
        let cam_contrib =
            Vec3::new(0.0, 0.03 + cam_pitch_exp, -0.025 + cam_pitch.abs() / 12.0) * head_upright;

        head_pos += world_contrib
            + head_rotation.transpose() * head_contrib
            + camera_rotation.inverse() * cam_contrib;

        Mat4::from_rotation_translation(camera_rotation, head_pos).into()
    }

    pub fn update_cs_cam(&mut self) {
        if !self.first_person() {
            return;
        }

        let camera_pos = self.camera_position();

        if self.config.soft_lock_on || !self.lock_tgt.is_locked_on {
            let lock_on_pos =
                Vec4::from(camera_pos.3) + Vec4::from(self.chr_cam.pers_cam.matrix.2) * 10.0;
            self.player.set_lock_on_target_position(lock_on_pos);
        }

        self.cs_cam.pers_cam_1.matrix = self.chr_cam.pers_cam.matrix;

        self.cs_cam.pers_cam_1.matrix.3 = camera_pos.3;
        self.chr_cam.pers_cam.matrix.3 = camera_pos.3;

        *self.player.aim_mtx_mut() = self.cs_cam.pers_cam_1.matrix;

        let fov = self.fov();

        self.cs_cam.pers_cam_1.fov = fov;
        self.chr_cam.pers_cam.fov = fov;
    }

    pub fn update_chr_cam(&mut self) {
        let first_person = self.first_person();

        self.set_crosshair_if(
            first_person
                && (!self.lock_tgt.is_locked_on || self.config.soft_lock_on)
                && !self.player.chr_flags1c5.precision_shooting(),
        );

        if !first_person {
            return;
        }

        self.player.enable_face_model(false);
        self.player.enable_sheathed_weapons(false);

        if self.config.unobtrusive_dodges {
            let is_dodging = self.has_state(BehaviorState::Evasion);
            self.player.make_transparent(is_dodging);
        }

        let camera_pos = self.camera_position();

        if self.config.restricted_sprint {
            self.restrict_sprint(camera_pos.rotation());
        }

        if self.config.soft_lock_on {
            self.soft_lock_on(camera_pos);
        }

        self.cs_cam.pers_cam_1.matrix = camera_pos;
        self.chr_cam.pers_cam.matrix = camera_pos;

        let fov = self.fov();

        self.cs_cam.pers_cam_1.fov = fov;
        self.chr_cam.pers_cam.fov = fov;
    }

    pub fn fov(&self) -> f32 {
        if !self.player.chr_flags1c5.precision_shooting() {
            return self.config.fov;
        }

        let aim_cam_fov = self.chr_cam.aim_cam.fov;
        if aim_cam_fov <= self.config.fov {
            return aim_cam_fov;
        }

        // f32::to_radians(25.0).atan()
        const AIM_CAM_HALF_WIDTH: f32 = 0.41143;
        let width_ratio = aim_cam_fov.atan() / AIM_CAM_HALF_WIDTH;

        f32::tan(self.config.fov.atan() * width_ratio)
    }

    pub fn is_player_sprinting(&self) -> bool {
        if self.config.restricted_sprint {
            self.player.is_sprinting()
        } else {
            self.player.is_sprint_requested()
        }
    }

    pub fn restrict_sprint(&mut self, camera_rotation: Mat3A) {
        let movement_dir = camera_rotation * self.player.input_move_dir();
        let movement_dir_xz = Vec3::new(-movement_dir.x, 0.0, movement_dir.z).normalize_or_zero();

        let player_dir = Vec4::from(self.player.chr_ctrl.model_matrix.0).truncate();
        let angle_from_movement = movement_dir_xz.dot(player_dir);

        if angle_from_movement < 0.5 {
            self.player.cancel_sprint();
        }
    }

    pub fn lock_on_to(&mut self, chr_handle: FieldInsHandle) {
        let mut next_node = self.lock_tgt.nodes;
        let mut target_node = None;

        while let Some(node) = next_node.map(|mut ptr| unsafe { ptr.as_mut() }) {
            next_node = node.next;
            node.flags &= !32;

            if unsafe { node.value.as_ref().chr_handle == chr_handle } {
                target_node = Some(node);
            }
        }

        if let Some(target_node) = target_node {
            target_node.flags |= 32;

            self.lock_tgt.is_locked_on = true;
            self.lock_tgt.is_lock_on_requested = true;
        }
    }

    pub fn has_state(&self, state: BehaviorState) -> bool {
        self.behavior_states.has_state(state)
    }

    fn set_crosshair_if(&self, cond: bool) {
        let is_hud_enabled = unsafe {
            GameDataMan::instance().is_some_and(|game_data_man| game_data_man.is_hud_enabled())
        };

        let crosshair = if cond && is_hud_enabled {
            self.config.crosshair
        } else {
            CrosshairKind::None
        };

        set_crosshair(crosshair, self.config.crosshair_scale);
    }

    pub fn update_behavior_states(&mut self) {
        let mut behavior_set = BehaviorStateSet::default();

        for node in self
            .player
            .module_container
            .behavior
            .hkb_context
            .hkb_character
            .behavior_graph
            .flat
            .iter()
            .map(|ptr| unsafe { ptr.as_ref() })
        {
            if node.flags[6] & 1 != 0 {
                continue;
            }

            let name = unsafe { *node.unk08.byte_add(0x48).cast::<*const c_char>() };
            if !name.is_null()
                && let Ok(name) = unsafe { CStr::from_ptr(name).to_str() }
                && let Ok(state) = name.try_into()
            {
                behavior_set.set_state(state);
            }
        }

        self.behavior_states.push_state_set(behavior_set);
    }

    fn soft_lock_on(&mut self, camera_pos: F32ViewMatrix) {
        self.lock_tgt.lock_camera = false;

        if !self.lock_tgt.is_lock_on_requested {
            return;
        }

        let origin = Vec4::from(camera_pos.3).truncate();
        let direction = Vec4::from(camera_pos.2).truncate() * 20.0;

        let hit = cast_sphere(origin, direction, 0.2, 0x2000058, |hit| {
            hit.field_ins().is_none_or(|owner| {
                if unsafe {
                    owner.as_ref().handle.selector.field_ins_type() == Some(FieldInsType::Chr)
                } {
                    self.player.can_target(owner.as_ptr() as *const ChrIns)
                } else {
                    true
                }
            })
        });

        if let Some(hit) = hit
            && let Some(field_ins_handle) = hit.field_ins_handle()
            && field_ins_handle.selector.field_ins_type() == Some(FieldInsType::Chr)
        {
            self.lock_on_to(field_ins_handle);
        }
    }

    fn show_tutorial(&self) {
        if self.config.show_tutorial
            && let Ok(event_flag) = unsafe { CSEventFlagMan::instance() }
            && !event_flag
                .virtual_memory_flag
                .get_flag(TUTORIAL_EVENT_FLAG_ID)
        {
            event_flag
                .virtual_memory_flag
                .set_flag(TUTORIAL_EVENT_FLAG_ID, true);

            static ONCE: Once = Once::new();
            ONCE.call_once(show_tutorial);
        }
    }
}

impl<'s, W: WorldState> Deref for CoreLogicContext<'s, W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

impl<'s, W: WorldState> DerefMut for CoreLogicContext<'s, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.world
    }
}

unsafe impl Send for CoreLogic {}

unsafe impl Sync for CoreLogic {}

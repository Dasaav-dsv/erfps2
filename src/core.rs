use std::{
    f32::consts::PI,
    ffi::{CStr, c_char},
    mem,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::{LazyLock, Once, RwLock},
};

use eldenring::{
    cs::{
        CSActionButtonMan, CSEventFlagMan, CSRemo, ChrCam, ChrCamType, ChrExFollowCam, ChrIns,
        EzDrawFillMode, FieldInsHandle, FieldInsType, GameDataMan, LockTgtMan, PlayerIns, RendMan,
    },
    position::{HavokPosition, PositionDelta},
};
use fromsoftware_shared::{F32Vector4, F32ViewMatrix, FromStatic, Triangle};
use glam::{EulerRot, Mat3A, Mat4, Quat, Vec3, Vec4};

use crate::{
    config::{Config, CrosshairKind, updater::ConfigUpdater},
    core::{
        behavior::{BehaviorStateSet, BehaviorStates},
        frame_cached::FrameCached,
        head_tracker::HeadTracker,
        time::{FrameTime, TransTime},
        world::{FromWorld, Void, World, WorldState},
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
mod frame_cached;
mod head_tracker;
mod stabilizer;
mod time;

pub struct CoreLogic {
    config: ConfigUpdater,
    state: RwLock<State>,
}

pub struct CoreLogicContext<'s, W> {
    pub config: &'s Config,
    world: NonNull<W>,
}

#[derive(Default)]
pub struct State {
    first_person: bool,
    should_transition: bool,
    frame_time: FrameCached<FrameTime>,
    trans_time: FrameCached<TransTime>,
    head_tracker: FrameCached<HeadTracker>,
    behavior_states: BehaviorStates,
    saved_angle_limit: Option<f32>,
}

impl CoreLogic {
    pub fn scope<W: WorldState, R>(
        f: impl for<'lt> FnOnce(&CoreLogicContext<'_, W::With<'lt>>) -> R,
    ) -> W::Result<R> {
        let scoped = CoreLogic::get();

        let config = scoped.config.get_or_update();
        let state = scoped.state.read().unwrap();

        W::in_world(&state, move |world| {
            f(&CoreLogicContext {
                config: &config,
                world: NonNull::from_ref(world),
            })
        })
    }

    pub fn scope_mut<W: WorldState, R>(
        f: impl for<'lt> FnOnce(&mut CoreLogicContext<'_, W::With<'lt>>) -> R,
    ) -> W::Result<R> {
        let scoped = CoreLogic::get();

        let config = scoped.config.get_or_update();
        let mut state = scoped.state.write().unwrap();

        W::in_world_mut(&mut state, move |world| {
            f(&mut CoreLogicContext {
                config: &config,
                world: NonNull::from_mut(world),
            })
        })
    }

    pub fn is_first_person() -> bool {
        CoreLogic::scope::<Void, _>(|context| context.first_person())
    }

    fn get() -> &'static CoreLogic {
        static S: LazyLock<CoreLogic> = LazyLock::new(CoreLogic::default);
        &S
    }
}

impl Default for CoreLogic {
    fn default() -> Self {
        let config = ConfigUpdater::new().unwrap();
        let state = State::from_config(&config.get_or_update());

        Self {
            config,
            state: RwLock::new(state),
        }
    }
}

impl State {
    fn from_config(config: &Config) -> Self {
        Self {
            should_transition: config.start_in_first_person,
            ..Default::default()
        }
    }
}

impl<'s, W: WorldState> CoreLogicContext<'s, W>
where
    for<'a> &'a ChrCam: FromWorld<&'a W>,
    for<'a> &'a CSRemo: FromWorld<&'a W>,
    for<'a> &'a LockTgtMan: FromWorld<&'a W>,
    for<'a> &'a PlayerIns: FromWorld<&'a W>,
{
    pub fn first_person(&self) -> bool {
        let in_cutscene = || {
            self.get::<CSRemo>()
                .and_then(|remo| remo.remo_man.as_ref())
                .is_some_and(|ptr| ptr.state != 1)
        };

        self.first_person && !in_cutscene() && !self.is_dist_view_cam()
    }

    pub fn next_frame(&mut self) {
        let frame_time = self.frame_time.measure();

        let stabilizer_window = self.config.stabilizer_window;
        self.head_tracker.set_stabilizer_window(stabilizer_window);

        self.trans_time.next_frame(frame_time);
        self.head_tracker.next_frame(frame_time);

        self.update_fov_correction();
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

        if let Some(player) = self.get::<PlayerIns>()
            && player.is_approaching_ladder()
        {
            follow_cam.reset_camera_y = true;
            follow_cam.reset_camera_x = true;
        }

        let frame_time = self.frame_time.get(());
        if let Some(lock_tgt) = self.get::<LockTgtMan>() {
            let lock_chase_rate = &mut follow_cam.lock_chase_rate;

            if lock_tgt.is_locked_on && *lock_chase_rate <= 1.0 {
                *lock_chase_rate = f32::min(*lock_chase_rate + frame_time, 1.0);
            } else if *lock_chase_rate > 0.3 {
                *lock_chase_rate = f32::max(*lock_chase_rate - frame_time, 0.3);
            }
        }
    }

    pub fn has_state(&self, state: BehaviorState) -> bool {
        self.behavior_states.has_state(state)
    }

    pub fn fov(&self) -> f32 {
        if self.is_aim_cam()
            && let Some(chr_cam) = self.get::<ChrCam>()
        {
            let aim_cam_fov = chr_cam.aim_cam.fov;

            if aim_cam_fov <= self.config.fov {
                return aim_cam_fov;
            }

            // f32::to_radians(25.0).atan()
            const AIM_CAM_HALF_WIDTH: f32 = 0.41143;
            let width_ratio = aim_cam_fov.atan() / AIM_CAM_HALF_WIDTH;

            f32::tan(self.config.fov.atan() * width_ratio)
        } else {
            self.config.fov
        }
    }

    fn update_fov_correction(&self) {
        enable_fov_correction(
            self.first_person && self.config.use_fov_correction,
            self.config.correction_strength,
            self.config.correction_cylindricity,
            self.config.use_barrel_correction,
            self.fov(),
        );
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

    fn is_aim_cam(&self) -> bool {
        self.get::<ChrCam>().is_some_and(|chr_cam| {
            matches!(
                chr_cam.camera_type,
                ChrCamType::Unk1 | ChrCamType::Unk2 | ChrCamType::Unk3
            )
        })
    }

    fn is_dist_view_cam(&self) -> bool {
        self.get::<ChrCam>()
            .is_some_and(|chr_cam| chr_cam.camera_type == ChrCamType::Unk5)
    }
}

impl<'s> CoreLogicContext<'_, World<'s>> {
    pub fn can_transition(&self) -> bool {
        self.trans_time.can_transition()
    }

    pub fn try_transition(&mut self) {
        let Ok(action_button_man) = (unsafe { CSActionButtonMan::instance() }) else {
            return;
        };

        if action_button_man.is_use_pressed {
            self.trans_time.get(());
        }

        let should_transition = self.should_transition;
        self.should_transition = false;

        if should_transition && (!self.config.prioritize_lock_on || !self.lock_tgt.is_locked_on) {
            self.transition();
        }

        if self.first_person() && self.can_show_tutorial() {
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
        let camera_rotation = Quat::from_mat3a(&self.chr_cam.pers_cam.matrix.rotation());

        let tracker_args = (&*self).into();
        let output = self.head_tracker.get(tracker_args);

        let head_rotation = output.head_matrix.rotation::<Mat3A>();
        let mut head_position = output.stabilized_head_position;

        let camera_rotation = camera_rotation * output.tracking_rotation;

        let cam_pitch = camera_rotation.to_euler(EulerRot::ZXY).1;
        let cam_pitch_exp = (cam_pitch.abs() / 3.0).powi(2);

        let (head_roll, head_pitch, _) = output
            .head_matrix
            .rotation::<Mat3A>()
            .to_euler(EulerRot::ZXY);

        let head_upright =
            ((1.05 - head_pitch.abs() / PI) * (1.05 - head_roll.abs() / PI)).clamp(0.0, 1.0);

        let world_contrib = Vec3::new(0.0, 0.1, 0.0);
        let head_contrib = Vec3::new(0.0, -0.1 * head_upright, -0.05);
        let cam_contrib =
            Vec3::new(0.0, 0.03 + cam_pitch_exp, -0.025 + cam_pitch.abs() / 12.0) * head_upright;

        head_position += world_contrib
            + head_rotation.transpose() * head_contrib
            + camera_rotation.inverse() * cam_contrib;

        Mat4::from_rotation_translation(camera_rotation, head_position).into()
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

        *self.player.aim_matrix_mut() = self.cs_cam.pers_cam_1.matrix;

        let fov = self.fov();

        self.cs_cam.pers_cam_1.fov = fov;
        self.chr_cam.pers_cam.fov = fov;
    }

    pub fn update_chr_cam(&mut self) {
        let first_person = self.first_person();

        self.set_crosshair_if(
            first_person
                && (!self.lock_tgt.is_locked_on || self.config.soft_lock_on)
                && !self.is_aim_cam(),
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

    pub fn update_chr_model_pos(&mut self) {
        let extra_player_height = self.config.extra_player_height;
        let player_height = extra_player_height * PlayerIns::HEIGHT;

        let location_entity_matrix = self.player.location_entity_matrix_mut();
        location_entity_matrix.3.1 += player_height;

        let chr_ctrl = self.player.chr_ctrl.as_mut();
        chr_ctrl.model_matrix.3.1 += player_height;

        let player_scale = 1.0 + extra_player_height.max(0.0);
        self.player.chr_ctrl.scale_size_y = player_scale;
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

    fn transition(&mut self) {
        self.first_person = !self.first_person;

        self.lock_tgt.is_lock_on_requested = false;

        self.update_fov_correction();

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

            self.player.chr_ctrl.scale_size_y = 1.0;
        } else {
            self.chr_cam.ex_follow_cam.max_lock_target_offset = 0.0;
        }
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

    fn can_show_tutorial(&self) -> bool {
        self.config.show_tutorial
            && self
                .player
                .module_container
                .action_request
                .movement_request_duration
                > 0.1
    }

    fn show_tutorial(&self) {
        if let Ok(event_flag) = unsafe { CSEventFlagMan::instance() }
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
        unsafe { self.world.as_ref() }
    }
}

impl<'s, W: WorldState> DerefMut for CoreLogicContext<'s, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.world.as_mut() }
    }
}

unsafe impl Send for CoreLogic {}

unsafe impl Sync for CoreLogic {}

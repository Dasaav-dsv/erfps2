use std::{
    collections::VecDeque,
    f32::consts::PI,
    mem,
    ops::{Deref, DerefMut},
    sync::{LazyLock, RwLock},
};

use eldenring::cs::{
    CSActionButtonMan, CSCamera, CSRemo, ChrCam, ChrExFollowCam, PlayerIns, WorldChrMan,
};
use fromsoftware_shared::{F32ViewMatrix, FromStatic};
use glam::{EulerRot, Mat3A, Mat4, Quat, Vec3, Vec4};

use crate::{
    config::{Config, CrosshairKind, FovCorrection, updater::ConfigUpdater},
    player::PlayerExt,
    program::Program,
    rva::CAM_WALL_RECOVERY_RVA,
    shaders::{enable_dithering, enable_fov_correction, enable_vfx_fade, set_crosshair},
};

pub struct CameraControl {
    state: CameraState,
    context: Option<CameraContext>,
    updater: ConfigUpdater,
}

pub struct CameraState {
    first_person: bool,

    should_transition: bool,

    pub fov: f32,

    pub tpf: f32,

    pub trans_time: f32,

    pub angle_limit: [f32; 2],

    pub soft_lock_on: bool,

    pub prioritize_lock_on: bool,

    pub unlocked_movement: bool,

    pub unobtrusive_dodges: bool,

    pub track_dodges: bool,

    pub restricted_sprint: bool,

    pub use_stabilizer: bool,

    pub stabilizer_window: f32,

    pub stabilizer_factor: f32,

    pub crosshair: CrosshairKind,

    pub crosshair_scale: (f32, f32),

    pub use_fov_correction: bool,

    pub use_barrel_correction: bool,

    pub correction_strength: f32,

    pub saved_angle_limit: Option<f32>,
}

pub struct CameraContext {
    pub cs_cam: &'static mut CSCamera,
    pub chr_cam: &'static mut ChrCam,
    pub lock_tgt: &'static mut LockTgtMan,
    pub player: &'static mut PlayerIns,
    persistent_context: PersistentCameraContext,
}

pub struct PersistentCameraContext {
    frame: u64,
    stabilizer: CameraStabilizer,
    head_tracker: HeadTracker,
    behavior_states: BehaviorStates,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorState {
    Attack,
    Evasion,
    Gesture,
}

#[repr(C)]
#[fromsoftware_shared::singleton("LockTgtMan")]
pub struct LockTgtMan {
    unk00: [u8; 0x2830],
    pub is_locked_on: bool,
    pub is_lock_on_requested: bool,
    unk2832: [u8; 0x15b],
    pub lock_camera: bool,
}

struct CameraStabilizer {
    frame: u64,
    samples: u32,
    buf: VecDeque<Vec3>,
}

struct HeadTracker {
    frame: u64,
    last: Quat,
    rotation: Quat,
}

struct BehaviorStates {
    state_names: Vec<BehaviorState>,
    erase_index: usize,
}

impl CameraControl {
    pub fn scope<F: FnOnce(&CameraControl) -> R, R>(f: F) -> R {
        f(&Self::get().read().unwrap())
    }

    pub fn scope_mut<F: FnOnce(&mut CameraControl) -> R, R>(f: F) -> R {
        f(&mut Self::get().write().unwrap())
    }

    fn new() -> Self {
        let mut updater = ConfigUpdater::new().unwrap();

        let state = updater.get_or_update().map_or_else(
            |error| {
                log::error!(
                    "failed to update config: {error}. Is it placed in the same directory as erfps2.dll?"
                );
                CameraState::default()
            },
            CameraState::from,
        );

        Self {
            state,
            context: None,
            updater,
        }
    }

    fn get() -> &'static RwLock<CameraControl> {
        static S: LazyLock<RwLock<CameraControl>> =
            LazyLock::new(|| RwLock::new(CameraControl::new()));
        &S
    }

    pub fn state_and_context(&mut self) -> (&mut CameraState, Option<&mut CameraContext>) {
        let Self {
            state,
            context,
            updater,
        } = self;

        *context = state.try_update(context.take(), updater);

        (state, context.as_mut())
    }

    pub fn next_frame(&mut self) {
        if let Some(context) = &mut self.context {
            context.frame += 1;
            context.behavior_states.next_frame();
        }
    }
}

impl CameraState {
    pub fn first_person(&self) -> bool {
        let remo = unsafe { CSRemo::instance().ok() };
        let in_cutscene = remo
            .and_then(|remo| remo.remo_man.as_ref())
            .is_some_and(|ptr| ptr.state != 1);

        self.first_person && !in_cutscene
    }

    pub fn update_follow_cam(&mut self, follow_cam: &mut ChrExFollowCam) {
        unsafe {
            let first_person = self.first_person();

            *Program::current().derva::<bool>(CAM_WALL_RECOVERY_RVA) &= !first_person;
            follow_cam.camera_auto_rotation &= !first_person;

            if first_person {
                let angle_limit = mem::replace(&mut follow_cam.angle_limit, self.angle_limit);

                if angle_limit[1] != self.angle_limit[1] {
                    self.saved_angle_limit = Some(angle_limit[1]);
                }

                if let Some(player) = PlayerIns::main_player()
                    && player.is_approaching_ladder()
                {
                    follow_cam.reset_camera_y = true;
                    follow_cam.reset_camera_x = true;
                }
            } else if let Some(saved_angle_limit) = self.saved_angle_limit.take() {
                follow_cam.angle_limit[1] = saved_angle_limit;
            }
        }
    }

    pub fn can_transition(&self) -> bool {
        const STATE_TRANS_TIME: f32 = 0.233;
        self.trans_time > STATE_TRANS_TIME
    }

    fn try_update(
        &mut self,
        context: Option<CameraContext>,
        updater: &mut ConfigUpdater,
    ) -> Option<CameraContext> {
        if let Ok(config) = updater.get_or_update() {
            *self = Self {
                first_person: self.first_person,
                should_transition: self.should_transition,
                tpf: self.tpf,
                trans_time: self.trans_time,
                ..config.into()
            };

            self.update_fov_correction(self.fov);
        }

        let world_chr_man = unsafe { WorldChrMan::instance().ok()? };

        let cs_cam = unsafe { CSCamera::instance().ok()? };
        let chr_cam = unsafe { world_chr_man.chr_cam?.as_mut() };
        let lock_tgt = unsafe { LockTgtMan::instance().ok()? };
        let player = world_chr_man.main_player.as_deref_mut()?;

        let persistent_context = context
            .map(|context| context.persistent_context)
            .unwrap_or_else(|| {
                let samples = self.stabilizer_window / self.tpf;
                PersistentCameraContext::new(samples.ceil() as u32)
            });

        Some(CameraContext {
            cs_cam,
            chr_cam,
            lock_tgt,
            player,
            persistent_context,
        })
    }

    fn update_fov_correction(&self, fov: f32) {
        enable_fov_correction(
            self.first_person && self.use_fov_correction,
            self.correction_strength,
            self.use_barrel_correction,
            fov,
        );
    }

    fn set_crosshair_if(&self, cond: bool) {
        let crosshair = if cond {
            self.crosshair
        } else {
            CrosshairKind::None
        };

        set_crosshair(crosshair, self.crosshair_scale);
    }
}

impl CameraContext {
    pub fn try_transition(&mut self, state: &mut CameraState) {
        let Ok(action_button_man) = (unsafe { CSActionButtonMan::instance() }) else {
            return;
        };

        match action_button_man.is_use_pressed {
            true => state.trans_time += state.tpf,
            false => state.trans_time = 0.0,
        }

        let should_transition = state.should_transition;
        state.should_transition = false;

        if should_transition && (!state.prioritize_lock_on || !self.lock_tgt.is_locked_on) {
            state.first_person = !state.first_person;

            self.lock_tgt.is_lock_on_requested = false;

            state.update_fov_correction(self.fov(state));

            let first_person = state.first_person();

            enable_dithering(!first_person);
            enable_vfx_fade(!first_person);

            state.set_crosshair_if(first_person);

            self.player.enable_face_model(!first_person);
            self.player.enable_sheathed_weapons(!first_person);

            if !first_person {
                self.player.make_transparent(false);
                self.lock_tgt.lock_camera = true;
            }
        }

        let is_lock_on_toggled = self.lock_tgt.is_locked_on != self.lock_tgt.is_lock_on_requested;
        let should_not_lock_on = (!state.prioritize_lock_on && is_lock_on_toggled)
            || (!self.lock_tgt.is_locked_on && is_lock_on_toggled);

        if state.can_transition() && should_not_lock_on {
            state.should_transition = true;

            if !state.prioritize_lock_on {
                self.lock_tgt.is_lock_on_requested = false;
            }
        }
    }

    pub fn camera_position(&mut self, state: &CameraState) -> F32ViewMatrix {
        let frame = self.frame;

        let head_mtx = self.player.head_position();

        let head_rotation = head_mtx.rotation();
        let camera_rotation = Quat::from_mat3a(&self.chr_cam.pers_cam.matrix.rotation());

        let mut head_pos = head_mtx.translation();

        if state.use_stabilizer {
            let player_mtx = Mat4::from(self.player.chr_ctrl.model_matrix);
            let local_head_pos = player_mtx.inverse().transform_point3(head_pos);

            let stabilized = self.stabilizer.next(frame, local_head_pos);
            let delta = stabilized - local_head_pos;

            head_pos = player_mtx.transform_point3(
                local_head_pos + delta.clamp_length_max(state.stabilizer_factor * 0.1),
            );
        }

        let tracking_rotation = if self.player.is_in_throw()
            || (state.track_dodges && self.has_state(BehaviorState::Evasion))
        {
            self.head_tracker.next_tracked(frame, head_rotation)
        } else {
            self.head_tracker.next_untracked(frame, head_rotation)
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

    pub fn update_cs_cam(&mut self, state: &mut CameraState) {
        if !state.first_person() {
            return;
        }

        let camera_pos = self.camera_position(state);

        if !self.lock_tgt.is_locked_on {
            let lock_on_pos =
                Vec4::from(camera_pos.3) + Vec4::from(self.chr_cam.pers_cam.matrix.2) * 10.0;
            self.player.set_lock_on_target_position(lock_on_pos);
        }

        self.cs_cam.pers_cam_1.matrix = self.chr_cam.pers_cam.matrix;

        self.cs_cam.pers_cam_1.matrix.3 = camera_pos.3;
        self.chr_cam.pers_cam.matrix.3 = camera_pos.3;

        *self.player.aim_mtx_mut() = self.cs_cam.pers_cam_1.matrix;

        let fov = self.fov(state);

        self.cs_cam.pers_cam_1.fov = fov;
        self.chr_cam.pers_cam.fov = fov;
    }

    pub fn update_chr_cam(&mut self, state: &CameraState) {
        let first_person = state.first_person();

        state.set_crosshair_if(
            first_person
                && !self.lock_tgt.is_locked_on
                && !self.player.chr_flags1c5.precision_shooting(),
        );

        if !first_person {
            return;
        }

        self.player.enable_face_model(false);
        self.player.enable_sheathed_weapons(false);

        if state.soft_lock_on {
            self.lock_tgt.lock_camera = false;
        }

        if state.unobtrusive_dodges {
            self.player
                .make_transparent(self.has_state(BehaviorState::Evasion));
        }

        let camera_pos = self.camera_position(state);

        if state.restricted_sprint {
            self.restrict_sprint(camera_pos.rotation());
        }

        self.cs_cam.pers_cam_1.matrix = camera_pos;
        self.chr_cam.pers_cam.matrix = camera_pos;

        let fov = self.fov(state);

        self.cs_cam.pers_cam_1.fov = fov;
        self.chr_cam.pers_cam.fov = fov;
    }

    pub fn fov(&self, state: &CameraState) -> f32 {
        if !self.player.chr_flags1c5.precision_shooting() {
            return state.fov;
        }

        let aim_cam_fov = self.chr_cam.aim_cam.fov;
        if aim_cam_fov <= state.fov {
            return aim_cam_fov;
        }

        // f32::to_radians(25.0).atan()
        const AIM_CAM_HALF_WIDTH: f32 = 0.41143;
        let width_ratio = aim_cam_fov.atan() / AIM_CAM_HALF_WIDTH;

        f32::tan(state.fov.atan() * width_ratio)
    }

    pub fn is_player_sprinting(&self, state: &CameraState) -> bool {
        if state.restricted_sprint {
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

    pub fn push_state(&mut self, state: BehaviorState) {
        self.behavior_states.push_state(state);
    }

    pub fn has_state(&self, state: BehaviorState) -> bool {
        self.behavior_states.has_state(state)
    }
}

impl PersistentCameraContext {
    fn new(samples: u32) -> Self {
        Self {
            frame: 0,
            stabilizer: CameraStabilizer::new(samples),
            head_tracker: HeadTracker::new(),
            behavior_states: BehaviorStates::new(),
        }
    }
}

impl CameraStabilizer {
    const fn new(samples: u32) -> Self {
        Self {
            frame: 0,
            samples,
            buf: VecDeque::new(),
        }
    }

    fn next(&mut self, frame: u64, new: Vec3) -> Vec3 {
        let prev_frame = mem::replace(&mut self.frame, frame);

        if prev_frame != frame {
            if prev_frame + 1 != frame {
                self.buf.clear();
            }

            self.buf.push_front(new);
            self.buf.truncate(self.samples as usize);
        }

        self.average(new)
    }

    fn average(&self, default: Vec3) -> Vec3 {
        if !self.buf.is_empty() {
            self.buf.iter().sum::<Vec3>() / self.buf.len() as f32
        } else {
            default
        }
    }
}

impl HeadTracker {
    const fn new() -> Self {
        Self {
            frame: 0,
            last: Quat::IDENTITY,
            rotation: Quat::IDENTITY,
        }
    }

    fn next_tracked(&mut self, frame: u64, new: Mat3A) -> Quat {
        let prev_frame = mem::replace(&mut self.frame, frame);

        if prev_frame != frame {
            let new = Quat::from_mat3a(&new);

            if prev_frame + 1 != frame {
                self.last = new;
            }

            self.rotation *= self.last.inverse() * new;
            self.rotation = self.rotation.normalize();

            self.last = new;
        }

        self.rotation
    }

    fn next_untracked(&mut self, frame: u64, new: Mat3A) -> Quat {
        let prev_frame = mem::replace(&mut self.frame, frame);

        if prev_frame == frame {
            return self.rotation;
        }

        self.rotation = self.rotation.slerp(Quat::IDENTITY, 0.35).normalize();
        self.last = Quat::from_mat3a(&new);

        self.rotation
    }
}

impl BehaviorStates {
    const fn new() -> Self {
        Self {
            state_names: vec![],
            erase_index: 0,
        }
    }

    fn has_state(&self, state: BehaviorState) -> bool {
        self.state_names.contains(&state)
    }

    fn push_state(&mut self, state: BehaviorState) {
        self.state_names.push(state);
    }

    fn next_frame(&mut self) {
        self.state_names.drain(..self.erase_index);
        self.erase_index = self.state_names.len();
    }
}

impl BehaviorState {
    pub fn into_state_name(self) -> &'static str {
        match self {
            Self::Attack => "Attack_SM",
            Self::Evasion => "Evasion_SM",
            Self::Gesture => "Gesture_SM",
        }
    }

    pub fn try_from_state_name(name: &str) -> Option<Self> {
        match name {
            "Attack_SM" => Some(Self::Attack),
            "Evasion_SM" => Some(Self::Evasion),
            "Gesture_SM" => Some(Self::Gesture),
            _ => None,
        }
    }
}

impl Deref for CameraControl {
    type Target = CameraState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for CameraControl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            first_person: false,
            should_transition: true,
            fov: const { f32::to_radians(85.0) },
            tpf: const { 1.0 / 60.0 },
            trans_time: 0.0,
            angle_limit: const { [f32::to_radians(-80.0), f32::to_radians(70.0)] },
            soft_lock_on: false,
            prioritize_lock_on: true,
            unlocked_movement: true,
            unobtrusive_dodges: false,
            track_dodges: false,
            restricted_sprint: false,
            use_stabilizer: true,
            stabilizer_window: 0.3,
            stabilizer_factor: 0.8,
            crosshair: CrosshairKind::Cross,
            crosshair_scale: (1.0, 1.0),
            use_fov_correction: true,
            use_barrel_correction: false,
            correction_strength: 0.5,
            saved_angle_limit: None,
        }
    }
}

impl From<&Config> for CameraState {
    fn from(config: &Config) -> Self {
        let mut state = Self::default();

        let degrees = config.fov.horizontal_fov.clamp(45.0, 130.0);
        state.fov = degrees.to_radians();

        match config.fov.fov_correction {
            FovCorrection::None => state.use_fov_correction = false,
            FovCorrection::Fisheye => state.use_barrel_correction = false,
            FovCorrection::Barrel => state.use_barrel_correction = true,
        }

        state.correction_strength = config.fov.fov_correction_strength;

        state.should_transition = config.gameplay.start_in_first_person;
        state.soft_lock_on = config.gameplay.soft_lock_on;
        state.prioritize_lock_on = config.gameplay.prioritize_lock_on;
        state.unlocked_movement = config.gameplay.unlocked_movement;
        state.unobtrusive_dodges = config.gameplay.unobtrusive_dodges;
        state.track_dodges = config.gameplay.track_dodges;
        state.restricted_sprint = config.gameplay.restricted_sprint;

        state.use_stabilizer = config.stabilizer.enabled;
        state.stabilizer_window = config.stabilizer.smoothing_window.clamp(0.1, 1.0);
        state.stabilizer_factor = config.stabilizer.smoothing_factor.clamp(0.0, 1.0);

        state.crosshair = config.crosshair.kind;
        state.crosshair_scale.0 = config.crosshair.scale_x.clamp(0.1, 4.0);
        state.crosshair_scale.1 = config.crosshair.scale_y.clamp(0.1, 4.0);

        state
    }
}

impl Deref for CameraContext {
    type Target = PersistentCameraContext;

    fn deref(&self) -> &Self::Target {
        &self.persistent_context
    }
}

impl DerefMut for CameraContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.persistent_context
    }
}

unsafe impl Send for CameraContext {}

unsafe impl Sync for CameraContext {}

impl From<BehaviorState> for &'static str {
    fn from(value: BehaviorState) -> Self {
        value.into_state_name()
    }
}

impl TryFrom<&'_ str> for BehaviorState {
    type Error = ();

    fn try_from(name: &'_ str) -> Result<Self, Self::Error> {
        Self::try_from_state_name(name).ok_or(())
    }
}

use std::{
    cell::LazyCell,
    collections::VecDeque,
    mem,
    ops::{Deref, DerefMut},
    sync::{Mutex, MutexGuard},
};

use eldenring::cs::{
    CSActionButtonMan, CSCamera, CSRemo, ChrCam, ChrExFollowCam, PlayerIns, WorldChrMan,
};
use fromsoftware_shared::{F32ViewMatrix, FromStatic};
use glam::{Mat4, Vec3, Vec4};

use crate::{
    config::{Config, FovCorrection, updater::ConfigUpdater},
    player::PlayerExt,
    program::Program,
    rva::CAM_WALL_RECOVERY_RVA,
    shaders::{enable_crosshair, enable_dithering, enable_fov_correction},
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

    pub angle_limit: f32,

    pub use_stabilizer: bool,

    pub stabilizer_window: f32,

    pub stabilizer_factor: f32,

    pub use_crosshair: bool,

    pub use_fov_correction: bool,

    pub use_barrel_correction: bool,

    pub correction_strength: f32,

    pub saved_angle_limit: Option<f32>,

    pub in_head_offset_y: f32,

    pub in_head_offset_z: f32,
}

pub struct CameraContext {
    pub cs_cam: &'static mut CSCamera,
    pub chr_cam: &'static mut ChrCam,
    pub lock_tgt: &'static mut LockTgtMan,
    pub player: &'static mut PlayerIns,
    frame: u64,
    stabilizer: CameraStabilizer,
}

#[repr(C)]
#[fromsoftware_shared::singleton("LockTgtMan")]
pub struct LockTgtMan {
    unk00: [u8; 0x2830],
    pub is_locked_on: bool,
    pub is_lock_on_requested: bool,
}

struct CameraStabilizer {
    frame: u64,
    samples: u32,
    buf: VecDeque<Vec3>,
}

impl CameraControl {
    pub fn lock() -> MutexGuard<'static, LazyCell<Self>> {
        static STATE: Mutex<LazyCell<CameraControl>> =
            Mutex::new(LazyCell::new(CameraControl::new));
        STATE.lock().unwrap()
    }

    pub fn new() -> Self {
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

                if angle_limit != self.angle_limit {
                    self.saved_angle_limit = Some(angle_limit);
                }

                if let Some(player) = PlayerIns::main_player()
                    && player.is_on_ladder()
                {
                    follow_cam.reset_camera_y = true;
                    follow_cam.reset_camera_x = true;
                }
            } else if let Some(saved_angle_limit) = self.saved_angle_limit.take() {
                follow_cam.angle_limit = saved_angle_limit;
            }
        }
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

            self.update_fov_correction();
        }

        let world_chr_man = unsafe { WorldChrMan::instance().ok()? };

        let cs_cam = unsafe { CSCamera::instance().ok()? };
        let chr_cam = unsafe { world_chr_man.chr_cam?.as_mut() };
        let lock_tgt = unsafe { LockTgtMan::instance().ok()? };
        let player = world_chr_man.main_player.as_deref_mut()?;

        let (frame, stabilizer) = context
            .map(|context| (context.frame, context.stabilizer))
            .unwrap_or_else(|| {
                let samples = self.stabilizer_window / self.tpf;
                (0, CameraStabilizer::new(samples.ceil() as u32))
            });

        Some(CameraContext {
            cs_cam,
            chr_cam,
            lock_tgt,
            player,
            frame,
            stabilizer,
        })
    }

    fn update_fov_correction(&self) {
        enable_fov_correction(
            self.first_person && self.use_fov_correction,
            self.correction_strength,
            self.use_barrel_correction,
            self.fov,
        );
    }
}

impl CameraContext {
    pub fn try_transition(&mut self, state: &mut CameraState) {
        const STATE_TRANS_TIME: f32 = 0.233;

        let Ok(action_button_man) = (unsafe { CSActionButtonMan::instance() }) else {
            return;
        };

        match action_button_man.is_use_pressed {
            true => state.trans_time += state.tpf,
            false => state.trans_time = 0.0,
        }

        if (state.trans_time > STATE_TRANS_TIME
            && self.lock_tgt.is_locked_on != self.lock_tgt.is_lock_on_requested)
            || state.should_transition
        {
            state.first_person = !state.first_person;
            state.should_transition = false;

            self.lock_tgt.is_lock_on_requested = false;

            state.update_fov_correction();
            enable_dithering(!state.first_person);

            self.player.enable_face_model(!state.first_person);
        }
    }

    pub fn camera_position(&mut self, state: &CameraState) -> F32ViewMatrix {
        let mut head_pos = self.player.head_position();
        let mut new_head_pos = Vec4::from(head_pos.3).truncate();

        if state.use_stabilizer {
            let player_pos = Mat4::from(self.player.chr_ctrl.model_matrix);
            let abs_head_pos = player_pos
                .inverse()
                .transform_point3(head_pos.translation());

            let stabilized = self.stabilizer.next(self.frame, abs_head_pos);
            let delta = stabilized - abs_head_pos;

            new_head_pos = player_pos.transform_point3(
                abs_head_pos + delta.clamp_length_max(state.stabilizer_factor * 0.1),
            );
        }

        let y_offset = Vec3::new(0.0, 1.0, 0.0);
        let z_offset = Vec4::from(head_pos.2)
            .with_y(0.0)
            .truncate()
            .normalize_or_zero();

        new_head_pos += y_offset * state.in_head_offset_y + z_offset * state.in_head_offset_z;
        head_pos.3 = new_head_pos.extend(1.0).into();

        head_pos
    }

    pub fn update_cs_cam(&mut self, state: &mut CameraState) {
        let camera_pos = self.camera_position(state);

        if !self.lock_tgt.is_locked_on && !self.player.is_on_ladder() && !self.player.is_in_throw()
        {
            let lock_on_pos =
                Vec4::from(camera_pos.3) + Vec4::from(self.chr_cam.pers_cam.matrix.2) * 10.0;
            self.player.set_lock_on_target_position(lock_on_pos);
        }

        self.cs_cam.pers_cam_1.matrix = self.chr_cam.pers_cam.matrix;

        self.cs_cam.pers_cam_1.matrix.3 = camera_pos.3;
        self.chr_cam.pers_cam.matrix.3 = camera_pos.3;

        *self.player.aim_mtx_mut() = self.cs_cam.pers_cam_1.matrix;

        let mut fov = state.fov;
        if self.player.chr_flags1c5.precision_shooting() {
            fov *= 0.6;
        }

        self.cs_cam.pers_cam_1.fov = fov;
        self.chr_cam.pers_cam.fov = fov;
    }

    pub fn update_chr_cam(&mut self, state: &CameraState) {
        enable_crosshair(!self.lock_tgt.is_locked_on && state.use_crosshair);

        self.player.enable_face_model(false);

        let camera_pos = self.camera_position(state);

        if self.player.is_on_ladder() || self.player.is_in_throw() {
            self.cs_cam.pers_cam_1.matrix = camera_pos;
            self.chr_cam.pers_cam.matrix = camera_pos;
        }

        self.chr_cam.pers_cam.matrix.3 = camera_pos.3;

        let mut fov = state.fov;
        if self.player.chr_flags1c5.precision_shooting() {
            fov *= 0.6;
        }

        self.cs_cam.pers_cam_1.fov = fov;
        self.chr_cam.pers_cam.fov = fov;
    }
}

impl CameraStabilizer {
    fn new(samples: u32) -> Self {
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
            angle_limit: const { f32::to_radians(50.0) },
            stabilizer_window: 0.3,
            stabilizer_factor: 0.8,
            use_stabilizer: true,
            use_crosshair: true,
            use_fov_correction: true,
            use_barrel_correction: false,
            correction_strength: 0.5,
            saved_angle_limit: None,
            in_head_offset_y: 0.015,
            in_head_offset_z: -0.02,
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

        state.use_stabilizer = config.stabilizer.enabled;
        state.stabilizer_window = config.stabilizer.smoothing_window.clamp(0.1, 1.0);
        state.stabilizer_factor = config.stabilizer.smoothing_factor.clamp(0.0, 1.0);

        state.use_crosshair = config.crosshair.enabled;

        state
    }
}

unsafe impl Send for CameraContext {}

unsafe impl Sync for CameraContext {}

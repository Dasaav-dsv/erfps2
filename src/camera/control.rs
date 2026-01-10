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
    player::PlayerExt,
    program::Program,
    rva::CAM_WALL_RECOVERY_RVA,
    shaders::{enable_crosshair, enable_dithering, enable_fisheye_distortion},
};

#[derive(Default)]
pub struct CameraControl {
    state: CameraState,
    context: Option<CameraContext>,
}

pub struct CameraState {
    first_person: bool,
    pub fov: f32,
    pub tpf: f32,
    pub trans_time: f32,
    pub angle_limit: f32,
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

#[derive(Default)]
struct CameraStabilizer {
    frame: u64,
    buf: VecDeque<Vec3>,
}

impl CameraControl {
    pub fn lock() -> MutexGuard<'static, LazyCell<Self>> {
        static STATE: Mutex<LazyCell<CameraControl>> = Mutex::new(LazyCell::new(Default::default));
        STATE.lock().unwrap()
    }

    pub fn state_and_context(&mut self) -> (&mut CameraState, Option<&mut CameraContext>) {
        let Self { state, context } = self;
        *context = CameraContext::try_update(context.take());
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

        if state.trans_time > STATE_TRANS_TIME
            && self.lock_tgt.is_locked_on != self.lock_tgt.is_lock_on_requested
        {
            state.first_person = !state.first_person;

            self.lock_tgt.is_lock_on_requested = false;

            enable_fisheye_distortion(state.first_person);
            enable_dithering(!state.first_person);

            self.player.enable_face_model(!state.first_person);
        }
    }

    pub fn camera_position(&mut self, state: &CameraState) -> F32ViewMatrix {
        let mut head_pos = self.player.head_position();

        let player_pos = Mat4::from(self.player.chr_ctrl.model_matrix);
        let abs_head_pos = player_pos
            .inverse()
            .transform_point3(head_pos.translation());

        let stabilized = self.stabilizer.next(self.frame, abs_head_pos);
        let delta = stabilized - abs_head_pos;

        let mut new_head_pos =
            player_pos.transform_point3(abs_head_pos + delta.clamp_length_max(0.08));

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
        enable_crosshair(!self.lock_tgt.is_locked_on);

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

    fn try_update(context: Option<Self>) -> Option<Self> {
        let world_chr_man = unsafe { WorldChrMan::instance().ok()? };

        let cs_cam = unsafe { CSCamera::instance().ok()? };
        let chr_cam = unsafe { world_chr_man.chr_cam?.as_mut() };
        let lock_tgt = unsafe { LockTgtMan::instance().ok()? };
        let player = world_chr_man.main_player.as_deref_mut()?;

        let (frame, stabilizer) = context
            .map(|context| (context.frame, context.stabilizer))
            .unwrap_or_default();

        Some(Self {
            cs_cam,
            chr_cam,
            lock_tgt,
            player,
            frame,
            stabilizer,
        })
    }
}

impl CameraStabilizer {
    const FRAMES: usize = 20;

    fn next(&mut self, frame: u64, new: Vec3) -> Vec3 {
        let prev_frame = mem::replace(&mut self.frame, frame);

        if prev_frame != frame {
            if prev_frame + 1 != frame {
                self.buf.clear();
            }

            self.buf.push_front(new);

            if self.buf.len() > Self::FRAMES {
                self.buf.pop_back();
            }
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
            fov: const { f32::to_radians(85.0) },
            tpf: const { 1.0 / 60.0 },
            trans_time: 0.0,
            angle_limit: const { f32::to_radians(50.0) },
            saved_angle_limit: None,
            in_head_offset_y: 0.020,
            in_head_offset_z: -0.045,
        }
    }
}

unsafe impl Send for CameraContext {}

unsafe impl Sync for CameraContext {}

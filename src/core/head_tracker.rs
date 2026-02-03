use std::f32::consts::PI;

use fromsoftware_shared::F32ModelMatrix;
use glam::{Mat4, Quat, Vec3};

use crate::{
    core::{
        BehaviorState, CoreLogicContext, frame_cached::FrameCache, stabilizer::CameraStabilizer,
        world::World,
    },
    player::PlayerExt,
};

#[derive(Default)]
pub struct HeadTracker {
    last: Option<Quat>,
    rotation: Quat,
    rotation_target: Quat,
    last_tracking_kind: TrackingKind,
    stabilizer: CameraStabilizer,
    output: Option<Output>,
}

#[derive(Copy, Clone, Default, PartialEq)]
pub enum TrackingKind {
    Global,
    PlayerRelative,
    PlayerRelativeFast,

    #[default]
    None,
}

pub struct Args {
    pub model_matrix: F32ModelMatrix,
    pub head_matrix: F32ModelMatrix,
    pub player_matrix: F32ModelMatrix,
    pub stabilizer_factor: f32,
    pub use_stabilizer: bool,
    pub tracking_kind: TrackingKind,
}

pub struct Output {
    pub tracking_rotation: Quat,
    pub stabilized_head_position: Vec3,
    pub head_matrix: F32ModelMatrix,
}

impl HeadTracker {
    pub fn set_stabilizer_window(&mut self, window: f32) {
        self.stabilizer.set_window(window);
    }

    fn rotate_towards_target(&mut self, speed: f32, frame_time: f32) {
        let angle = self.rotation_target.angle_between(Quat::IDENTITY);

        let center_bias_until = PI / 2.0 * 3.0;

        let biased_target = if angle < center_bias_until {
            self.rotation_target.rotate_towards(
                Quat::IDENTITY,
                angle * (1.0 - (angle / center_bias_until)).powi(2),
            )
        } else {
            self.rotation_target
        };

        let distance = self.rotation.angle_between(biased_target);
        let step = rip(distance, 0.0, speed, frame_time);

        self.rotation = self.rotation.rotate_towards(biased_target, step);
    }
}

impl FrameCache for HeadTracker {
    type Input = Args;
    type Output<'a> = &'a Output;

    fn update(&mut self, frame_time: f32, args: Self::Input) -> Self::Output<'_> {
        let mut head_position = args.head_matrix.translation();

        if args.use_stabilizer {
            let player_matrix = Mat4::from(args.model_matrix);

            let mut local_head_pos = player_matrix.inverse().project_point3(head_position);

            let stabilized = self.stabilizer.update(frame_time, local_head_pos);
            let delta = stabilized - local_head_pos;

            local_head_pos += delta.clamp_length_max(args.stabilizer_factor * 0.1);

            head_position = player_matrix.project_point3(local_head_pos);
        }

        let mut input = Quat::from_mat3a(&args.head_matrix.rotation());

        if args.tracking_kind != self.last_tracking_kind {
            self.last = None;
            self.last_tracking_kind = args.tracking_kind;
        }

        if matches!(
            args.tracking_kind,
            TrackingKind::PlayerRelative | TrackingKind::PlayerRelativeFast
        ) {
            let player_rotation = Quat::from_mat3a(&args.player_matrix.rotation());
            input = player_rotation.inverse() * input;
        }

        if args.tracking_kind != TrackingKind::None
            && let Some(last) = self.last
        {
            self.rotation_target *= last.inverse() * input;
            self.rotation_target = self.rotation_target.normalize();
        } else {
            self.rotation_target = self
                .rotation_target
                .slerp(Quat::IDENTITY, frame_time * 10.0);
        }

        self.last = Some(input);
        self.rotate_towards_target(
            frame_time,
            if args.tracking_kind == TrackingKind::PlayerRelativeFast {
                1.4
            } else {
                1.0
            },
        );

        self.output.insert(Output {
            tracking_rotation: self.rotation,
            stabilized_head_position: head_position,
            head_matrix: args.head_matrix,
        })
    }

    fn get_cached(&mut self, _frame_time: f32, _input: Self::Input) -> Self::Output<'_> {
        self.output.as_ref().expect("FrameCache logic error")
    }

    fn reset(&mut self) {
        self.stabilizer.reset();
        self.last = None;
    }
}

impl From<&CoreLogicContext<'_, World<'_>>> for Args {
    fn from(context: &CoreLogicContext<'_, World<'_>>) -> Self {
        let head_matrix = context.player.head_matrix();
        let model_matrix = context.player.model_matrix();

        let tracking_kind = if context.player.is_in_throw()
            || (context.config.track_damage && context.has_state(BehaviorState::Damage))
            || (context.config.track_damage
                && context.has_state(BehaviorState::DeathAnim)
                && !context.has_state(BehaviorState::DeathIdle))
        {
            TrackingKind::Global
        } else if context.config.track_attacks && context.has_state(BehaviorState::Attack) {
            TrackingKind::PlayerRelative
        } else if context.config.track_dodges && context.has_state(BehaviorState::Evasion) {
            TrackingKind::PlayerRelativeFast
        } else {
            TrackingKind::None
        };

        Self {
            head_matrix,
            model_matrix,
            player_matrix: context.player.chr_ctrl.model_matrix,
            stabilizer_factor: context.config.stabilizer_factor,
            use_stabilizer: context.config.use_stabilizer,
            tracking_kind,
        }
    }
}

/**
    Computes a signed distance step that moves `distance` toward 0 over the next `timedelta`.

      Curve: d(t) = (t * b)^6 - a
    Inverse: t(d) = (d + a)^(1/6) / b
       Step:        d(t) - d(t-Δt)

    Method:
    - Interpret `distance` as the remaining distance to zero, offset by `curve_offset`.
    - Convert remaining distance -> remaining time using t(d), scaled by `curve_scale`.
    - Advance time by `timedelta` and map back using d(t) to get the new remaining distance.
    - Return step = distance - distance_new, clamped to \[0, distance\].
*/
fn rip(distance: f32, curve_offset: f32, curve_scale: f32, timedelta: f32) -> f32 {
    let sign = distance.signum();
    let distance = distance.abs();

    let time_remaining = (distance + curve_offset).powf(1.0 / 6.0) / curve_scale;
    let time_new = (time_remaining - timedelta).max(0.0);

    let distance_new = (time_new * curve_scale).powi(6) - curve_offset;

    let step = (distance - distance_new).max(0.0).min(distance);

    step * sign
}

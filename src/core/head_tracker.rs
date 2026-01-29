use std::mem;

use glam::{Mat3A, Quat};

pub struct HeadTracker {
    frame: u64,
    last: Quat,
    rotation: Quat,
    rotation_target: Quat,
}

impl HeadTracker {
    pub fn next_tracked(&mut self, frame: u64, frame_time: f32, new: Mat3A) -> Quat {
        let prev_frame = mem::replace(&mut self.frame, frame);

        if prev_frame != frame {
            let new = Quat::from_mat3a(&new);

            if prev_frame + 1 != frame {
                self.last = new;
            }

            self.rotation_target *= self.last.inverse() * new;
            self.rotation_target = self.rotation_target.normalize();
            self.last = new;
            self.rotate_towards_target(frame_time);
        }

        self.rotation
    }

    pub fn next_untracked(&mut self, frame: u64, frame_time: f32, new: Mat3A) -> Quat {
        let prev_frame = mem::replace(&mut self.frame, frame);

        if prev_frame != frame {
            self.rotation_target = Quat::IDENTITY;
            self.rotate_towards_target(frame_time);
            self.last = Quat::from_mat3a(&new);
        }

        self.rotation
    }

    fn rotate_towards_target(&mut self, frame_time: f32) {
        let distance = self.rotation.angle_between(self.rotation_target);
        let step = rip(distance, 0.0, 1.0, frame_time);

        self.rotation = self.rotation.rotate_towards(self.rotation_target, step);
    }
}

impl Default for HeadTracker {
    fn default() -> Self {
        Self {
            frame: 0,
            last: Quat::IDENTITY,
            rotation: Quat::IDENTITY,
            rotation_target: Quat::IDENTITY,
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

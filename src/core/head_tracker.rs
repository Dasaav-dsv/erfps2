use std::mem;

use glam::{Mat3A, Quat};

pub struct HeadTracker {
    frame: u64,
    last: Quat,
    rotation: Quat,
}

impl HeadTracker {
    pub fn next_tracked(&mut self, frame: u64, new: Mat3A) -> Quat {
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

    pub fn next_untracked(&mut self, frame: u64, new: Mat3A) -> Quat {
        let prev_frame = mem::replace(&mut self.frame, frame);

        if prev_frame == frame {
            return self.rotation;
        }

        self.rotation = self.rotation.slerp(Quat::IDENTITY, 0.35).normalize();
        self.last = Quat::from_mat3a(&new);

        self.rotation
    }
}

impl Default for HeadTracker {
    fn default() -> Self {
        Self {
            frame: 0,
            last: Quat::IDENTITY,
            rotation: Quat::IDENTITY,
        }
    }
}

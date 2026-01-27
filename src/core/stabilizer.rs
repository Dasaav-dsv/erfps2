use std::{collections::VecDeque, mem};

use glam::Vec3;

pub struct CameraStabilizer {
    frame: u64,
    samples: u32,
    buf: VecDeque<Vec3>,
}

impl CameraStabilizer {
    pub const fn new(samples: u32) -> Self {
        Self {
            frame: 0,
            samples,
            buf: VecDeque::new(),
        }
    }

    pub fn next(&mut self, frame: u64, new: Vec3) -> Vec3 {
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

    pub fn set_sample_count(&mut self, samples: u32) {
        self.samples = samples;
    }

    fn average(&self, default: Vec3) -> Vec3 {
        if !self.buf.is_empty() {
            self.buf.iter().sum::<Vec3>() / self.buf.len() as f32
        } else {
            default
        }
    }
}

use std::collections::VecDeque;

use glam::Vec3;

use crate::core::frame_cached::FrameCache;

pub struct CameraStabilizer {
    window: f32,
    samples: u32,
    buf: VecDeque<Vec3>,
}

impl CameraStabilizer {
    pub const DEFAULT_WINDOW: f32 = 0.3;

    pub const fn new(window: f32) -> Self {
        Self {
            window,
            samples: 0,
            buf: VecDeque::new(),
        }
    }

    pub fn set_window(&mut self, window: f32) {
        self.window = window;
    }

    fn average(&self, default: Vec3) -> Vec3 {
        if !self.buf.is_empty() {
            self.buf.iter().sum::<Vec3>() / self.buf.len() as f32
        } else {
            default
        }
    }
}

impl FrameCache for CameraStabilizer {
    type Input = Vec3;
    type Output<'a> = Vec3;

    fn update(&mut self, frame_time: f32, input: Self::Input) -> Self::Output<'_> {
        self.samples = (self.window / frame_time).ceil() as u32;
        self.buf.push_front(input);
        self.buf.truncate(self.samples as usize);
        self.average(input)
    }

    fn get_cached(&mut self, _frame_time: f32, input: Self::Input) -> Self::Output<'_> {
        self.average(input)
    }

    fn reset(&mut self) {
        self.buf.clear();
    }
}

impl Default for CameraStabilizer {
    fn default() -> Self {
        Self::new(Self::DEFAULT_WINDOW)
    }
}

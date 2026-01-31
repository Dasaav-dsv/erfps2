use std::time::Instant;

use crate::core::frame_cached::{FrameCache};

pub const FRAME_TIME_60: f32 = 1.0 / 60.0;

pub struct FrameTime {
    instant: Option<Instant>,
}

#[derive(Default)]
pub struct TransTime {
    time: f32,
}

impl TransTime {
    const STATE_TRANS_TIME: f32 = 0.233;

    pub fn can_transition(&self) -> bool {
        self.time > Self::STATE_TRANS_TIME
    }
}

impl FrameCache for FrameTime {
    type Input = ();
    type Output<'a> = f32;

    fn update(&mut self, _frame_time: f32, _input: Self::Input) -> Self::Output<'_> {
        let now = Instant::now();

        let elapsed = self
            .instant
            .and_then(|instant| now.checked_duration_since(instant))
            .map_or(FRAME_TIME_60, |dur| dur.as_secs_f32());

        self.instant = Some(now);

        elapsed
    }

    fn get_cached(&mut self, frame_time: f32, _input: Self::Input) -> Self::Output<'_> {
        frame_time
    }

    fn reset(&mut self) {
        self.instant = None;
    }
}

impl FrameCache for TransTime {
    type Input = ();
    type Output<'a> = f32;

    fn update(&mut self, frame_time: f32, _input: Self::Input) -> Self::Output<'_> {
        self.time += frame_time;
        self.time
    }

    fn get_cached(&mut self, _frame_time: f32, _input: Self::Input) -> Self::Output<'_> {
        self.time
    }

    fn reset(&mut self) {
        self.time = 0.0;
    }
}

impl Default for FrameTime {
    fn default() -> Self {
        Self {
            instant: Some(Instant::now()),
        }
    }
}

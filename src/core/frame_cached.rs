use std::ops::{Deref, DerefMut};

use crate::core::time::{FRAME_TIME_60, FrameTime};

pub struct FrameCached<T: FrameCache> {
    state: State,
    frame_time: f32,
    cache: T,
}

#[derive(PartialEq, Eq)]
struct State(u8);

impl State {
    const NOT_UPDATED: Self = Self(0);
    const STALE: Self = Self(1);
    const UPDATED: Self = Self(2);

    fn downgrade(&mut self) {
        self.0 = self.0.saturating_sub(1);
    }
}

pub trait FrameCache {
    type Input;
    type Output<'a>
    where
        Self: 'a;

    fn update(&mut self, frame_time: f32, input: Self::Input) -> Self::Output<'_>;

    fn get_cached(&mut self, frame_time: f32, input: Self::Input) -> Self::Output<'_>;

    fn reset(&mut self);
}

impl<T: FrameCache> FrameCached<T> {
    pub const fn new(cache: T) -> Self {
        Self {
            state: State::NOT_UPDATED,
            frame_time: FRAME_TIME_60,
            cache,
        }
    }

    pub fn next_frame(&mut self, frame_time: f32) {
        if self.state == State::STALE {
            self.cache.reset();
        }

        self.state.downgrade();

        self.frame_time = frame_time;
    }

    pub fn get(&mut self, input: T::Input) -> T::Output<'_> {
        if self.state != State::UPDATED {
            self.state = State::UPDATED;
            self.cache.update(self.frame_time, input)
        } else {
            self.cache.get_cached(self.frame_time, input)
        }
    }
}

impl FrameCached<FrameTime> {
    pub fn measure(&mut self) -> f32 {
        self.next_frame(FRAME_TIME_60);
        self.frame_time = self.get(());
        self.frame_time
    }
}

impl<T: Default + FrameCache> Default for FrameCached<T> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T: FrameCache> Deref for FrameCached<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.cache
    }
}

impl<T: FrameCache> DerefMut for FrameCached<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cache
    }
}

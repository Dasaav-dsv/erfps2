use std::ops::{Deref, DerefMut};

use eldenring::cs::{CSCamera, CSRemo, ChrCam, LockTgtMan, PlayerIns, WorldChrMan};
use fromsoftware_shared::FromStatic;

use crate::core::State;

pub trait WorldState: InWorldResult + Deref<Target = State> + DerefMut + WithLt + Sized {
    fn in_world<'s, R>(
        state: &'s mut State,
        f: impl for<'lt> FnOnce(Self::With<'lt>) -> R,
    ) -> Self::Result<R>;

    fn get<T>(&self) -> Option<&T>
    where
        for<'a> &'a T: FromWorld<&'a Self>,
    {
        <&T>::from_world(self)
    }

    #[allow(unused)]
    fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        for<'a> &'a mut T: FromWorld<&'a mut Self>,
    {
        <&mut T>::from_world(self)
    }
}

pub trait FromWorld<S>: Sized {
    fn from_world(state: S) -> Option<Self>;
}

pub trait InWorldResult {
    type Result<T>;
}

pub trait WithLt {
    type With<'lt>;
}

pub struct World<'s> {
    pub cs_cam: &'static mut CSCamera,
    pub chr_cam: &'static mut ChrCam,
    pub cs_remo: &'static mut CSRemo,
    pub lock_tgt: &'static mut LockTgtMan,
    pub player: &'static mut PlayerIns,
    pub state: &'s mut State,
}

pub struct Void<'s> {
    pub state: &'s mut State,
}

impl WorldState for World<'_> {
    fn in_world<'s, R>(
        state: &'s mut State,
        f: impl for<'lt> FnOnce(Self::With<'lt>) -> R,
    ) -> Self::Result<R> {
        let world_chr_man = unsafe { WorldChrMan::instance().ok()? };

        let cs_cam = unsafe { CSCamera::instance().ok()? };
        let chr_cam = unsafe { world_chr_man.chr_cam?.as_mut() };
        let cs_remo = unsafe { CSRemo::instance().ok()? };
        let lock_tgt = unsafe { LockTgtMan::instance().ok()? };
        let player = world_chr_man.main_player.as_deref_mut()?;

        let context = World {
            cs_cam,
            chr_cam,
            cs_remo,
            lock_tgt,
            player,
            state,
        };

        Some(f(context))
    }
}

impl<'s> InWorldResult for World<'s> {
    type Result<T> = Option<T>;
}

impl WithLt for World<'_> {
    type With<'lt> = World<'lt>;
}

impl WorldState for Void<'_> {
    fn in_world<'s, R>(
        state: &'s mut State,
        f: impl for<'lt> FnOnce(Self::With<'lt>) -> R,
    ) -> Self::Result<R> {
        f(Void { state })
    }
}

impl<'s> InWorldResult for Void<'s> {
    type Result<T> = T;
}

impl WithLt for Void<'_> {
    type With<'lt> = Void<'lt>;
}

impl Deref for World<'_> {
    type Target = State;

    fn deref(&self) -> &Self::Target {
        self.state
    }
}

impl DerefMut for World<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
    }
}

impl Deref for Void<'_> {
    type Target = State;

    fn deref(&self) -> &Self::Target {
        self.state
    }
}

impl DerefMut for Void<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
    }
}

impl<'a> FromWorld<&'a World<'_>> for &'a ChrCam {
    fn from_world(state: &'a World<'_>) -> Option<Self> {
        Some(state.chr_cam)
    }
}

impl<'a> FromWorld<&'a Void<'_>> for &'a ChrCam {
    fn from_world(_: &'a Void<'_>) -> Option<Self> {
        unsafe { Some(WorldChrMan::instance().ok()?.chr_cam?.as_ref()) }
    }
}

impl<'a> FromWorld<&'a World<'_>> for &'a CSRemo {
    fn from_world(state: &'a World<'_>) -> Option<Self> {
        Some(state.cs_remo)
    }
}

impl<'a> FromWorld<&'a Void<'_>> for &'a CSRemo {
    fn from_world(_: &'a Void<'_>) -> Option<Self> {
        unsafe { Some(CSRemo::instance().ok()?) }
    }
}

impl<'a> FromWorld<&'a World<'_>> for &'a LockTgtMan {
    fn from_world(state: &'a World<'_>) -> Option<Self> {
        Some(state.lock_tgt)
    }
}

impl<'a> FromWorld<&'a Void<'_>> for &'a LockTgtMan {
    fn from_world(_: &'a Void<'_>) -> Option<Self> {
        unsafe { Some(LockTgtMan::instance().ok()?) }
    }
}

impl<'a> FromWorld<&'a World<'_>> for &'a PlayerIns {
    fn from_world(state: &'a World<'_>) -> Option<Self> {
        Some(state.player)
    }
}

impl<'a> FromWorld<&'a Void<'_>> for &'a PlayerIns {
    fn from_world(_: &'a Void<'_>) -> Option<Self> {
        unsafe { WorldChrMan::instance().ok()?.main_player.as_deref() }
    }
}

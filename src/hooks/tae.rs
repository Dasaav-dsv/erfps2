use std::{
    ffi::c_void,
    ptr::{self, NonNull},
};

use eldenring::cs::{
    CSChrTaeAnimEvent, CSTaeAnimEventArgs, CSTaeAnimEventId, EnableTwistModifierArgs, PlayerIns,
    SetBulletAimAngleArgs,
};

use crate::{
    core::CoreLogic, hooks::install::hook, player::PlayerExt, program::Program,
    rva::CHR_TAE_ANIM_EVENT_VMT_RVA,
};

pub fn hook_tae(program: Program) -> eyre::Result<()> {
    let vtable = program.derva::<CSChrTaeAnimEventVtable>(CHR_TAE_ANIM_EVENT_VMT_RVA);

    unsafe {
        let exec_one = (*vtable).exec_one;

        hook(exec_one, |hook| {
            move |event, mut args| {
                let args = args.as_mut();

                let args_original = args.args;
                let mut args_override = override_tae_args(event.as_ref(), args);

                if let Some(args_override) = &mut args_override {
                    args.args = args_override.as_mut_ptr();
                }

                hook.call_original((event, args.into()));

                args.args = args_original;
            }
        })?;
    }

    Ok(())
}

#[cfg_attr(debug_assertions, libhotpatch::hotpatch)]
unsafe fn override_tae_args(
    event: &CSChrTaeAnimEvent,
    args: &CSTaeAnimEventArgs,
) -> Option<TaeArgsOverride> {
    if !ptr::addr_eq(event.owner, unsafe { PlayerIns::main_player()? })
        || !CoreLogic::is_first_person()
    {
        return None;
    }

    if args.event_id == CSTaeAnimEventId::SetBulletAimAngle {
        let mut args = unsafe { args.args.cast::<SetBulletAimAngleArgs>().as_ref()?.clone() };

        args.up_deadzone_angle = 0;
        args.down_deadzone_angle = 0;

        Some(TaeArgsOverride::SetBulletAimAngleArgs(args))
    } else if args.event_id == CSTaeAnimEventId::EnableTwistModifier {
        let mut args = unsafe {
            args.args
                .cast::<EnableTwistModifierArgs>()
                .as_ref()?
                .clone()
        };

        args.up_minimum_angle = 0.0;
        args.down_minimum_angle = 0.0;

        Some(TaeArgsOverride::EnableTwistModifierArgs(args))
    } else {
        None
    }
}

enum TaeArgsOverride {
    SetBulletAimAngleArgs(SetBulletAimAngleArgs),
    EnableTwistModifierArgs(EnableTwistModifierArgs),
}

impl TaeArgsOverride {
    fn as_mut_ptr(&mut self) -> *mut c_void {
        match self {
            Self::SetBulletAimAngleArgs(args) => &raw mut *args as *mut c_void,
            Self::EnableTwistModifierArgs(args) => &raw mut *args as *mut c_void,
        }
    }
}

type CSChrTaeAnimEventExecute =
    unsafe extern "C" fn(NonNull<CSChrTaeAnimEvent>, NonNull<CSTaeAnimEventArgs>);

#[repr(C)]
struct CSChrTaeAnimEventVtable {
    drop: unsafe extern "C" fn(*mut CSChrTaeAnimEvent, u8),
    exec_all: CSChrTaeAnimEventExecute,
    exec_one: CSChrTaeAnimEventExecute,
    exec_two: CSChrTaeAnimEventExecute,
}

use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

use diversion::hook::custom::{
    install_custom,
    place::{Place, Ref},
    x86_64::{Frame, R15},
};
use portable_atomic::AtomicF32;
use windows::{
    Win32::System::Memory::{PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect},
    core::PCWSTR,
};

use crate::{
    config::CrosshairKind,
    hooks::install::hook,
    program::Program,
    rva::{
        ADD_PIXEL_SHADER_RVA, CB_FISHEYE_HOOK_RVA, GX_FFX_DRAW_CONTEXT_RVA, GX_FFX_DRAW_PASS_RVA,
        USES_DITHERING_RVA,
    },
};

pub mod screen;

// Mirrors shaders/ToneMap_PostHook.hlsl.
#[allow(non_snake_case)]
#[repr(C)]
struct ToneMapCb {
    g_ToneMapInvSceneLumScale: [f32; 3],
    g_ErfpsFlags: i32,
    g_ReinhardParam: [f32; 4],
    g_ToneMapParam: [f32; 4],
    g_ToneMapSceneLumScale: [f32; 4],
    g_AdaptParam: [f32; 4],
    g_AdaptCenterWeight: [f32; 4],
    g_BrightPassThreshold: [f32; 4],
    g_GlareLuminance: [f32; 4],
    g_BloomBoostColor: [f32; 4],
    g_vBloomFinalColor: [f32; 4],
    g_vBloomScaleParam: [f32; 4],
    g_mtxColorMultiplyer: [f32; 12],
    g_vChromaticAberrationRG: [f32; 4],
    g_vChromaticAberrationB: [f32; 2],
    g_ErfpsCorrectParam: [f32; 2],
    g_bEnableFlags: [i32; 4],
    g_vFeedBackBlurParam: [f32; 4],
    g_vVignettingParam: [f32; 4],
    g_vHDRDisplayParam: [f32; 4],
    g_vChromaticAberrationShapeParam: [f32; 4],
    g_vScreenSize: [f32; 4],
    g_vSampleDistanceAdjust: [f32; 4],
    g_vMaxSampleCount: [i32; 4],
    g_vScenePreExposure: [f32; 4],
    g_vCameraParam: [f32; 2],
    g_ErfpsCrosshairScaleReciprocal: [f32; 2],
}

static TONE_MAP_HOOK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ToneMap_PostHook.ppo"));

pub fn hook_shaders(program: Program) -> eyre::Result<()> {
    unsafe {
        let add_pixel_shader = program.derva_ptr::<unsafe extern "C" fn(
            *mut c_void,
            PCWSTR,
            *const u8,
            usize,
        ) -> *mut c_void>(ADD_PIXEL_SHADER_RVA);

        hook(add_pixel_shader, |hook| {
            move |repository, name, mut blob, mut len| {
                if name
                    .to_string()
                    .is_ok_and(|name| name == "ToneMap_PostOETFPS")
                {
                    blob = TONE_MAP_HOOK.as_ptr();
                    len = TONE_MAP_HOOK.len();
                }

                hook.call_original((repository, name, blob, len))
            }
        })?;

        let uses_dithering = program
            .derva_ptr::<unsafe extern "C" fn(*const c_void, *mut c_void, u32) -> bool>(
                USES_DITHERING_RVA,
            );

        hook(uses_dithering, |hook| {
            move |param_1, param_2, param_3| {
                ENABLE_DITHERING.load(Ordering::Relaxed)
                    && hook.call_original((param_1, param_2, param_3))
            }
        })?;

        hook_shader_cb(program)?;

        patch_vfx_range(program)?;

        Ok(())
    }
}

static SHADER_FLAGS: AtomicU32 = AtomicU32::new(0);

static SHADER_CYLINDRICITY: AtomicF32 = AtomicF32::new(0.0);
static SHADER_STRENGTH_RATIO: AtomicF32 = AtomicF32::new(0.0);

static SHADER_XHAIR_SCALE_X: AtomicF32 = AtomicF32::new(0.0);
static SHADER_XHAIR_SCALE_Y: AtomicF32 = AtomicF32::new(0.0);

pub fn enable_fov_correction(
    state: bool,
    strength: f32,
    cylindricity: f32,
    use_barrel: bool,
    horizontal_fov: f32,
) {
    let state = state && strength > 0.05;

    set_shader_flag(state, 0);
    set_shader_flag(use_barrel, 1);

    if state {
        SHADER_CYLINDRICITY.store(cylindricity, Ordering::Release);

        let strength_width_ratio = strength * f32::tan(horizontal_fov * 0.5);
        SHADER_STRENGTH_RATIO.store(strength_width_ratio, Ordering::Release);
    }
}

pub fn set_crosshair(crosshair: CrosshairKind, scale: (f32, f32)) {
    let _ = SHADER_FLAGS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value & !0b11100 | (crosshair as u32 & 0b111) << 2)
    });

    SHADER_XHAIR_SCALE_X.store(scale.0.recip(), Ordering::Release);
    SHADER_XHAIR_SCALE_Y.store(scale.1.recip(), Ordering::Release);
}

fn get_fov_correction() -> (f32, f32) {
    let cylindricity = SHADER_CYLINDRICITY.load(Ordering::Acquire);
    let strength_width_ratio = SHADER_STRENGTH_RATIO.load(Ordering::Acquire);

    (cylindricity, strength_width_ratio)
}

unsafe fn hook_shader_cb(program: Program) -> eyre::Result<()> {
    unsafe {
        let cb_fisheye_hook = program.derva_ptr::<*const ()>(CB_FISHEYE_HOOK_RVA);

        install_custom(cb_fisheye_hook)?.hook(|_| {
            |tone_map: Frame<ToneMapCb, -0x50>, is_enabled: R15<Ref<bool, 0xcb0>>| {
                let flags = SHADER_FLAGS.load(Ordering::Acquire);
                if flags == 0 {
                    return;
                }

                let cylindricity = SHADER_CYLINDRICITY.load(Ordering::Acquire);
                let strength_width_ratio = SHADER_STRENGTH_RATIO.load(Ordering::Acquire);

                let crosshair_scale_x = SHADER_XHAIR_SCALE_X.load(Ordering::Acquire);
                let crosshair_scale_y = SHADER_XHAIR_SCALE_Y.load(Ordering::Acquire);

                let mut tone_map = tone_map.read();

                tone_map.g_ErfpsFlags = flags as i32;
                tone_map.g_ErfpsCorrectParam = [cylindricity, strength_width_ratio];
                tone_map.g_ErfpsCrosshairScaleReciprocal = [crosshair_scale_x, crosshair_scale_y];

                *is_enabled.read() = true;
            }
        });
    }

    Ok(())
}

unsafe fn patch_vfx_range(program: Program) -> eyre::Result<()> {
    unsafe {
        let ffx_draw_pass = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool>(
                GX_FFX_DRAW_PASS_RVA,
            );

        hook(ffx_draw_pass, |hook| {
            move |param_1, param_2| {
                if !ENABLE_VFX_FADE.load(Ordering::Relaxed) {
                    return false;
                }

                hook.call_original((param_1, param_2))
            }
        })?;

        // or eax,-1
        // vcvtsi2ss xmm11,xmm11,eax
        let ffx_draw_context_buf = [0x83, 0xC8, 0xFF, 0xC5, 0x22, 0x2A, 0xD8];

        let ffx_draw_context_mem = program.derva::<[u8; 7]>(GX_FFX_DRAW_CONTEXT_RVA);

        VirtualProtect(
            ffx_draw_context_mem as *const c_void,
            ffx_draw_context_buf.len(),
            PAGE_EXECUTE_READWRITE,
            &mut PAGE_PROTECTION_FLAGS::default(),
        )?;

        ffx_draw_context_mem.write(ffx_draw_context_buf);
    }

    Ok(())
}

static ENABLE_VFX_FADE: AtomicBool = AtomicBool::new(true);

pub fn enable_vfx_fade(state: bool) {
    ENABLE_VFX_FADE.store(state, Ordering::Relaxed);
}

static ENABLE_DITHERING: AtomicBool = AtomicBool::new(true);

pub fn enable_dithering(state: bool) {
    ENABLE_DITHERING.store(state, Ordering::Relaxed);
}

fn set_shader_flag(state: bool, pos: u32) -> u32 {
    let flag = 1 << pos;
    match state {
        true => SHADER_FLAGS.fetch_or(flag, Ordering::Relaxed),
        false => SHADER_FLAGS.fetch_and(!flag, Ordering::Relaxed),
    }
}

fn get_shader_flag(pos: u32) -> bool {
    (SHADER_FLAGS.load(Ordering::Relaxed) >> pos) & 1 != 0
}

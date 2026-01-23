use std::{
    arch::naked_asm,
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

use windows::{
    Win32::System::Memory::{PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect},
    core::PCWSTR,
};

use crate::{
    config::CrosshairKind,
    hook,
    program::Program,
    rva::{
        ADD_PIXEL_SHADER_RVA, CB_FISHEYE_HOOK_RVA, GX_FFX_DRAW_CONTEXT_RVA, GX_FFX_DRAW_PASS_RVA,
        USES_DITHERING_RVA,
    },
};

static TONE_MAP_HOOK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ToneMap_PostHook.ppo"));

pub fn hook_shaders(program: Program) -> eyre::Result<()> {
    unsafe {
        let add_pixel_shader = program.derva_ptr::<unsafe extern "C" fn(
            *mut c_void,
            PCWSTR,
            *const u8,
            usize,
        ) -> *mut c_void>(ADD_PIXEL_SHADER_RVA);

        hook::install(add_pixel_shader, |original| {
            move |repository, name, mut blob, mut len| {
                if name
                    .to_string()
                    .is_ok_and(|name| name == "ToneMap_PostOETFPS")
                {
                    blob = TONE_MAP_HOOK.as_ptr();
                    len = TONE_MAP_HOOK.len();
                }

                original(repository, name, blob, len)
            }
        })
        .unwrap();

        let uses_dithering = program
            .derva_ptr::<unsafe extern "C" fn(*const c_void, *mut c_void, u32) -> bool>(
                USES_DITHERING_RVA,
            );

        hook::install(uses_dithering, |original| {
            move |param_1, param_2, param_3| {
                ENABLE_DITHERING.load(Ordering::Relaxed) && original(param_1, param_2, param_3)
            }
        })
        .unwrap();

        hook_shader_cb(program)?;

        patch_vfx_range(program)?;

        Ok(())
    }
}

static SHADER_FLAGS: AtomicU32 = AtomicU32::new(0);
static SHADER_PARAMS: AtomicU64 = AtomicU64::new(0);
static SHADER_PARAMS2: AtomicU64 = AtomicU64::new(0);

pub fn enable_fov_correction(
    state: bool,
    strength: f32,
    cylindricity: f32,
    use_barrel: bool,
    vfov: f32,
) {
    let state = state && strength > 0.05;

    set_shader_flag(state, 0);
    set_shader_flag(use_barrel, 1);

    if state {
        let cylindricity = cylindricity.to_bits() as u64;

        let strength_width_ratio = strength * f32::tan(vfov * 0.5);
        let strength_width_ratio = strength_width_ratio.to_bits() as u64;

        SHADER_PARAMS.store(
            cylindricity | (strength_width_ratio << 32),
            Ordering::Relaxed,
        );
    }
}

pub fn set_crosshair(crosshair: CrosshairKind, scale: (f32, f32)) {
    let _ = SHADER_FLAGS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value & !0b11100 | (crosshair as u32 & 0b111) << 2)
    });

    let rscale_x = scale.0.recip().to_bits() as u64;
    let rscale_y = scale.1.recip().to_bits() as u64;

    SHADER_PARAMS2.store(rscale_x | (rscale_y << 32), Ordering::Relaxed);
}

unsafe fn hook_shader_cb(program: Program) -> eyre::Result<()> {
    #[unsafe(naked)]
    extern "C" fn fisheye_distortion_cb_hook() {
        naked_asm! {
            // Original code start...
            "mov r8,[rsp+0x78]",
            "lea rdx,[rbp-0x80]",
            "mov rcx,[r14+0x08]",
            // ...original code end.
            // Forward the flags to the constant buffer (see "shaders/ToneMap_PostHook.hlsl").
            "mov eax,[rip+{}]",
            "mov [rbp-0x44],eax",
            // Force the shader on.
            "test eax,eax",
            "setne al",
            "mov [r15+0xcb0],al",
            // Forward the screen width ratio to the shader (see above).
            "mov rax,[rip+{}]",
            "mov [rbp+0xa8],rax",
            // Forward the crosshair size.
            "mov rax,[rip+{}]",
            "mov [rbp+0x148],rax",
            "ret",
            sym SHADER_FLAGS,
            sym SHADER_PARAMS,
            sym SHADER_PARAMS2,
        }
    }

    // 00 CALL [0x0A]
    // 06 JMP 0x15
    // 08 JMP 0x00
    // 0A DQ `fisheye_distortion_cb_hook`
    // 12 int3 int3 int3
    // 15 ...
    let cb_hook_buf = {
        let [b0, b1, b2, b3, b4, b5, b6, b7] =
            u64::to_le_bytes(fisheye_distortion_cb_hook as usize as u64);
        [
            0xff, 0x15, 0x04, 0x00, 0x00, 0x00, 0xeb, 0x0d, 0xeb, 0xf6, b0, b1, b2, b3, b4, b5, b6,
            b7, 0xcc, 0xcc, 0xcc,
        ]
    };

    let cb_hook_mem = program.derva::<[u8; 21]>(CB_FISHEYE_HOOK_RVA);

    unsafe {
        VirtualProtect(
            cb_hook_mem as *const c_void,
            cb_hook_buf.len(),
            PAGE_EXECUTE_READWRITE,
            &mut PAGE_PROTECTION_FLAGS::default(),
        )?;

        cb_hook_mem.write(cb_hook_buf);
    }

    Ok(())
}

unsafe fn patch_vfx_range(program: Program) -> eyre::Result<()> {
    unsafe {
        let ffx_draw_pass = program
            .derva_ptr::<unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool>(
                GX_FFX_DRAW_PASS_RVA,
            );

        hook::install(ffx_draw_pass, |original| {
            move |param_1, param_2| {
                if !ENABLE_VFX_FADE.load(Ordering::Relaxed) {
                    return false;
                }

                original(param_1, param_2)
            }
        })
        .unwrap();

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

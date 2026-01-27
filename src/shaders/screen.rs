use glam::Vec2;

use crate::shaders::{get_fov_correction, get_shader_flag};

pub fn correct_screen_coords(xy: Vec2) -> Vec2 {
    if !get_shader_flag(0) {
        return xy;
    }

    let xy = xy.clamp(Vec2::ZERO, Vec2::ONE);
    if get_shader_flag(1) {
        correct_screen_coords_barrel(xy)
    } else {
        correct_screen_coords_fisheye(xy)
    }
}

fn correct_screen_coords_fisheye(uv: Vec2) -> Vec2 {
    let uv = uv - 0.5;

    let strength = get_fov_correction().1;
    let k_max = 1.0 + strength * 0.125;

    let uvk = uv * k_max;

    if uvk.x == 0.0 {
        return uvk + 0.5;
    }

    let x = uvk.x.abs();
    let mut y = x + 0.07 * (strength * x * x) - 0.5 * (strength * x * x) * x;

    let f = |y: f32| (strength * (y * y)) * (y * y) + y - x;
    let f_prime = |y: f32| 4.0 * (strength * (y * y)) * y + 1.0;

    y -= f(y) / f_prime(y);

    let uv = Vec2::new(y.copysign(uvk.x), uvk.y * y / x);

    uv + 0.5
}

// Source: https://www.decarpentier.nl/lens-distortion
//
// Copyright (c) 2015, Giliam de Carpentier
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the
// documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
// CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
fn correct_screen_coords_barrel(xy: Vec2) -> Vec2 {
    const ASPECT_RATIO: f32 = 16.0 / 9.0;

    let (cylindrical_ratio, strength_width_ratio) = get_fov_correction();
    let scaled_height = strength_width_ratio / ASPECT_RATIO;
    let cyl_aspect_ratio = ASPECT_RATIO * cylindrical_ratio;
    let cyl_aspect_ratio_sq = cyl_aspect_ratio * cyl_aspect_ratio;
    let aspect_diag_sq = ASPECT_RATIO * ASPECT_RATIO + 1.0;
    let diag_sq = scaled_height * scaled_height * aspect_diag_sq;
    let signed_uv = 2.0 * xy - 1.0;

    let z = 0.5 * (diag_sq + 1.0).sqrt() + 0.5;
    let ny = (z - 1.0) / (cyl_aspect_ratio_sq + 1.0);
    let nx = cyl_aspect_ratio_sq * ny;
    let p_sq = signed_uv * signed_uv;
    let ivp = (0.25 + z * (nx * p_sq.x + ny * p_sq.y)).sqrt();

    z * signed_uv / (0.5 + ivp) * 0.5 + 0.5
}

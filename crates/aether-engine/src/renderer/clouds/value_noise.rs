//! Shared value-noise and FBM helpers for cloud noise generation.
//!
//! Used by the CPU-side procedural noise generators (Perlin-Worley, curl,
//! weather) to keep the lattice hash and fractal summation logic in one place.

use glam::{Vec2, Vec3};

/// Fractal Brownian Motion using 3D Perlin-like value noise.
pub(crate) fn fbm_perlin_3d(p: Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise_3d(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

/// Trilinearly interpolated value noise on an integer lattice.
pub(crate) fn value_noise_3d(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let ix = i.x as i32;
    let iy = i.y as i32;
    let iz = i.z as i32;

    let u = smoothstep(f.x);
    let v = smoothstep(f.y);
    let w = smoothstep(f.z);

    let c000 = hash3(ix, iy, iz);
    let c100 = hash3(ix + 1, iy, iz);
    let c010 = hash3(ix, iy + 1, iz);
    let c110 = hash3(ix + 1, iy + 1, iz);
    let c001 = hash3(ix, iy, iz + 1);
    let c101 = hash3(ix + 1, iy, iz + 1);
    let c011 = hash3(ix, iy + 1, iz + 1);
    let c111 = hash3(ix + 1, iy + 1, iz + 1);

    let c00 = c000 + (c100 - c000) * u;
    let c01 = c001 + (c101 - c001) * u;
    let c10 = c010 + (c110 - c010) * u;
    let c11 = c011 + (c111 - c011) * u;

    let c0 = c00 + (c10 - c00) * v;
    let c1 = c01 + (c11 - c01) * v;

    c0 + (c1 - c0) * w
}

/// Deterministic hash for an integer lattice corner, normalized to `[0, 1]`.
fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ z.wrapping_mul(2086444801);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    n as u32 as f32 / u32::MAX as f32
}

/// Polynomial smoothstep (`3t² - 2t³`).
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Fractal Brownian Motion using 2D Perlin-like value noise.
pub(crate) fn fbm_perlin_2d(p: Vec2, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise_2d(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

/// Bilinearly interpolated value noise on an integer lattice.
pub(crate) fn value_noise_2d(p: Vec2) -> f32 {
    let i = p.floor();
    let f = p - i;
    let ix = i.x as i32;
    let iy = i.y as i32;

    let u = smoothstep(f.x);
    let v = smoothstep(f.y);

    let c00 = hash2(ix, iy);
    let c10 = hash2(ix + 1, iy);
    let c01 = hash2(ix, iy + 1);
    let c11 = hash2(ix + 1, iy + 1);

    let c0 = c00 + (c10 - c00) * u;
    let c1 = c01 + (c11 - c01) * u;

    c0 + (c1 - c0) * v
}

/// Deterministic hash for an integer lattice corner, normalized to `[0, 1]`.
fn hash2(x: i32, y: i32) -> f32 {
    let mut n = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    n as u32 as f32 / u32::MAX as f32
}

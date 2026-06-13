//! Procedural noise generation for volumetric clouds.
//!
//! Provides CPU-side generation of 3D density noise textures that can be
//! uploaded to the GPU and sampled by the cloud ray-marching shader.

use glam::Vec3;

/// Generate a 3D R8Unorm noise texture for cloud density.
///
/// The texture contains layered value noise with multiple octaves so that
/// the shader can produce soft, billowy cloud shapes. The result is fully
/// deterministic for a given `size`.
pub fn generate_cloud_noise_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let data = generate_noise_data(size);

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Cloud Noise Texture"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        },
    );

    (texture, view)
}

/// Generate normalized 3D noise data of the requested size.
fn generate_noise_data(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size * size) as usize];

    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = Vec3::new(x as f32, y as f32, z as f32) / size as f32;
                let value = fbm_noise(p, 4, 2.0, 0.5);
                let idx = ((z * size * size) + (y * size) + x) as usize;
                data[idx] = (value.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    data
}

/// Fractal Brownian Motion value noise.
fn fbm_noise(p: Vec3, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += amplitude * value_noise(p * frequency);
        max_value += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    total / max_value
}

/// Trilinearly interpolated value noise on an integer lattice.
fn value_noise(p: Vec3) -> f32 {
    let i = p.floor().as_ivec3();
    let f = p.fract();
    let u = smooth3(f);
    trilinear(i, f, u)
}

/// Proper trilinear interpolation of lattice corner hashes.
fn trilinear(i: glam::IVec3, _f: Vec3, u: Vec3) -> f32 {
    let mut value = 0.0;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let corner = i + glam::IVec3::new(x, y, z);
                let hash = hash3(corner);
                let wx = if x == 0 { 1.0 - u.x } else { u.x };
                let wy = if y == 0 { 1.0 - u.y } else { u.y };
                let wz = if z == 0 { 1.0 - u.z } else { u.z };
                value += hash * wx * wy * wz;
            }
        }
    }
    value
}

/// Deterministic hash for an integer lattice corner.
fn hash3(p: glam::IVec3) -> f32 {
    let mut n =
        p.x.wrapping_mul(374761393) ^ p.y.wrapping_mul(668265263) ^ p.z.wrapping_mul(2086444801);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    (n as f32 / u32::MAX as f32).clamp(0.0, 1.0)
}

/// Component-wise smoothstep (3x^2 - 2x^3).
fn smooth3(f: Vec3) -> Vec3 {
    f * f * (3.0 - 2.0 * f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_noise_data_has_expected_size() {
        let size = 16;
        let data = generate_noise_data(size);
        assert_eq!(data.len(), (size * size * size) as usize);
    }

    #[test]
    fn cloud_noise_texture_matches_dimensions() {
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("device");

            let (texture, view) = generate_cloud_noise_texture(&device, &queue, 32);
            assert_eq!(texture.width(), 32);
            assert_eq!(texture.height(), 32);
            assert_eq!(texture.depth_or_array_layers(), 32);
            assert_eq!(texture.format(), wgpu::TextureFormat::R8Unorm);
            let _ = view; // ensure view is created
        });
    }
}

//! 03_gltf_scene – Scene Loading with GLTF/OBJ + Deferred Shading
//!
//! Loads a 3D model and renders it with the deferred pipeline
//! (GBuffer → Lighting → Tonemap) with shadow mapping.

use crate::{
    asset::{
        format::SceneLoader,
        gpu_material::GpuMaterial,
        mesh::{CpuMesh, GpuMesh},
        scene::{MeshData, SceneData, SceneNode, TransformData},
    },
    examples::{Example, ExampleStateSnapshot},
    input::InputManager,
    renderer::{
        camera::OrbitCamera,
        context::{GBuffer, RenderContext},
        passes::{
            gbuffer::{GBufferPass, MaterialUniform, Renderable},
            lighting::{Light, LightsUniform, LightingPass, LIGHT_TYPE_DIRECTIONAL},
            shadow::ShadowPass,
            tonemap::TonemapPass,
        },
    },
};
use glam::{Mat4, Vec3};
use std::sync::Arc;
use tracing::info;

/// GLTF scene loading + deferred rendering example.
pub struct GltfSceneExample {
    _model_path: String,
    scene_data: SceneData,

    // GPU resources – wrapped in Option so `cleanup` can drop them.
    gbuffer_pass: Option<GBufferPass>,
    shadow_pass: Option<ShadowPass>,
    lighting_pass: Option<LightingPass>,
    tonemap_pass: Option<TonemapPass>,
    gbuffer: Option<GBuffer>,
    hdr_texture: Option<wgpu::Texture>,
    renderables: Option<Vec<Renderable>>,

    // Camera & projection
    orbit_cam: Option<OrbitCamera>,
    proj: Option<Mat4>,

    // Lighting
    lights_uniform: Option<LightsUniform>,
    light_dir: Vec3,
    scene_center: Vec3,
    shadow_radius: f32,

    // UI state
    show_wireframe: bool,
    show_gbuffer_vis: bool,
    exposure: f32,

    // Timing
    last_frame_time: std::time::Instant,
    fps: f32,
}

impl GltfSceneExample {
    /// Create a new example.  CPU-side scene data is loaded immediately;
    /// GPU resources are allocated in [`Example::init`].
    pub fn new(model_path: impl Into<String>) -> Self {
        let model_path = model_path.into();

        let scene_loader = SceneLoader::new();
        let scene_data = scene_loader
            .load(std::path::Path::new(&model_path))
            .expect("Failed to load scene");

        info!(
            "Scene loaded: {} ({} nodes, {} meshes, {} materials)",
            scene_data.name,
            scene_data.nodes.len(),
            scene_data.meshes.len(),
            scene_data.materials.len()
        );

        let (scene_center, scene_radius) = scene_bounding_sphere(&scene_data);
        let shadow_radius = scene_radius.max(15.0);
        let light_dir = Vec3::new(-1.0, -1.0, -1.0).normalize();

        Self {
            _model_path: model_path,
            scene_data,
            gbuffer_pass: None,
            shadow_pass: None,
            lighting_pass: None,
            tonemap_pass: None,
            gbuffer: None,
            hdr_texture: None,
            renderables: None,
            orbit_cam: None,
            proj: None,
            lights_uniform: None,
            light_dir,
            scene_center,
            shadow_radius,
            show_wireframe: false,
            show_gbuffer_vis: false,
            exposure: 1.0,
            last_frame_time: std::time::Instant::now(),
            fps: 0.0,
        }
    }

    /// Convenience constructor that uses the default duck model.
    ///
    /// The model is embedded in the binary via `include_bytes!`.  On first
    /// run it is unpacked to the system temp directory; subsequent runs reuse
    /// the cached file.
    pub fn with_default_model() -> Self {
        const EMBEDDED: &[u8] = include_bytes!("../../examples/assets/duck.glb");
        let cache_dir = std::env::temp_dir().join("aether-engine");
        std::fs::create_dir_all(&cache_dir).expect("Failed to create temp dir");
        let cached = cache_dir.join("duck.glb");
        if !cached.exists() {
            std::fs::write(&cached, EMBEDDED).expect("Failed to write embedded model");
        }
        Self::new(cached.to_string_lossy())
    }
}

// ---------------------------------------------------------------------------
// Example trait
// ---------------------------------------------------------------------------

impl Example for GltfSceneExample {
    fn name(&self) -> &'static str {
        "03_gltf_scene"
    }

    fn description(&self) -> &'static str {
        "Scene loading with GLTF/OBJ + Deferred shading + Shadow mapping."
    }

    fn init(&mut self, ctx: &RenderContext) -> anyhow::Result<()> {
        let gbuffer_pass = GBufferPass::new(&ctx.device, &ctx.queue);

        // Upload meshes to GPU
        let mut gpu_meshes: Vec<Arc<GpuMesh>> = Vec::new();
        for mesh_data in &self.scene_data.meshes {
            let cpu_mesh = mesh_data_to_cpu_mesh(mesh_data);
            let gpu_mesh = GpuMesh::from_cpu(&ctx.device, &cpu_mesh);
            gpu_meshes.push(Arc::new(gpu_mesh));
        }

        // Build renderables from scene nodes
        let mut renderables = Vec::new();
        for node in &self.scene_data.nodes {
            if let Some(mesh_idx) = node.mesh_index {
                if let Some(gpu_mesh) = gpu_meshes.get(mesh_idx) {
                    let transform = transform_from_data(&node.transform);
                    let material = material_from_scene(&self.scene_data, node);
                    let gpu_material = Arc::new(GpuMaterial::from_uniform(
                        &ctx.device,
                        &ctx.queue,
                        gbuffer_pass.material_texture_bind_group_layout(),
                        &material,
                    ));

                    renderables.push(Renderable {
                        mesh: gpu_mesh.clone(),
                        transform,
                        material,
                        material_bind_group: Some(gpu_material.bind_group.clone()),
                    });
                }
            }
        }

        // Ground plane
        {
            let ground_cpu = CpuMesh::quad();
            let ground_gpu = Arc::new(GpuMesh::from_cpu(&ctx.device, &ground_cpu));
            let ground_material = MaterialUniform {
                albedo: [0.15, 0.18, 0.22, 1.0],
                roughness: 0.95,
                metallic: 0.0,
                ao: 1.0,
                emissive_intensity: 0.0,
            };
            let ground_mat = Arc::new(GpuMaterial::from_uniform(
                &ctx.device,
                &ctx.queue,
                gbuffer_pass.material_texture_bind_group_layout(),
                &ground_material,
            ));
            let ground_transform = Mat4::from_scale(Vec3::new(20.0, 20.0, 1.0))
                * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            renderables.push(Renderable {
                mesh: ground_gpu,
                transform: ground_transform,
                material: ground_material,
                material_bind_group: Some(ground_mat.bind_group.clone()),
            });
        }

        // Fallback cube
        if renderables.is_empty() {
            let cube_cpu = CpuMesh::cube();
            let cube_gpu = Arc::new(GpuMesh::from_cpu(&ctx.device, &cube_cpu));
            let default_mat = Arc::new(GpuMaterial::new_default(
                &ctx.device,
                &ctx.queue,
                gbuffer_pass.material_texture_bind_group_layout(),
            ));
            renderables.push(Renderable {
                mesh: cube_gpu,
                transform: Mat4::IDENTITY,
                material: MaterialUniform::default(),
                material_bind_group: Some(default_mat.bind_group.clone()),
            });
        }

        info!("Created {} renderables", renderables.len());

        let gbuffer = GBuffer::new(&ctx.device, ctx.config.width, ctx.config.height);
        let hdr_texture = create_hdr_texture(&ctx.device, ctx.config.width, ctx.config.height);

        let mut shadow_pass = ShadowPass::new(&ctx.device, 2048);
        let lighting_pass = LightingPass::new(
            &ctx.device,
            &gbuffer,
            shadow_pass.shadow_map_view(),
            &shadow_pass.shadow_sampler,
        );
        let tonemap_pass = TonemapPass::new(
            &ctx.device,
            &hdr_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            ctx.config.format,
        );

        // Camera
        let orbit_cam = init_camera_from_scene(&self.scene_data);
        let proj = Mat4::perspective_rh(
            45.0f32.to_radians(),
            ctx.config.width as f32 / ctx.config.height as f32,
            0.1,
            1000.0,
        );

        // Lighting
        let mut lights_uniform = LightsUniform {
            camera_pos: orbit_cam.position().into(),
            light_count: 1,
            ..Default::default()
        };
        lights_uniform.lights[0] = Light {
            position: self.light_dir.into(),
            light_type: LIGHT_TYPE_DIRECTIONAL,
            color: [1.0, 1.0, 1.0],
            intensity: 1.5,
            ..Default::default()
        };
        lighting_pass.update_lights(&ctx.queue, &lights_uniform);
        shadow_pass.update_light_matrix(
            &ctx.queue,
            self.light_dir,
            self.scene_center,
            self.shadow_radius,
        );
        lighting_pass.update_shadow_uniforms(&ctx.queue, &shadow_pass.light_matrix());

        self.gbuffer_pass = Some(gbuffer_pass);
        self.shadow_pass = Some(shadow_pass);
        self.lighting_pass = Some(lighting_pass);
        self.tonemap_pass = Some(tonemap_pass);
        self.gbuffer = Some(gbuffer);
        self.hdr_texture = Some(hdr_texture);
        self.renderables = Some(renderables);
        self.orbit_cam = Some(orbit_cam);
        self.proj = Some(proj);
        self.lights_uniform = Some(lights_uniform);
        self.last_frame_time = std::time::Instant::now();
        self.fps = 0.0;

        Ok(())
    }

    fn cleanup(&mut self, _ctx: &RenderContext) {
        self.gbuffer_pass = None;
        self.shadow_pass = None;
        self.lighting_pass = None;
        self.tonemap_pass = None;
        self.gbuffer = None;
        self.hdr_texture = None;
        self.renderables = None;
        self.orbit_cam = None;
        self.proj = None;
        self.lights_uniform = None;
    }

    fn update(&mut self, _ctx: &RenderContext, _dt: f32, input: &InputManager) {
        use winit::event::MouseButton;

        let cam = self.orbit_cam.as_mut().unwrap();
        let (dx, dy) = input.mouse_delta();
        let scroll = input.scroll_delta();

        if input.mouse_held(MouseButton::Left) {
            cam.update(dx, dy, 0.0);
        }
        if input.mouse_held(MouseButton::Right) {
            cam.pan(dx, dy);
        }
        if scroll != 0.0 {
            cam.update(0.0, 0.0, scroll * 5.0);
        }

        let lights = self.lights_uniform.as_mut().unwrap();
        lights.camera_pos = cam.position().into();
    }

    fn prepare(&mut self, ctx: &RenderContext) {
        let _cam = self.orbit_cam.as_ref().unwrap();
        let lights = self.lights_uniform.as_ref().unwrap();

        self.lighting_pass
            .as_ref()
            .unwrap()
            .update_lights(&ctx.queue, lights);

        let shadow_pass_mut = self.shadow_pass.as_mut().unwrap();
        shadow_pass_mut.update_light_matrix(
            &ctx.queue,
            self.light_dir,
            self.scene_center,
            self.shadow_radius,
        );
        let light_matrix = shadow_pass_mut.light_matrix();
        self.lighting_pass
            .as_ref()
            .unwrap()
            .update_shadow_uniforms(&ctx.queue, &light_matrix);

        // Exposure change → update GPU buffer.
        // (We use update_exposure which uploads; recreate_bind_group is only needed when
        // the HDR texture view itself changes, e.g. on resize.)
        let tonemap = self.tonemap_pass.as_mut().unwrap();
        if (self.exposure - tonemap.exposure()).abs() > 0.001 {
            tonemap.update_exposure(&ctx.queue, self.exposure);
        }
    }

    fn render(
        &mut self,
        ctx: &RenderContext,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) -> anyhow::Result<()> {
        let view = self.orbit_cam.as_ref().unwrap().view_matrix();
        let proj = self.proj.as_ref().unwrap();
        let renderables = self.renderables.as_ref().unwrap();
        let gbuffer = self.gbuffer.as_ref().unwrap();
        let hdr_texture = self.hdr_texture.as_ref().unwrap();

        // 0. Shadow pass
        self.shadow_pass
            .as_ref()
            .unwrap()
            .execute(encoder, renderables, &ctx.queue);

        // 1. G-Buffer pass
        self.gbuffer_pass
            .as_ref()
            .unwrap()
            .execute(encoder, gbuffer, ctx, renderables, &view, proj);

        // 2. Lighting pass → HDR texture
        let hdr_view = hdr_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.lighting_pass
            .as_ref()
            .unwrap()
            .execute(encoder, &hdr_view);

        // 3. Tonemap pass → swapchain
        self.tonemap_pass
            .as_ref()
            .unwrap()
            .execute(encoder, target_view);

        Ok(())
    }

    fn ui(&mut self, egui_ctx: &egui::Context) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        let frame_time_ms = dt * 1000.0;
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }

        egui::Window::new("Aether Debug")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
            .show(egui_ctx, |ui| {
                ui.label(format!("FPS: {:.1}", self.fps));
                ui.label(format!("Frame: {:.2} ms", frame_time_ms));
                ui.label(format!(
                    "Meshes: {} | Materials: {}",
                    self.scene_data.meshes.len(),
                    self.scene_data.materials.len()
                ));
                ui.label(format!(
                    "Nodes: {} | Renderables: {}",
                    self.scene_data.nodes.len(),
                    self.renderables.as_ref().map(|r| r.len()).unwrap_or(0)
                ));
                ui.separator();
                ui.checkbox(&mut self.show_wireframe, "Wireframe");
                ui.checkbox(&mut self.show_gbuffer_vis, "G-Buffer Vis");
                ui.add(egui::Slider::new(&mut self.exposure, 0.1..=5.0).text("Exposure"));
            });
    }

    fn resize(&mut self, ctx: &RenderContext, width: u32, height: u32) {
        let gbuffer = GBuffer::new(&ctx.device, width, height);
        let hdr_texture = create_hdr_texture(&ctx.device, width, height);

        self.lighting_pass
            .as_mut()
            .unwrap()
            .recreate_bind_group(&ctx.device, &gbuffer);
        self.tonemap_pass.as_mut().unwrap().recreate_bind_group(
            &ctx.device,
            &hdr_texture.create_view(&wgpu::TextureViewDescriptor::default()),
        );

        self.gbuffer = Some(gbuffer);
        self.hdr_texture = Some(hdr_texture);
        self.proj = Some(Mat4::perspective_rh(
            45.0f32.to_radians(),
            width as f32 / height as f32,
            0.1,
            1000.0,
        ));
    }

    fn snapshot(&self) -> Option<ExampleStateSnapshot> {
        let cam_pos = self.orbit_cam.as_ref().map(|c| {
            let p = c.position();
            [p.x, p.y, p.z]
        });
        Some(ExampleStateSnapshot {
            fps: self.fps,
            frame_time_ms: if self.fps > 0.0 { 1000.0 / self.fps } else { 0.0 },
            renderable_count: self.renderables.as_ref().map(|r| r.len()),
            camera_position: cam_pos,
            custom: vec![
                ("scene_name".to_string(), self.scene_data.name.clone()),
                ("meshes".to_string(), self.scene_data.meshes.len().to_string()),
                ("nodes".to_string(), self.scene_data.nodes.len().to_string()),
            ],
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn init_camera_from_scene(scene: &SceneData) -> OrbitCamera {
    let (center, radius) = scene_bounding_sphere(scene);
    info!("Scene bounds: center={:?}, radius={:.2}", center, radius);
    OrbitCamera {
        target: center,
        distance: radius * 3.0,
        polar: std::f32::consts::FRAC_PI_4,
        ..Default::default()
    }
}

fn scene_bounding_sphere(scene: &SceneData) -> (Vec3, f32) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);

    for mesh in &scene.meshes {
        for pos in &mesh.positions {
            let p = Vec3::from_array(*pos);
            min = min.min(p);
            max = max.max(p);
        }
    }

    let center = (min + max) * 0.5;
    let radius = (max - min).length() * 0.5;

    if radius.is_finite() && radius > 0.0 {
        (center, radius)
    } else {
        (Vec3::ZERO, 1.0)
    }
}

fn mesh_data_to_cpu_mesh(mesh_data: &MeshData) -> crate::asset::mesh::CpuMesh {
    crate::asset::mesh::CpuMesh {
        positions: mesh_data.positions.clone(),
        normals: mesh_data.normals.clone(),
        uvs: mesh_data.uvs.clone(),
        tangents: mesh_data.tangents.clone(),
        indices: mesh_data.indices.clone(),
    }
}

fn transform_from_data(transform: &TransformData) -> Mat4 {
    let translation = Vec3::from_array(transform.translation);
    let rotation = glam::Quat::from_array(transform.rotation);
    let scale = Vec3::from_array(transform.scale);
    Mat4::from_translation(translation) * Mat4::from_quat(rotation) * Mat4::from_scale(scale)
}

fn material_from_scene(scene: &SceneData, node: &SceneNode) -> MaterialUniform {
    let mat_idx = node
        .material_index
        .or_else(|| {
            node.mesh_index
                .and_then(|mi| scene.meshes.get(mi)?.material_index)
        })
        .unwrap_or(0);

    let mat = scene.materials.get(mat_idx).cloned().unwrap_or_default();

    MaterialUniform {
        albedo: mat.base_color,
        roughness: mat.roughness,
        metallic: mat.metallic,
        ao: 1.0,
        emissive_intensity: mat.emissive_intensity,
    }
}

fn create_hdr_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

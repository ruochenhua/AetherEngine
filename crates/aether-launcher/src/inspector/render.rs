//! egui rendering for each inspector target variant.

use super::helpers::{drag_xyz, drag_xyz_raw, rebuild_terrain_material};
use super::InspectorTarget;
use aether_engine::ecs::components::{
    Atmosphere, Camera, Clouds, GodRay, Light, Terrain, Transform, Water,
};
use aether_engine::renderer::renderable::MaterialUniform;

/// Render the inspector UI for the given target.
pub(crate) fn render(ui: &mut egui::Ui, target: &mut InspectorTarget) {
    ui.heading("Inspector");
    ui.separator();

    match target {
        InspectorTarget::Mesh {
            transform,
            material,
            euler,
            ..
        } => {
            render_transform(ui, transform, euler);
            render_material(ui, material);
        }
        InspectorTarget::Light {
            light, direction, ..
        } => render_light(ui, light, direction),
        InspectorTarget::Terrain { terrain, .. } => render_terrain(ui, terrain),
        InspectorTarget::Water { water, .. } => render_water(ui, water),
        InspectorTarget::Atmosphere { atmosphere, .. } => render_atmosphere(ui, atmosphere),
        InspectorTarget::Clouds { clouds, .. } => render_clouds(ui, clouds),
        InspectorTarget::GodRay { god_ray, .. } => render_god_ray(ui, god_ray),
        InspectorTarget::Camera {
            camera,
            fov_degrees,
            ..
        } => render_camera(ui, camera, fov_degrees),
    }
}

fn render_transform(ui: &mut egui::Ui, transform: &mut Transform, euler: &mut [f32; 3]) {
    ui.label("Translation");
    drag_xyz(ui, &mut transform.translation, 0.1);
    ui.label("Rotation (rad)");
    drag_xyz_raw(ui, euler, 0.01);
    ui.label("Scale");
    drag_xyz(ui, &mut transform.scale, 0.05);
    ui.separator();
}

fn render_material(ui: &mut egui::Ui, material: &mut MaterialUniform) {
    ui.label("Material");
    ui.horizontal(|ui| {
        ui.label("R");
        ui.add(
            egui::DragValue::new(&mut material.albedo[0])
                .speed(0.01)
                .range(0.0..=1.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("G");
        ui.add(
            egui::DragValue::new(&mut material.albedo[1])
                .speed(0.01)
                .range(0.0..=1.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("B");
        ui.add(
            egui::DragValue::new(&mut material.albedo[2])
                .speed(0.01)
                .range(0.0..=1.0),
        );
    });
    ui.add(egui::Slider::new(&mut material.roughness, 0.0..=1.0).text("Roughness"));
    ui.add(egui::Slider::new(&mut material.metallic, 0.0..=1.0).text("Metallic"));
    ui.separator();
}

fn render_camera(ui: &mut egui::Ui, camera: &mut Camera, fov_degrees: &mut f32) {
    ui.label("Camera");
    ui.add(egui::Slider::new(fov_degrees, 10.0..=120.0).text("FOV (degrees)"));
    camera.fov = fov_degrees.to_radians();
    ui.add(
        egui::DragValue::new(&mut camera.speed)
            .speed(0.1)
            .range(0.1..=100.0)
            .prefix("Speed: "),
    );
    ui.add(
        egui::DragValue::new(&mut camera.near)
            .speed(0.01)
            .range(0.001..=10.0)
            .prefix("Near: "),
    );
    ui.add(
        egui::DragValue::new(&mut camera.far)
            .speed(1.0)
            .range(10.0..=10000.0)
            .prefix("Far: "),
    );
    ui.separator();
}

fn render_light(ui: &mut egui::Ui, light: &mut Light, direction: &mut [f32; 3]) {
    ui.label("Light");
    ui.horizontal(|ui| {
        ui.label("R");
        ui.add(
            egui::DragValue::new(&mut light.color[0])
                .speed(0.01)
                .range(0.0..=1.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("G");
        ui.add(
            egui::DragValue::new(&mut light.color[1])
                .speed(0.01)
                .range(0.0..=1.0),
        );
    });
    ui.horizontal(|ui| {
        ui.label("B");
        ui.add(
            egui::DragValue::new(&mut light.color[2])
                .speed(0.01)
                .range(0.0..=1.0),
        );
    });
    ui.add(
        egui::DragValue::new(&mut light.intensity)
            .speed(0.1)
            .prefix("Intensity: "),
    );
    ui.label("Direction");
    drag_xyz_raw(ui, direction, 0.01);
    ui.checkbox(&mut light.cast_shadow, "Cast Shadow");
    ui.separator();
}

fn render_terrain(ui: &mut egui::Ui, terrain: &mut Terrain) {
    ui.label("Terrain");
    ui.label("Geometry");
    ui.add(
        egui::DragValue::new(&mut terrain.geometry.extent)
            .speed(1.0)
            .prefix("Extent: "),
    );
    ui.add(
        egui::DragValue::new(&mut terrain.geometry.chunk_size)
            .speed(1.0)
            .prefix("Chunk Size: "),
    );
    ui.add(
        egui::DragValue::new(&mut terrain.geometry.max_lod)
            .speed(1.0)
            .prefix("Max LOD: "),
    );

    match &mut terrain.source {
        aether_engine::scene::TerrainSource::Procedural {
            seed,
            frequency,
            amplitude,
        } => {
            ui.label("Procedural Source");
            ui.add(egui::DragValue::new(seed).speed(1.0).prefix("Seed: "));
            ui.add(
                egui::DragValue::new(frequency)
                    .speed(0.001)
                    .prefix("Frequency: "),
            );
            ui.add(
                egui::DragValue::new(amplitude)
                    .speed(0.1)
                    .prefix("Amplitude: "),
            );
        }
        aether_engine::scene::TerrainSource::Perlin {
            seed,
            frequency,
            amplitude,
            octaves,
            persistence,
            lacunarity,
            exponent,
        } => {
            ui.label("Perlin Source");
            ui.add(egui::DragValue::new(seed).speed(1.0).prefix("Seed: "));
            ui.add(
                egui::DragValue::new(frequency)
                    .speed(0.001)
                    .prefix("Frequency: "),
            );
            ui.add(
                egui::DragValue::new(amplitude)
                    .speed(0.1)
                    .prefix("Amplitude: "),
            );
            ui.add(
                egui::DragValue::new(octaves)
                    .speed(0.1)
                    .range(1..=8)
                    .prefix("Octaves: "),
            );
            ui.add(
                egui::DragValue::new(persistence)
                    .speed(0.01)
                    .range(0.0..=1.0)
                    .prefix("Persistence: "),
            );
            ui.add(
                egui::DragValue::new(lacunarity)
                    .speed(0.01)
                    .range(1.0..=4.0)
                    .prefix("Lacunarity: "),
            );
            ui.add(
                egui::DragValue::new(exponent)
                    .speed(0.01)
                    .range(0.1..=3.0)
                    .prefix("Exponent: "),
            );
        }
        aether_engine::scene::TerrainSource::Heightmap(_) => {
            ui.label("Heightmap source: edit in RON file.");
        }
    }

    ui.label("Layers");
    rebuild_terrain_material(terrain);
    for (i, layer) in terrain.layer_configs.iter_mut().take(4).enumerate() {
        ui.collapsing(format!("Layer {}", i), |ui| {
            ui.horizontal(|ui| {
                ui.label("R");
                ui.add(
                    egui::DragValue::new(&mut layer.albedo[0])
                        .speed(0.01)
                        .range(0.0..=1.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("G");
                ui.add(
                    egui::DragValue::new(&mut layer.albedo[1])
                        .speed(0.01)
                        .range(0.0..=1.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("B");
                ui.add(
                    egui::DragValue::new(&mut layer.albedo[2])
                        .speed(0.01)
                        .range(0.0..=1.0),
                );
            });
            ui.add(egui::Slider::new(&mut layer.roughness, 0.0..=1.0).text("Roughness"));
            ui.add(egui::Slider::new(&mut layer.metallic, 0.0..=1.0).text("Metallic"));
        });
    }
    ui.separator();
}

fn render_water(ui: &mut egui::Ui, water: &mut Water) {
    ui.label("Water");
    let cfg = &mut water.config;
    ui.add(
        egui::DragValue::new(&mut cfg.level)
            .speed(0.1)
            .prefix("Level: "),
    );
    ui.label("Wave Direction");
    ui.horizontal(|ui| {
        ui.label("X");
        ui.add(egui::DragValue::new(&mut cfg.wave_direction[0]).speed(0.01));
    });
    ui.horizontal(|ui| {
        ui.label("Z");
        ui.add(egui::DragValue::new(&mut cfg.wave_direction[1]).speed(0.01));
    });
    ui.add(
        egui::DragValue::new(&mut cfg.wave_amplitude)
            .speed(0.01)
            .prefix("Amplitude: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.wave_wavelength)
            .speed(0.1)
            .prefix("Wavelength: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.wave_speed)
            .speed(0.1)
            .prefix("Speed: "),
    );
    ui.add(egui::Slider::new(&mut cfg.wave_steepness, 0.0..=1.0).text("Steepness"));
    ui.label("Shallow Color");
    drag_xyz_raw(ui, &mut cfg.water_color, 0.01);
    ui.label("Deep Color");
    drag_xyz_raw(ui, &mut cfg.deep_color, 0.01);
    ui.add(egui::Slider::new(&mut cfg.fresnel_power, 0.1..=10.0).text("Fresnel Power"));
    ui.add(
        egui::DragValue::new(&mut cfg.refraction_scale)
            .speed(0.001)
            .prefix("Refraction Scale: "),
    );
    ui.add(egui::Slider::new(&mut cfg.reflectivity, 0.0..=1.0).text("Reflectivity"));
    ui.add(
        egui::DragValue::new(&mut cfg.texture_scale)
            .speed(0.1)
            .prefix("Texture Scale: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.dudv_strength)
            .speed(0.001)
            .prefix("DUDV Strength: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.normal_strength)
            .speed(0.01)
            .prefix("Normal Strength: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.depth_scale)
            .speed(0.001)
            .prefix("Depth Scale: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.specular_power)
            .speed(1.0)
            .prefix("Specular Power: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.secondary_scale)
            .speed(0.01)
            .prefix("Secondary Scale: "),
    );
    ui.label(format!(
        "DUDV map: {}",
        cfg.dudv_map.as_deref().unwrap_or("(none)")
    ));
    ui.label(format!(
        "Normal map: {}",
        cfg.normal_map.as_deref().unwrap_or("(none)")
    ));
    ui.separator();
}

fn render_atmosphere(ui: &mut egui::Ui, atmosphere: &mut Atmosphere) {
    ui.label("Atmosphere");
    let cfg = &mut atmosphere.config;
    ui.label("Sun Direction");
    drag_xyz_raw(ui, &mut cfg.sun_direction, 0.01);
    ui.add(
        egui::DragValue::new(&mut cfg.sun_intensity)
            .speed(0.1)
            .prefix("Sun Intensity: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.mie_asymmetry)
            .speed(0.01)
            .prefix("Mie Asymmetry: "),
    );
    ui.separator();
}

fn render_clouds(ui: &mut egui::Ui, clouds: &mut Clouds) {
    ui.label("Clouds");
    let cfg = &mut clouds.config;
    ui.add(
        egui::DragValue::new(&mut cfg.bottom_altitude)
            .speed(1.0)
            .prefix("Bottom Altitude: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.top_altitude)
            .speed(1.0)
            .prefix("Top Altitude: "),
    );
    ui.add(egui::Slider::new(&mut cfg.coverage, 0.0..=1.0).text("Coverage"));
    ui.add(
        egui::DragValue::new(&mut cfg.density)
            .speed(0.1)
            .prefix("Density: "),
    );
    ui.label("Wind Direction");
    ui.horizontal(|ui| {
        ui.label("X");
        ui.add(egui::DragValue::new(&mut cfg.wind_direction[0]).speed(0.01));
    });
    ui.horizontal(|ui| {
        ui.label("Z");
        ui.add(egui::DragValue::new(&mut cfg.wind_direction[1]).speed(0.01));
    });
    ui.add(
        egui::DragValue::new(&mut cfg.wind_speed)
            .speed(0.1)
            .prefix("Wind Speed: "),
    );
    ui.separator();
}

fn render_god_ray(ui: &mut egui::Ui, god_ray: &mut GodRay) {
    ui.label("God Rays");
    let cfg = &mut god_ray.config;
    ui.add(
        egui::DragValue::new(&mut cfg.samples)
            .speed(1.0)
            .prefix("Samples: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.density)
            .speed(0.01)
            .prefix("Density: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.decay)
            .speed(0.001)
            .prefix("Decay: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.weight)
            .speed(0.01)
            .prefix("Weight: "),
    );
    ui.add(
        egui::DragValue::new(&mut cfg.exposure)
            .speed(0.01)
            .prefix("Exposure: "),
    );
    ui.separator();
}

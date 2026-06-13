//! Small helpers shared by the inspector UI.

use aether_engine::ecs::components::Terrain;
use glam::{Quat, Vec3};

/// Convert a light direction vector into the Transform.rotation used by the
/// renderer. The convention is that the default light direction is -Y.
pub(crate) fn light_direction_to_rotation(direction: Vec3) -> Quat {
    let d = direction.normalize();
    if d.abs_diff_eq(Vec3::NEG_Y, 1e-6) {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_arc(Vec3::NEG_Y, d)
    }
}

/// Extract the light direction vector from a Transform.rotation.
pub(crate) fn light_rotation_to_direction(rotation: Quat) -> Vec3 {
    (rotation * Vec3::NEG_Y).normalize()
}

/// Rebuild the runtime TerrainMaterial from the editable layer configs so that
/// layer color/roughness/metallic edits show up immediately.
pub(crate) fn rebuild_terrain_material(terrain: &mut Terrain) {
    for (i, cfg) in terrain.layer_configs.iter().take(4).enumerate() {
        terrain.material.layers[i].albedo = cfg.albedo;
        terrain.material.layers[i].roughness = cfg.roughness;
        terrain.material.layers[i].metallic = cfg.metallic;
    }
}

/// Drag a Vec3 with per-axis X/Y/Z labels.
pub(crate) fn drag_xyz(ui: &mut egui::Ui, v: &mut Vec3, speed: f32) {
    ui.horizontal(|ui| {
        ui.label("X");
        ui.add(egui::DragValue::new(&mut v.x).speed(speed));
    });
    ui.horizontal(|ui| {
        ui.label("Y");
        ui.add(egui::DragValue::new(&mut v.y).speed(speed));
    });
    ui.horizontal(|ui| {
        ui.label("Z");
        ui.add(egui::DragValue::new(&mut v.z).speed(speed));
    });
}

/// Drag a raw [f32; 3] with per-axis X/Y/Z labels.
pub(crate) fn drag_xyz_raw(ui: &mut egui::Ui, v: &mut [f32; 3], speed: f32) {
    ui.horizontal(|ui| {
        ui.label("X");
        ui.add(egui::DragValue::new(&mut v[0]).speed(speed));
    });
    ui.horizontal(|ui| {
        ui.label("Y");
        ui.add(egui::DragValue::new(&mut v[1]).speed(speed));
    });
    ui.horizontal(|ui| {
        ui.label("Z");
        ui.add(egui::DragValue::new(&mut v[2]).speed(speed));
    });
}

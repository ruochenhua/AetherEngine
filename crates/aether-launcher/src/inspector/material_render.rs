//! egui controls for the editable material source configuration.

use aether_engine::scene::MaterialConfig;

pub(crate) fn render_material(ui: &mut egui::Ui, material: &mut MaterialConfig) {
    ui.label("Material");
    render_material_channel(ui, "R", &mut material.albedo[0]);
    render_material_channel(ui, "G", &mut material.albedo[1]);
    render_material_channel(ui, "B", &mut material.albedo[2]);
    ui.add(egui::Slider::new(&mut material.roughness, 0.0..=1.0).text("Roughness"));
    ui.add(egui::Slider::new(&mut material.metallic, 0.0..=1.0).text("Metallic"));
    ui.add(egui::Slider::new(&mut material.normal_scale, 0.0..=2.0).text("Normal Scale"));
    ui.add(
        egui::Slider::new(&mut material.occlusion_strength, 0.0..=1.0).text("Occlusion Strength"),
    );
    ui.add(
        egui::DragValue::new(&mut material.emissive_intensity)
            .speed(0.05)
            .range(0.0..=100.0)
            .prefix("Emissive Intensity: "),
    );
    ui.checkbox(&mut material.unlit, "Unlit");
    render_material_path(ui, "Albedo Texture", &mut material.albedo_texture);
    render_material_path(ui, "Normal Texture", &mut material.normal_texture);
    render_material_path(ui, "ORM Texture", &mut material.orm_texture);
    render_material_path(ui, "Emissive Texture", &mut material.emissive_texture);
    ui.separator();
}

fn render_material_channel(ui: &mut egui::Ui, label: &str, channel: &mut f32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(channel).speed(0.01).range(0.0..=1.0));
    });
}

fn render_material_path(ui: &mut egui::Ui, label: &str, path: &mut Option<String>) {
    let mut value = path.clone().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(&mut value).changed() {
            *path = (!value.trim().is_empty()).then_some(value.clone());
        }
    });
}

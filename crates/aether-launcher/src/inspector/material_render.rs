//! egui controls for the editable material source configuration.

use aether_engine::scene::MaterialConfig;

pub(crate) fn render_material(
    ui: &mut egui::Ui,
    material: &mut MaterialConfig,
    entity: aether_engine::ecs::Entity,
    read_only: bool,
) -> Option<crate::app::workspace::MaterialDropRequest> {
    use aether_engine::scene::material::TextureUsage;
    let mut pending = None;
    ui.add_enabled_ui(!read_only, |ui| {
        ui.label("Material");
        render_material_channel(ui, "R", &mut material.albedo[0]);
        render_material_channel(ui, "G", &mut material.albedo[1]);
        render_material_channel(ui, "B", &mut material.albedo[2]);
        ui.add(egui::Slider::new(&mut material.roughness, 0.0..=1.0).text("Roughness"));
        ui.add(egui::Slider::new(&mut material.metallic, 0.0..=1.0).text("Metallic"));
        ui.add(egui::Slider::new(&mut material.normal_scale, 0.0..=2.0).text("Normal Scale"));
        ui.add(
            egui::Slider::new(&mut material.occlusion_strength, 0.0..=1.0)
                .text("Occlusion Strength"),
        );
        ui.add(
            egui::DragValue::new(&mut material.emissive_intensity)
                .speed(0.05)
                .range(0.0..=100.0)
                .prefix("Emissive Intensity: "),
        );
        ui.checkbox(&mut material.unlit, "Unlit");
    });
    for (label, path, slot) in [
        (
            "Albedo Texture",
            &mut material.albedo_texture,
            TextureUsage::Albedo,
        ),
        (
            "Normal Texture",
            &mut material.normal_texture,
            TextureUsage::Normal,
        ),
        ("ORM Texture", &mut material.orm_texture, TextureUsage::Orm),
        (
            "Emissive Texture",
            &mut material.emissive_texture,
            TextureUsage::Emissive,
        ),
    ] {
        let response = render_material_path(ui, label, path, read_only);
        let request = super::asset_drop::capture(
            ui,
            &response,
            entity,
            crate::app::workspace::MaterialDropTarget::Texture(slot),
            read_only,
        );
        if pending.is_none() {
            pending = request;
        }
    }
    ui.separator();
    pending
}

fn render_material_channel(ui: &mut egui::Ui, label: &str, channel: &mut f32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(channel).speed(0.01).range(0.0..=1.0));
    });
}

fn render_material_path(
    ui: &mut egui::Ui,
    label: &str,
    path: &mut Option<String>,
    read_only: bool,
) -> egui::Response {
    let mut value = path.clone().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(label);
        if ui
            .add_enabled(!read_only, egui::TextEdit::singleline(&mut value))
            .changed()
        {
            *path = (!value.trim().is_empty()).then_some(value.clone());
        }
    })
    .response
}

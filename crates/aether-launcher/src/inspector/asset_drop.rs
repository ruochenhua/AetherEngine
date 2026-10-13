//! egui drop capture only; filesystem and World work belongs to the host.
use crate::{
    app::workspace::{AssetDragPayload, MaterialDropRequest, MaterialDropTarget},
    project_index::ResourceKind,
};
use aether_engine::ecs::Entity;

pub(super) fn capture(
    ui: &egui::Ui,
    response: &egui::Response,
    entity: Entity,
    target: MaterialDropTarget,
    read_only: bool,
) -> Option<MaterialDropRequest> {
    let accepts = |resource: &AssetDragPayload| {
        !read_only
            && match target {
                MaterialDropTarget::Asset => resource.kind == ResourceKind::Material,
                MaterialDropTarget::Texture(_) => resource.kind == ResourceKind::Image,
            }
    };
    if let Some(payload) = response.dnd_hover_payload::<AssetDragPayload>() {
        let message = if read_only {
            "Linked material: use Detach and edit first"
        } else if accepts(&payload) {
            "Release to assign resource"
        } else if target == MaterialDropTarget::Asset {
            "Material Asset accepts Material resources only"
        } else {
            "Texture slots accept Image resources only"
        };
        ui.painter().rect_stroke(
            response.rect,
            2.0,
            egui::Stroke::new(
                1.0,
                if accepts(&payload) {
                    egui::Color32::LIGHT_GREEN
                } else {
                    egui::Color32::LIGHT_RED
                },
            ),
            egui::StrokeKind::Inside,
        );
        response.show_tooltip_text(message);
    }
    let resource = response.dnd_release_payload::<AssetDragPayload>()?;
    accepts(&resource).then(|| MaterialDropRequest {
        entity,
        target,
        resource: (*resource).clone(),
    })
}

pub(super) fn render_mesh(
    ui: &mut egui::Ui,
    target: &mut super::InspectorTarget,
) -> Option<MaterialDropRequest> {
    let super::InspectorTarget::Mesh {
        entity,
        material,
        material_asset,
        material_asset_error,
        detach_material,
        ..
    } = target
    else {
        return None;
    };
    let response = ui
        .horizontal(|ui| {
            ui.label("Material Asset");
            ui.label(
                material_asset
                    .as_deref()
                    .unwrap_or("Drop a Material resource to link"),
            );
        })
        .response;
    let mut pending = capture(ui, &response, *entity, MaterialDropTarget::Asset, false);
    if material_asset.is_some() {
        if let Some(error) = material_asset_error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        } else {
            ui.label("Watching for file changes");
        }
        if !*detach_material && ui.button("Detach and edit").clicked() {
            *detach_material = true;
        }
    }
    let read_only = material_asset.is_some() && !*detach_material;
    let texture_drop = super::material_render::render_material(ui, material, *entity, read_only);
    if pending.is_none() {
        pending = texture_drop;
    }
    pending
}

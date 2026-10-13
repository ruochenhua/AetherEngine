pub(super) fn capture_viewport_drop(
    ui: &mut egui::Ui,
) -> Option<super::super::workspace::ViewportDropRequest> {
    let response = ui.allocate_rect(ui.max_rect(), egui::Sense::hover());
    if let Some(payload) = response.dnd_hover_payload::<super::super::workspace::AssetDragPayload>()
    {
        ui.painter().text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            if payload.kind == crate::project_index::ResourceKind::Obj {
                "Release to place model"
            } else {
                "Viewport accepts OBJ models only"
            },
            egui::FontId::proportional(18.0),
            ui.visuals().text_color(),
        );
    }
    let payload = response.dnd_release_payload::<super::super::workspace::AssetDragPayload>()?;
    if payload.kind != crate::project_index::ResourceKind::Obj {
        return None;
    }
    let pointer = ui.ctx().input(|input| input.pointer.interact_pos())?;
    let scale = ui.ctx().pixels_per_point();
    Some(super::super::workspace::ViewportDropRequest {
        relative_path: payload.relative_path.clone(),
        pointer_physical: [pointer.x * scale, pointer.y * scale],
        viewport_points_min: [response.rect.min.x, response.rect.min.y],
        viewport_points_max: [response.rect.max.x, response.rect.max.y],
        pixels_per_point: scale,
    })
}

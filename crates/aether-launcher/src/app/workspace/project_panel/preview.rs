use super::helpers::resource_icon;
use crate::{
    project_index::ResourceKind,
    project_preview::{PreviewState, PreviewWorker, Thumbnail},
};
use std::{path::PathBuf, sync::Arc};

pub(super) struct ResourcePreview {
    worker: PreviewWorker,
    texture: Option<(Arc<Thumbnail>, egui::TextureHandle)>,
}

impl ResourcePreview {
    pub fn new(root: PathBuf) -> Self {
        Self {
            worker: PreviewWorker::new(root),
            texture: None,
        }
    }
    pub fn invalidate(&mut self) {
        self.worker.invalidate();
        self.texture = None;
    }
    pub fn poll(&mut self) {
        self.worker.poll();
    }
    pub fn select(&mut self, selection: Option<(&str, ResourceKind)>) {
        let selection = selection.filter(|(_, kind)| {
            matches!(
                kind,
                ResourceKind::Image | ResourceKind::Obj | ResourceKind::Gltf
            )
        });
        self.worker.select(selection);
        if !matches!(self.worker.state(), PreviewState::Ready(_)) {
            self.texture = None;
        }
    }
    pub fn render(&mut self, ui: &mut egui::Ui, kind: ResourceKind) {
        let edge = (ui.available_height() * 0.3).clamp(32., 96.);
        match self.worker.state() {
            PreviewState::Idle => {}
            PreviewState::Loading => {
                placeholder(ui, kind, edge, "Loading preview…");
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
            PreviewState::Unavailable(reason) => {
                placeholder(ui, kind, edge, &format!("Preview unavailable: {reason}"))
            }
            PreviewState::Ready(thumbnail) => {
                if self
                    .texture
                    .as_ref()
                    .map_or(true, |(loaded, _)| !Arc::ptr_eq(loaded, thumbnail))
                {
                    let image =
                        egui::ColorImage::from_rgba_unmultiplied(thumbnail.size, &thumbnail.rgba);
                    self.texture = Some((
                        thumbnail.clone(),
                        ui.ctx().load_texture(
                            "selected-project-thumbnail",
                            image,
                            egui::TextureOptions::LINEAR,
                        ),
                    ));
                }
                if let Some((_, texture)) = &self.texture {
                    ui.horizontal(|ui| {
                        let size = egui::vec2(thumbnail.size[0] as f32, thumbnail.size[1] as f32);
                        let fit = (edge / size.x).min(edge / size.y).min(1.);
                        ui.add(egui::Image::new(texture).fit_to_exact_size(size * fit));
                        ui.vertical(|ui| {
                            ui.label(&thumbnail.dimensions);
                            if matches!(kind, ResourceKind::Obj | ResourceKind::Gltf) {
                                ui.small("Geometry preview · fixed angle · flat shading");
                            }
                        });
                    });
                }
            }
        }
    }
}

fn placeholder(ui: &mut egui::Ui, kind: ResourceKind, edge: f32, message: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(edge, edge), egui::Sense::hover());
        let (icon, color) = resource_icon(kind, ui);
        ui.painter()
            .rect_filled(rect, 4., ui.visuals().extreme_bg_color);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(24.),
            color,
        );
        ui.add(egui::Label::new(message).wrap());
    });
}

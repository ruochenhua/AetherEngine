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
    pub fn retained_requests(&self) -> Vec<(String, ResourceKind)> {
        self.worker.retained_requests()
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
        let available = ui.available_size();
        let max_width = (available.x - 12.).clamp(96., 520.);
        let max_height = (available.y - 12.).clamp(96., 520.);
        match self.worker.state() {
            PreviewState::Idle => {}
            PreviewState::Loading => {
                placeholder(
                    ui,
                    kind,
                    max_width.min(max_height).min(280.),
                    "Loading preview…",
                );
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
            PreviewState::Unavailable(reason) => placeholder(
                ui,
                kind,
                max_width.min(max_height).min(280.),
                &format!("Preview unavailable: {reason}"),
            ),
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
                    ui.label(&thumbnail.dimensions);
                    if matches!(kind, ResourceKind::Obj | ResourceKind::Gltf) {
                        ui.small("Geometry preview · fixed angle · flat shading");
                    }
                    let size = fit_size(thumbnail.size, max_width, max_height);
                    ui.add(egui::Image::new(texture).fit_to_exact_size(size));
                }
            }
        }
    }
}

fn fit_size(size: [usize; 2], max_width: f32, max_height: f32) -> egui::Vec2 {
    if size[0] == 0 || size[1] == 0 {
        return egui::Vec2::ZERO;
    }
    let original = egui::vec2(size[0] as f32, size[1] as f32);
    let scale = (max_width / original.x).min(max_height / original.y);
    original * scale
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_scales_to_available_details_space_and_preserves_aspect_ratio() {
        let wide = fit_size([384, 96], 520., 300.);
        assert_eq!(wide, egui::vec2(520., 130.));
        let square = fit_size([384, 384], 520., 300.);
        assert_eq!(square, egui::vec2(300., 300.));
        assert_eq!(fit_size([0, 10], 520., 300.), egui::Vec2::ZERO);
    }
}

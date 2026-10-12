use crate::project_index::{IndexSnapshot, ProjectSort, ResourceKind, ResourceValidity};
use std::path::Path;

pub(super) fn ui_state_empty(ui: &mut egui::Ui, snapshot: &IndexSnapshot, directory: &str) {
    if !snapshot.missing_roots.is_empty() && directory.is_empty() {
        ui.label(format!(
            "Empty project · missing {}",
            snapshot.missing_roots.join(", ")
        ));
    } else {
        ui.label("No resources in this folder");
    }
}

pub(super) fn current_prefix(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    }
}

pub(super) fn child_directories(snapshot: &IndexSnapshot, parent: &str) -> Vec<String> {
    let prefix = current_prefix(parent);
    let mut children = snapshot
        .entries
        .iter()
        .filter_map(|entry| {
            let rest = entry.path.strip_prefix(&prefix)?;
            let segment = rest.split('/').next()?;
            (rest.contains('/')).then(|| {
                if prefix.is_empty() {
                    segment.to_owned()
                } else {
                    format!("{prefix}{segment}")
                }
            })
        })
        .collect::<Vec<_>>();
    if parent.is_empty() {
        for root in ["assets", "scenes"] {
            children.push(root.to_owned());
        }
    }
    children.sort();
    children.dedup();
    children
}

pub(super) fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(super) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(super) fn resource_icon(kind: ResourceKind, ui: &egui::Ui) -> (&'static str, egui::Color32) {
    let color = match kind {
        ResourceKind::Scene => egui::Color32::from_rgb(110, 190, 255),
        ResourceKind::Prefab => egui::Color32::from_rgb(190, 150, 255),
        ResourceKind::Material => egui::Color32::from_rgb(255, 190, 100),
        ResourceKind::Gltf | ResourceKind::Obj => egui::Color32::from_rgb(130, 220, 170),
        ResourceKind::Image => egui::Color32::from_rgb(255, 145, 165),
        ResourceKind::Unknown => ui.visuals().weak_text_color(),
    };
    let icon = match kind {
        ResourceKind::Scene => "◈",
        ResourceKind::Prefab => "◇",
        ResourceKind::Material => "●",
        ResourceKind::Gltf | ResourceKind::Obj => "△",
        ResourceKind::Image => "▧",
        ResourceKind::Unknown => "·",
    };
    (icon, color)
}

pub(super) fn kind_name(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Scene => "Scene",
        ResourceKind::Prefab => "Prefab",
        ResourceKind::Material => "Material",
        ResourceKind::Gltf => "glTF",
        ResourceKind::Obj => "OBJ",
        ResourceKind::Image => "Image",
        ResourceKind::Unknown => "Unknown",
    }
}

pub(super) fn validity_name(validity: ResourceValidity) -> &'static str {
    match validity {
        ResourceValidity::Valid => "Valid",
        ResourceValidity::Invalid => "Invalid",
        ResourceValidity::Unknown => "Unknown",
    }
}

pub(super) fn sort_name(sort: ProjectSort) -> &'static str {
    match sort {
        ProjectSort::Name => "Name",
        ProjectSort::Path => "Path",
        ProjectSort::Kind => "Kind",
    }
}

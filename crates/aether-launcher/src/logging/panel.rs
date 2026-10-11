//! UI consumer and resource validation. Filesystem checks never run in tracing callbacks.
use super::{CapturedEvent, Health, LogStore, LogView, UI_QUEUE_CAPACITY};
use std::{
    path::{Component, Path, PathBuf},
    sync::{mpsc::Receiver, Arc},
};

pub(crate) struct LogPanel {
    pub store: LogStore,
    pub view: LogView,
    receiver: Option<Receiver<CapturedEvent>>,
    pub ui_health: Arc<Health>,
    pub file_health: Arc<Health>,
    root: Option<PathBuf>,
    selected: Option<CapturedEvent>,
}
impl Default for LogPanel {
    fn default() -> Self {
        Self::new(
            None,
            Default::default(),
            Default::default(),
            std::env::current_dir().unwrap_or_default(),
        )
    }
}
impl LogPanel {
    pub fn new(
        receiver: Option<Receiver<CapturedEvent>>,
        ui_health: Arc<Health>,
        file_health: Arc<Health>,
        root: PathBuf,
    ) -> Self {
        Self {
            store: LogStore::new(super::UI_HISTORY_CAPACITY),
            view: LogView::default(),
            receiver,
            ui_health,
            file_health,
            root: root.canonicalize().ok(),
            selected: None,
        }
    }
    pub fn drain(&mut self) {
        for _ in 0..UI_QUEUE_CAPACITY {
            let event = self
                .receiver
                .as_ref()
                .and_then(|receiver| receiver.try_recv().ok());
            let Some(mut event) = event else {
                break;
            };
            event.resource = self.resource(&event);
            self.store.push(event);
        }
    }
    pub fn resource(&self, event: &CapturedEvent) -> Option<PathBuf> {
        ["asset", "path"].iter().find_map(|key| {
            event
                .fields
                .get(*key)
                .and_then(|value| value.as_str())
                .and_then(|value| self.validate_resource(Path::new(value)))
        })
    }
    pub fn validate_resource(&self, path: &Path) -> Option<PathBuf> {
        // Reject foreign-platform absolute paths too, regardless of the host OS.
        let text = path.to_str()?;
        if text.contains(':') {
            return None;
        }
        let normalized = text.replace('\\', "/");
        let path = Path::new(&normalized);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return None;
        }
        let root = self.root.as_ref()?;
        let resolved = root.join(path).canonicalize().ok()?;
        if resolved.starts_with(root) && resolved.is_file() {
            Some(path.to_path_buf())
        } else {
            None
        }
    }
    pub fn render(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let mut resource = None;
        ui.horizontal(|ui| {
            if ui.button("Clear").clicked() {
                self.store.clear();
                self.selected = None;
            }
            ui.checkbox(&mut self.view.paused, "Pause auto-scroll");
            egui::ComboBox::from_id_salt("log_level")
                .selected_text(["ERROR", "WARN", "INFO", "DEBUG", "TRACE"][self.view.filter.level])
                .show_ui(ui, |ui| {
                    for (index, label) in ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut self.view.filter.level, index, *label);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.view.filter.category)
                    .hint_text("Category / target")
                    .desired_width(130.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.view.filter.text)
                    .hint_text("Filter text / fields")
                    .desired_width(180.0),
            );
        });
        for (name, health) in [("UI", &self.ui_health), ("File", &self.file_health)] {
            if health.degraded() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "{name} degraded — dropped ERROR/WARN/INFO/DEBUG/TRACE: {:?}",
                        health.dropped()
                    ),
                );
                if let Some(error) = health.error() {
                    ui.label(error);
                }
            }
        }
        if let Some(event) = self.selected.as_ref() {
            egui::CollapsingHeader::new("Event details")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(format!(
                        "{} ms since Unix epoch · {} · {}",
                        event.timestamp_ms, event.level, event.target
                    ));
                    for (name, value) in &event.fields {
                        ui.label(format!("{name}: {value}"));
                    }
                    if let Some(path) = self.resource(event) {
                        if ui
                            .link(format!("Show in Project: {}", path.display()))
                            .clicked()
                        {
                            resource = self.validate_resource(&path);
                        }
                    }
                });
        }
        let visible: Vec<_> = self.store.visible(&self.view.filter).collect();
        if visible.is_empty() {
            ui.label("No log messages yet.");
        }
        egui::ScrollArea::vertical()
            .stick_to_bottom(self.view.auto_scroll())
            .show_rows(ui, 18.0, visible.len(), |ui, rows| {
                for index in rows {
                    let event = visible[index];
                    let label = format!(
                        "{} [{}] {}: {}",
                        event.timestamp_ms, event.level, event.target, event.message
                    );
                    if ui.selectable_label(false, label).clicked() {
                        self.selected = Some(event.clone());
                    }
                }
            });
        resource
    }
}

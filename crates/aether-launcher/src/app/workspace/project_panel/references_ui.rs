use super::ProjectPanel;
use crate::project_dependencies::Resolution;

fn focus_id(ui: &egui::Ui) -> egui::Id {
    ui.make_persistent_id("reference-preview-focus")
}
impl ProjectPanel {
    pub(super) fn focus_references(&self, ui: &egui::Ui) {
        ui.memory_mut(|memory| memory.request_focus(focus_id(ui)));
    }
    pub(super) fn render_references(&mut self, ui: &mut egui::Ui) {
        let Some(target) = self.references.target().map(str::to_owned) else {
            return;
        };
        ui.separator();
        let heading = ui.label(format!("References: {target}"));
        // A dedicated, non-text focus owner prevents Escape leaking from search or other panels.
        let focus = ui.interact(heading.rect, focus_id(ui), egui::Sense::click());
        if focus.clicked() {
            focus.request_focus();
        }
        // egui surrenders focus on Escape before widgets render. Accept that loss only
        // for this preview's own focus id, with no other control taking focus.
        let owns_focus = focus.has_focus()
            || (focus.lost_focus() && ui.memory(|memory| memory.focused().is_none()));
        if ui.input(|input| input.key_pressed(egui::Key::Escape))
            && self.references.escape(owns_focus, false)
        {
            ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.strong(self.references.status());
            if ui.button("Back to target").clicked() {
                self.back_to_reference_target();
                // Use the parent UI's id, shared with the focus response above.
                focus.request_focus();
            }
            if ui
                .add_enabled(!self.references.busy(), egui::Button::new("Reanalyze"))
                .clicked()
            {
                self.references.reanalyze();
                focus.request_focus();
            }
            if ui
                .add_enabled(
                    !self.references.busy() && self.references.report().is_some(),
                    egui::Button::new("Check for changes"),
                )
                .clicked()
            {
                self.references.recheck();
                focus.request_focus();
            }
            if ui.button("Close references").clicked() {
                self.references.close();
            }
        });
        if self.references.target().is_none() {
            return;
        }
        ui.small("Read-only snapshot of authored references");
        if self.references.busy() {
            ui.spinner();
            if self.references.report().is_some() {
                ui.label("Checking fingerprints…");
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if let Some(error) = self.references.error() {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        match self.references.status() {
            "Stale" => {
                ui.label("Project content or inventory changed. Reanalyze for a current snapshot.");
            }
            "Incomplete" => {
                ui.label(
                    "Incomplete result: blocking diagnostics prevent a complete reference report.",
                );
            }
            "Empty" => {
                ui.label("No authored references found in this complete snapshot.");
            }
            _ => {}
        }
        let Some(report) = self.references.report() else {
            return;
        };
        // Clone only the visible report data; the worker retains the fingerprint snapshot in an Arc.
        let inbound = report.used_by().cloned().collect::<Vec<_>>();
        let outbound = report.depends_on().cloned().collect::<Vec<_>>();
        let diagnostics = report.diagnostics.clone();
        egui::ScrollArea::vertical()
            .id_salt("project_references")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (heading, edges) in [("Used by", inbound), ("Depends on", outbound)] {
                    ui.heading(heading);
                    if edges.is_empty() {
                        ui.label("No rows in this snapshot");
                    }
                    for (index, edge) in edges.iter().enumerate() {
                        ui.push_id((heading, index), |ui| {
                            let navigable = self.references.source_navigable(edge);
                            let reason = match &edge.resolution {
                                Resolution::Resolved if navigable => "Resolved".to_owned(),
                                Resolution::Resolved => {
                                    "Source is not a safe file in this snapshot".to_owned()
                                }
                                Resolution::Missing => {
                                    format!("Missing target: {}", edge.candidate)
                                }
                                Resolution::Unsafe(reason) => {
                                    format!("Unsafe/non-file reference: {reason}")
                                }
                            };
                            let response =
                                ui.add_enabled(navigable, egui::Button::new(&edge.source));
                            if response.clicked() {
                                self.select_reference_source(edge);
                                focus.request_focus();
                            }
                            ui.label(format!("{:?} · {:?}", edge.locator, edge.kind));
                            ui.label(format!("{} → {} · {reason}", edge.authored, edge.candidate));
                            ui.separator();
                        });
                    }
                }
                if !diagnostics.is_empty() {
                    ui.heading("Blocking diagnostics · incomplete result");
                    for diagnostic in diagnostics {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!("{}: {}", diagnostic.path, diagnostic.message),
                        );
                    }
                }
            });
    }
}

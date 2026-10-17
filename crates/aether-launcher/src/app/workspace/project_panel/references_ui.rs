use super::ProjectPanel;
use crate::project_dependencies::Resolution;

fn focus_id(ui: &egui::Ui) -> egui::Id {
    ui.make_persistent_id("reference-preview-focus")
}
// Escape clears egui focus before widgets render. Only a response belonging to
// this preview may claim that focus loss; search/text controls are excluded.
fn owns_focus(response: &egui::Response) -> bool {
    response.has_focus()
        || (response.lost_focus() && response.ctx.memory(|memory| memory.focused().is_none()))
}
impl ProjectPanel {
    pub(super) fn focus_references(&self, ui: &egui::Ui) {
        ui.memory_mut(|memory| memory.request_focus(focus_id(ui)));
    }
    pub(super) fn render_references(&mut self, ui: &mut egui::Ui) {
        let preview_owns_focus = self.render_reference_contents(ui);
        if ui.input(|input| input.key_pressed(egui::Key::Escape))
            && self.references.escape(preview_owns_focus, false)
        {
            ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
        }
    }
    fn render_reference_contents(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(target) = self.references.target().map(str::to_owned) else {
            return false;
        };
        ui.separator();
        let heading = ui.label(format!("References: {target}"));
        // A dedicated, non-text focus owner prevents Escape leaking from search or other panels.
        let focus = ui.interact(heading.rect, focus_id(ui), egui::Sense::click());
        if focus.clicked() {
            focus.request_focus();
        }
        let mut preview_owns_focus = owns_focus(&focus);
        ui.horizontal_wrapped(|ui| {
            ui.strong(self.references.status());
            let back = ui.button("Back to target");
            preview_owns_focus |= owns_focus(&back);
            if back.clicked() {
                self.back_to_reference_target();
                // Use the parent UI's id, shared with the focus response above.
                focus.request_focus();
            }
            let reanalyze = ui.add_enabled(!self.references.busy(), egui::Button::new("Reanalyze"));
            preview_owns_focus |= owns_focus(&reanalyze);
            if reanalyze.clicked() {
                self.references.reanalyze();
                focus.request_focus();
            }
            let recheck = ui.add_enabled(
                !self.references.busy() && self.references.report().is_some(),
                egui::Button::new("Check for changes"),
            );
            preview_owns_focus |= owns_focus(&recheck);
            if recheck.clicked() {
                self.references.recheck();
                focus.request_focus();
            }
            let close = ui.button("Close references");
            preview_owns_focus |= owns_focus(&close);
            if close.clicked() {
                self.references.close();
            }
        });
        if self.references.target().is_none() {
            return preview_owns_focus;
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
            return preview_owns_focus;
        };
        // Clone section rows and diagnostics while Project navigation is mutable; the snapshot remains in an Arc.
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
                            preview_owns_focus |= owns_focus(&response);
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
        preview_owns_focus
    }
}

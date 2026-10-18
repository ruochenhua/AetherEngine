use crate::app::{
    document_state::{DocumentIdentity, SceneDocumentState},
    scene_transition::{PendingSceneTransition, SceneTransitionChoice},
};

#[derive(Default)]
pub(super) struct DialogOutput {
    pub choice: Option<SceneTransitionChoice>,
    pub cancel_has_focus: bool,
}

pub(super) fn render(
    ui: &mut egui::Ui,
    document: &SceneDocumentState,
    transition: Option<&PendingSceneTransition>,
    focus_cancel: bool,
) -> DialogOutput {
    let Some(transition) = transition.filter(|transition| transition.choice.is_none()) else {
        return DialogOutput::default();
    };
    let document_name = match document.identity() {
        DocumentIdentity::Untitled => "Untitled".to_owned(),
        DocumentIdentity::Saved(path) => path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    };
    let requested_action = transition.prompt();
    let mut output = DialogOutput::default();
    egui::Modal::new(egui::Id::new("scene_transition_confirmation")).show(ui.ctx(), |ui| {
        ui.set_min_width(360.0);
        ui.heading(format!(
            "Save changes to ‘{document_name}’ before continuing?"
        ));
        ui.label(format!("Requested action: {requested_action}"));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                output.choice = Some(SceneTransitionChoice::Save);
            }
            if ui.button("Discard").clicked() {
                output.choice = Some(SceneTransitionChoice::Discard);
            }
            let cancel = ui.button("Cancel");
            if focus_cancel {
                cancel.request_focus();
            }
            output.cancel_has_focus = cancel.has_focus();
            if cancel.clicked() {
                output.choice = Some(SceneTransitionChoice::Cancel);
            }
        });
    });
    if ui
        .ctx()
        .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        output.choice = Some(SceneTransitionChoice::Cancel);
    }
    output
}

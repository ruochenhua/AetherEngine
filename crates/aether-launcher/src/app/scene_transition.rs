use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SceneTransitionTarget {
    NewScene,
    OpenDialog,
    OpenPath(PathBuf),
    CloseApplication,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneTransitionChoice {
    Save,
    Discard,
    Cancel,
    Proceed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingSceneTransition {
    pub target: SceneTransitionTarget,
    pub choice: Option<SceneTransitionChoice>,
}

impl PendingSceneTransition {
    pub(crate) fn new(target: SceneTransitionTarget, is_dirty: bool) -> Self {
        Self {
            target,
            choice: (!is_dirty).then_some(SceneTransitionChoice::Proceed),
        }
    }

    pub(crate) fn prompt(&self) -> String {
        match &self.target {
            SceneTransitionTarget::NewScene => "Create a new scene".into(),
            SceneTransitionTarget::OpenDialog => "Open another scene".into(),
            SceneTransitionTarget::OpenPath(path) => {
                format!("Open {}", path.display())
            }
            SceneTransitionTarget::CloseApplication => "Close Aether Engine".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_documents_proceed_without_a_decision_prompt() {
        let transition = PendingSceneTransition::new(SceneTransitionTarget::NewScene, false);
        assert_eq!(transition.choice, Some(SceneTransitionChoice::Proceed));
    }

    #[test]
    fn dirty_documents_wait_for_an_explicit_choice_and_keep_the_target_visible() {
        let path = PathBuf::from("scenes/next.ron");
        let transition =
            PendingSceneTransition::new(SceneTransitionTarget::OpenPath(path.clone()), true);
        assert_eq!(transition.choice, None);
        assert_eq!(transition.prompt(), format!("Open {}", path.display()));
    }

    #[test]
    fn explicit_choices_are_stored_with_the_requested_transition() {
        let mut transition =
            PendingSceneTransition::new(SceneTransitionTarget::CloseApplication, true);
        transition.choice = Some(SceneTransitionChoice::Cancel);
        assert_eq!(transition.choice, Some(SceneTransitionChoice::Cancel));
    }
}

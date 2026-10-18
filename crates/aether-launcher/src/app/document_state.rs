use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DocumentIdentity {
    Untitled,
    Saved(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentStatus {
    Clean,
    Dirty,
    Unknown,
    RecoveryRequired,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneDocumentState {
    identity: DocumentIdentity,
    clean_snapshot: Option<String>,
    current_snapshot: Option<String>,
    unavailable: Option<DocumentStatus>,
}

impl SceneDocumentState {
    pub(crate) fn uninitialized() -> Self {
        Self {
            identity: DocumentIdentity::Untitled,
            clean_snapshot: None,
            current_snapshot: None,
            unavailable: Some(DocumentStatus::Unknown),
        }
    }

    pub(crate) fn new_untitled(snapshot: String) -> Self {
        Self {
            identity: DocumentIdentity::Untitled,
            clean_snapshot: Some(snapshot.clone()),
            current_snapshot: Some(snapshot),
            unavailable: None,
        }
    }

    pub(crate) fn opened(path: PathBuf, snapshot: String) -> Self {
        let mut state = Self::new_untitled(snapshot);
        state.identity = DocumentIdentity::Saved(path);
        state
    }

    pub(crate) fn identity(&self) -> &DocumentIdentity {
        &self.identity
    }

    pub(crate) fn status(&self) -> DocumentStatus {
        self.unavailable.unwrap_or_else(|| {
            if self.clean_snapshot == self.current_snapshot {
                DocumentStatus::Clean
            } else {
                DocumentStatus::Dirty
            }
        })
    }

    pub(crate) fn refresh(&mut self, snapshot: String) {
        self.current_snapshot = Some(snapshot);
    }

    pub(crate) fn checkpoint(&mut self, path: PathBuf, snapshot: String) {
        self.identity = DocumentIdentity::Saved(path);
        self.clean_snapshot = Some(snapshot.clone());
        self.current_snapshot = Some(snapshot);
    }

    pub(crate) fn mark_unknown(&mut self) {
        self.unavailable = Some(DocumentStatus::Unknown);
    }

    pub(crate) fn mark_recovery_required(&mut self) {
        self.unavailable = Some(DocumentStatus::RecoveryRequired);
    }

    pub(crate) fn can_transition(&self) -> bool {
        !matches!(
            self.status(),
            DocumentStatus::Unknown | DocumentStatus::RecoveryRequired
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untitled_and_opened_documents_start_clean_with_their_identity() {
        let untitled = SceneDocumentState::new_untitled("baseline".into());
        assert_eq!(untitled.status(), DocumentStatus::Clean);
        assert_eq!(untitled.identity(), &DocumentIdentity::Untitled);

        let path = PathBuf::from("scenes/level.ron");
        let opened = SceneDocumentState::opened(path.clone(), "baseline".into());
        assert_eq!(opened.status(), DocumentStatus::Clean);
        assert_eq!(opened.identity(), &DocumentIdentity::Saved(path));
    }

    #[test]
    fn content_change_is_dirty_and_returning_to_checkpoint_is_clean() {
        let mut state = SceneDocumentState::new_untitled("saved contents".into());
        state.refresh("edited contents".into());
        assert_eq!(state.status(), DocumentStatus::Dirty);

        state.refresh("saved contents".into());
        assert_eq!(state.status(), DocumentStatus::Clean);
    }

    #[test]
    fn successful_save_advances_path_and_checkpoint() {
        let mut state = SceneDocumentState::new_untitled("initial".into());
        state.refresh("edited".into());
        state.checkpoint(PathBuf::from("scenes/new.ron"), "edited".into());
        assert_eq!(state.status(), DocumentStatus::Clean);
        assert_eq!(
            state.identity(),
            &DocumentIdentity::Saved(PathBuf::from("scenes/new.ron"))
        );

        state.refresh("edited again".into());
        assert_eq!(state.status(), DocumentStatus::Dirty);
    }

    #[test]
    fn unknown_and_recovery_states_block_transition_and_cannot_be_refreshed_clean() {
        let mut state = SceneDocumentState::new_untitled("baseline".into());
        state.mark_unknown();
        state.refresh("same contents".into());
        assert_eq!(state.status(), DocumentStatus::Unknown);
        assert!(!state.can_transition());

        state.mark_recovery_required();
        assert_eq!(state.status(), DocumentStatus::RecoveryRequired);
        assert!(!state.can_transition());
    }
}

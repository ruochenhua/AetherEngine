//! A pinned, read-only snapshot. Only explicit actions schedule filesystem work.
use super::ProjectPanel;
use crate::project_dependencies::{
    self, AnalysisBudget, DependencyReport, ReferenceEdge, Resolution,
};
use std::{
    path::{Component, Path, PathBuf},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
};

enum Outcome {
    Analysis(Result<DependencyReport, String>),
    Recheck(Result<bool, String>),
}
pub(super) struct ReferencePreview {
    root: PathBuf,
    target: Option<String>,
    report: Option<Arc<DependencyReport>>,
    result: Option<Receiver<Outcome>>,
    error: Option<String>,
    stale: bool,
}
impl ReferencePreview {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            target: None,
            report: None,
            result: None,
            error: None,
            stale: false,
        }
    }
    pub fn open(&mut self, target: String) {
        self.target = Some(target);
        self.reanalyze();
    }
    pub fn busy(&self) -> bool {
        self.result.is_some()
    }
    pub fn poll(&mut self) {
        let Some(receiver) = &self.result else {
            return;
        };
        let outcome = match receiver.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.error = Some("Dependency worker disconnected".into());
                self.result = None;
                return;
            }
        };
        self.result = None;
        match outcome {
            Outcome::Analysis(Ok(report)) => {
                self.stale = report.stale;
                self.report = Some(Arc::new(report));
            }
            Outcome::Recheck(Ok(stale)) => self.stale |= stale,
            Outcome::Analysis(Err(error)) | Outcome::Recheck(Err(error)) => {
                self.error = Some(error)
            }
        }
    }
    pub fn report(&self) -> Option<&DependencyReport> {
        self.report.as_deref()
    }
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }
    pub fn status(&self) -> &'static str {
        if self.target.is_none() {
            "Closed"
        } else if self.error.is_some() {
            "Failed"
        } else if self.report.is_none() {
            "Loading"
        } else if self.stale {
            "Stale"
        } else if self.report().is_some_and(|report| !report.is_complete()) {
            "Incomplete"
        } else if self.report().is_some_and(|report| {
            report.used_by().next().is_none() && report.depends_on().next().is_none()
        }) {
            "Empty"
        } else {
            "Complete"
        }
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn recheck(&mut self) {
        if self.busy() {
            return;
        }
        let Some(report) = self.report.clone() else {
            return;
        };
        let root = self.root.clone();
        self.schedule(move || Outcome::Recheck(report.is_stale(&root, AnalysisBudget::default())));
    }
    pub fn reanalyze(&mut self) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let root = self.root.clone();
        self.report = None;
        self.stale = false;
        self.schedule(move || {
            Outcome::Analysis(project_dependencies::analyze(
                &root,
                &target,
                AnalysisBudget::default(),
            ))
        });
    }
    fn schedule(&mut self, work: impl FnOnce() -> Outcome + Send + 'static) {
        let (sender, receiver) = mpsc::channel();
        self.error = None;
        // Replacing/dropping this receiver also discards results from an older target.
        self.result = Some(receiver);
        if let Err(error) = std::thread::Builder::new()
            .name("project-references".into())
            .spawn(move || {
                let _ = sender.send(work());
            })
        {
            self.result = None;
            self.error = Some(format!("Cannot start dependency worker: {error}"));
        }
    }
    pub fn escape(&mut self, owns_focus: bool, text_input: bool) -> bool {
        if self.target.is_some() && owns_focus && !text_input {
            self.close();
            true
        } else {
            false
        }
    }
    pub fn close(&mut self) {
        self.target = None;
        self.report = None;
        self.result = None;
        self.error = None;
        self.stale = false;
    }
    pub fn source_navigable(&self, edge: &ReferenceEdge) -> bool {
        edge.resolution == Resolution::Resolved
            && safe_source(&edge.source)
            && self
                .report()
                .is_some_and(|report| report.files.contains_key(&edge.source))
    }
}
fn safe_source(source: &str) -> bool {
    let mut parts = Path::new(source).components();
    matches!(parts.next(), Some(Component::Normal(root)) if root == "assets" || root == "scenes")
        && parts.clone().next().is_some()
        && parts.all(|part| matches!(part, Component::Normal(_)))
        && !source.contains('\\')
}
impl ProjectPanel {
    pub(super) fn show_references(&mut self) {
        if let Some(path) = self.selected_resource.clone().filter(|path| {
            self.index
                .snapshot()
                .entries
                .iter()
                .any(|entry| &entry.path == path)
        }) {
            self.references.open(path);
        }
    }
    pub(super) fn select_reference_source(&mut self, edge: &ReferenceEdge) {
        if self.references.source_navigable(edge) {
            self.select_resource(Some(PathBuf::from(&edge.source)));
        }
    }
    pub(super) fn back_to_reference_target(&mut self) {
        if let Some(target) = self.references.target().map(PathBuf::from) {
            self.select_resource(Some(target));
        }
    }
}

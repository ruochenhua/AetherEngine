//! Read-only typed authored dependency snapshots for the Project panel.
use std::{collections::BTreeMap, path::Path};
mod inventory;
mod resolve;
mod visit;

#[derive(Clone, Copy, Debug)]
pub struct AnalysisBudget {
    pub max_entries: usize,
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}
impl Default for AnalysisBudget {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_files: 50_000,
            max_file_bytes: 64 * 1024 * 1024,
            max_total_bytes: 1024 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ReferenceKind {
    Mesh,
    MaterialAsset,
    PrefabAsset,
    AlbedoTexture,
    NormalTexture,
    OrmTexture,
    EmissiveTexture,
    TransparentTexture,
    TerrainHeightmap,
    TerrainSplatmap,
    TerrainAlbedo,
    TerrainNormal,
    TerrainRoughnessMetallic,
    WaterDudv,
    WaterNormal,
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SourceLocator {
    SceneObject {
        index: usize,
        name: String,
    },
    ScenePrefab {
        index: usize,
        instance_id: u64,
        override_index: Option<usize>,
        node_id: Option<u64>,
    },
    PrefabNode {
        instance_id: u64,
        component_index: usize,
    },
    MaterialAsset,
    Terrain {
        layer: Option<usize>,
    },
    Water,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Resolution {
    Resolved,
    Missing,
    Unsafe(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceEdge {
    pub source: String,
    pub locator: SourceLocator,
    pub kind: ReferenceKind,
    pub authored: String,
    pub candidate: String,
    pub resolution: Resolution,
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Diagnostic {
    pub path: String,
    pub message: String,
}
pub type Fingerprint = [u8; 32];
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyReport {
    pub target: String,
    pub edges: Vec<ReferenceEdge>,
    pub diagnostics: Vec<Diagnostic>,
    pub files: BTreeMap<String, Fingerprint>,
    pub directories: Vec<String>,
    pub fingerprint: Fingerprint,
    pub stale: bool,
}
impl DependencyReport {
    pub fn is_complete(&self) -> bool {
        !self.stale && self.diagnostics.is_empty()
    }
    pub fn used_by(&self) -> impl Iterator<Item = &ReferenceEdge> {
        self.edges.iter().filter(|e| e.candidate == self.target)
    }
    pub fn depends_on(&self) -> impl Iterator<Item = &ReferenceEdge> {
        self.edges.iter().filter(|e| e.source == self.target)
    }
    pub fn is_stale(&self, root: &Path, budget: AnalysisBudget) -> Result<bool, String> {
        let current = inventory::scan(root, budget, false)?;
        Ok(self.stale || current.stale || self.fingerprint != current.fingerprint)
    }
}
/// Analyze a bounded snapshot of both allowlisted roots. No project state is mutated.
/// The second streaming pass detects content and inventory changes during analysis.
pub fn analyze(
    root: &Path,
    target: &str,
    budget: AnalysisBudget,
) -> Result<DependencyReport, String> {
    let first = inventory::scan(root, budget, true)?;
    finish(root, target, budget, first)
}
fn finish(
    root: &Path,
    target: &str,
    budget: AnalysisBudget,
    first: inventory::Inventory,
) -> Result<DependencyReport, String> {
    let mut report = DependencyReport {
        target: target.into(),
        edges: vec![],
        diagnostics: first.diagnostics,
        files: first.files,
        directories: first.directories,
        fingerprint: first.fingerprint,
        stale: first.stale,
    };
    match resolve::resolve(root, target) {
        Resolution::Resolved => (),
        status => report.diagnostics.push(Diagnostic {
            path: target.into(),
            message: format!("Selected target is unresolved: {status:?}"),
        }),
    }
    for path in report.files.keys() {
        let extension = inventory::extension(path);
        if extension == "ron" {
            let references = first
                .documents
                .get(path)
                .ok_or_else(|| "Document bytes unavailable".to_owned())
                .and_then(|bytes| std::str::from_utf8(bytes).map_err(|e| e.to_string()))
                .and_then(visit::document);
            match references {
                Err(message) => report.diagnostics.push(Diagnostic {
                    path: path.clone(),
                    message,
                }),
                Ok(references) => {
                    for reference in references {
                        let candidate = resolve::candidate(path, &reference);
                        let resolution =
                            resolve::reference_resolution(root, &reference, &candidate);
                        if resolution != Resolution::Resolved {
                            report.diagnostics.push(Diagnostic {
                                path: path.clone(),
                                message: format!(
                                    "{:?} {:?}: {:?} -> {:?}: {resolution:?}",
                                    reference.locator, reference.kind, reference.path, candidate
                                ),
                            });
                        }
                        report.edges.push(ReferenceEdge {
                            source: path.clone(),
                            locator: reference.locator,
                            kind: reference.kind,
                            authored: reference.path,
                            candidate,
                            resolution,
                        });
                    }
                }
            }
        } else if let Some(message) =
            visit::endpoint_diagnostic(&extension, first.documents.get(path).map(Vec::as_slice))
        {
            report.diagnostics.push(Diagnostic {
                path: path.clone(),
                message,
            });
        }
    }
    let current = inventory::scan(root, budget, false)?;
    if current.stale || current.fingerprint != report.fingerprint {
        report.stale = true;
    }
    if report.stale {
        report.diagnostics.push(Diagnostic {
            path: String::new(),
            message: "Project content or inventory changed during analysis".into(),
        });
    }
    report.edges.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.kind.cmp(&b.kind))
            .then(a.candidate.cmp(&b.candidate))
            .then(a.locator.cmp(&b.locator))
            .then(a.authored.cmp(&b.authored))
    });
    report.diagnostics.sort();
    report.diagnostics.dedup();
    Ok(report)
}

#[cfg(test)]
mod snapshot_tests;

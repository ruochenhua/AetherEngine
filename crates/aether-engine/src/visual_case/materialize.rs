//! Independent case expectations and artifact identities; no runner I/O.

use super::{validation::validate_id, ExpectedResult, VisualCase, VisualCaseError};
use serde::Serialize;
use std::ops::Deref;

/// Expanded case plus its explicit expectation. This execution record is distinct
/// from the strict input schema; serializing `case` alone preserves that schema.
#[derive(Clone, Debug, Serialize)]
pub struct MaterializedCase {
    /// Complete case after overrides, with its own reference provenance.
    #[serde(flatten)]
    pub case: VisualCase,
    /// Preserved variant expectation; absent when no variants were declared.
    pub expected_result: Option<ExpectedResult>,
}

impl Deref for MaterializedCase {
    type Target = VisualCase;

    fn deref(&self) -> &Self::Target {
        &self.case
    }
}

/// Run-specific artifact identity for one complete case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseArtifacts {
    /// Rendered output image path relative to the project root.
    pub output: String,
    /// Difference image path relative to the project root.
    pub diff: String,
    /// Case report path relative to the project root.
    pub report: String,
}

impl MaterializedCase {
    /// Derives artifact paths without creating files or starting a runner.
    /// The future runner must validate filesystem containment before writing.
    pub fn artifacts(&self, run_id: &str) -> Result<CaseArtifacts, VisualCaseError> {
        validate_id(run_id, "run id")?;
        validate_id(&self.id, "case id")?;
        let directory = format!("tests/reports/{run_id}/{}", self.id);
        Ok(CaseArtifacts {
            output: format!("{directory}/output.png"),
            diff: format!("{directory}/diff.png"),
            report: format!("{directory}/report.html"),
        })
    }
}

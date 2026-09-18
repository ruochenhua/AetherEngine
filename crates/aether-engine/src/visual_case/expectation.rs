//! Typed expectations from the frozen VisualCase v2 contract.

use super::validation::{invalid, is_lower_hex, nonempty, required_option};
use super::VisualCaseError;
use serde::{Deserialize, Serialize};

/// A variant's required render outcome or classified failure.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ExpectedResult {
    /// Successful rendering with structured assertions.
    Render {
        /// Required diagnostic codes and severities.
        diagnostics: Vec<DiagnosticExpectation>,
        /// Required fallback modes by scope.
        fallbacks: Vec<FallbackExpectation>,
        /// Numeric comparisons.
        metrics: Vec<MetricExpectation>,
        /// Exact named SHA-256 values.
        hashes: Vec<HashExpectation>,
        /// Typed artifact or state probes.
        probes: Vec<ProbeExpectation>,
        /// Required graph read/write sets and ordered edges; explicitly nullable.
        #[serde(deserialize_with = "required_option")]
        graph: Option<GraphExpectation>,
    },
    /// A specific expected error, never an arbitrary runner failure.
    ExpectedError {
        /// Stable error code.
        code: String,
        /// Required diagnostics.
        diagnostics: Vec<DiagnosticExpectation>,
    },
}

/// Diagnostic severity as serialized by the frozen fixtures.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational diagnostic.
    Info,
    /// Recoverable warning.
    Warning,
    /// Error diagnostic.
    Error,
}

/// A required diagnostic code and severity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticExpectation {
    /// Stable diagnostic code.
    pub code: String,
    /// Required severity.
    pub severity: Severity,
}

/// Fallback mode within a named scope.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FallbackExpectation {
    /// Subsystem or primitive scope.
    pub scope: String,
    /// Expected fallback mode.
    pub mode: String,
}

/// Supported metric comparison operators.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum MetricOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
}

/// A finite numeric metric comparison.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MetricExpectation {
    /// Registered metric name.
    pub name: String,
    /// Comparison operation.
    pub operator: MetricOp,
    /// Finite comparison value.
    pub value: f64,
}

/// An exact named hash.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HashExpectation {
    /// Hash name.
    pub name: String,
    /// Lowercase hexadecimal SHA-256.
    pub sha256: String,
}

/// A typed probe value, including explicitly present artifacts.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum ProbeValue {
    /// Boolean assertion.
    Bool(bool),
    /// Unsigned integer assertion.
    U32(u32),
    /// Finite floating point assertion with nonnegative tolerance.
    F32 {
        /// Expected value.
        value: f32,
        /// Absolute tolerance.
        tolerance: f32,
    },
    /// Exact pixel channels.
    Rgba8([u8; 4]),
    /// Exact textual value.
    Text(String),
    /// Artifact or value must be present.
    Present,
}

/// A named probe assertion.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProbeExpectation {
    /// Registered probe name.
    pub name: String,
    /// Expected typed value.
    pub value: ProbeValue,
}

/// Expected resource sets and ordered graph edges.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GraphExpectation {
    /// Resources read.
    pub reads: Vec<String>,
    /// Resources written.
    pub writes: Vec<String>,
    /// Ordered execution edges followed by composite equation edges.
    pub edges: Vec<String>,
}

impl ExpectedResult {
    pub(super) fn validate(&self) -> Result<(), VisualCaseError> {
        let diagnostics = match self {
            Self::Render {
                diagnostics,
                fallbacks,
                metrics,
                hashes,
                probes,
                graph,
            } => {
                for item in fallbacks {
                    nonempty(&item.scope, "fallback scope")?;
                    nonempty(&item.mode, "fallback mode")?;
                }
                for item in metrics {
                    nonempty(&item.name, "metric name")?;
                    if !item.value.is_finite() {
                        return Err(invalid("metric values must be finite"));
                    }
                }
                for item in hashes {
                    nonempty(&item.name, "hash name")?;
                    if !is_lower_hex(&item.sha256, 64) {
                        return Err(invalid("expected hash must be lowercase SHA-256"));
                    }
                }
                for item in probes {
                    nonempty(&item.name, "probe name")?;
                    if let ProbeValue::F32 { value, tolerance } = item.value {
                        if !value.is_finite() || !tolerance.is_finite() || tolerance < 0.0 {
                            return Err(invalid(
                                "probe value/tolerance must be finite and tolerance nonnegative",
                            ));
                        }
                    }
                }
                if let Some(graph) = graph {
                    for name in graph.reads.iter().chain(&graph.writes).chain(&graph.edges) {
                        nonempty(name, "graph entry")?;
                    }
                }
                diagnostics
            }
            Self::ExpectedError { code, diagnostics } => {
                nonempty(code, "expected error code")?;
                diagnostics
            }
        };
        for item in diagnostics {
            nonempty(&item.code, "diagnostic code")?;
        }
        Ok(())
    }
}

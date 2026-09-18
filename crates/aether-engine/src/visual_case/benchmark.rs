//! Strict benchmark configuration; collection remains outside T0.1.

use super::validation::{invalid, is_lower_hex, nonempty};
use super::VisualCaseError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BenchmarkSpec {
    entity_count: u32,
    seed: u64,
    warmup: u32,
    samples: u32,
    feature_flags: BTreeMap<String, bool>,
    baseline_commit: String,
    device: DeviceIdentity,
    paired: Option<PairedBenchmark>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeviceIdentity {
    os: String,
    arch: String,
    driver: String,
    adapter: String,
    resolution: [u32; 2],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PairedBenchmark {
    baseline_flags: BTreeMap<String, bool>,
    feature_flags: BTreeMap<String, bool>,
    equivalence: String,
}

impl BenchmarkSpec {
    pub(super) fn validate(&self) -> Result<(), VisualCaseError> {
        if self.entity_count == 0 || self.samples == 0 {
            return Err(invalid(
                "benchmark entity_count and samples must be positive",
            ));
        }
        if !is_lower_hex(&self.baseline_commit, 40) {
            return Err(invalid(
                "benchmark baseline_commit must be a lowercase 40-digit commit",
            ));
        }
        for value in [
            &self.device.os,
            &self.device.arch,
            &self.device.driver,
            &self.device.adapter,
        ] {
            nonempty(value, "benchmark device identity")?;
        }
        if self.device.resolution.contains(&0) {
            return Err(invalid("benchmark device resolution must be positive"));
        }
        for name in self.feature_flags.keys() {
            nonempty(name, "benchmark feature flag")?;
        }
        if let Some(pair) = &self.paired {
            nonempty(&pair.equivalence, "paired equivalence")?;
            for name in pair.baseline_flags.keys().chain(pair.feature_flags.keys()) {
                nonempty(name, "paired feature flag")?;
            }
        }
        Ok(())
    }
}

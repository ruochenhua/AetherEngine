//! Phase ordering support for the render graph builder.

use crate::renderer::pass::{Pass, PassSignature};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RenderPhase {
    Shadow,
    Geometry,
    AmbientOcclusion,
    Lighting,
    Atmosphere,
    ScreenSpaceReflection,
    VolumetricOverlay,
    PlanarReflection,
    Transparent,
    Composite,
    Bloom,
    ToneMapping,
    AntiAliasing,
    Debug,
    Unspecified,
}

pub(super) fn phase_for_pass(name: &str) -> RenderPhase {
    match name {
        "Shadow" => RenderPhase::Shadow,
        "GBuffer" | "Terrain" => RenderPhase::Geometry,
        "SSAO" | "AOBlur" => RenderPhase::AmbientOcclusion,
        "Lighting" => RenderPhase::Lighting,
        "Atmosphere" => RenderPhase::Atmosphere,
        "SSR" => RenderPhase::ScreenSpaceReflection,
        "VolumetricCloud" | "GodRay" => RenderPhase::VolumetricOverlay,
        "WaterReflection" => RenderPhase::PlanarReflection,
        "Water" => RenderPhase::Transparent,
        "Composite" => RenderPhase::Composite,
        "Bloom" => RenderPhase::Bloom,
        "ToneMapping" => RenderPhase::ToneMapping,
        "FXAA" => RenderPhase::AntiAliasing,
        "DebugLine" => RenderPhase::Debug,
        _ => RenderPhase::Unspecified,
    }
}

/// Add ordering edges between explicitly staged passes.
pub(super) fn add_phase_dependencies(deps: &mut [Vec<usize>], sigs: &[PassSignature]) {
    for (later, later_sig) in sigs.iter().enumerate() {
        let later_phase = phase_for_pass(later_sig.name);
        if later_phase == RenderPhase::Unspecified {
            continue;
        }
        for (earlier, earlier_sig) in sigs.iter().enumerate() {
            let earlier_phase = phase_for_pass(earlier_sig.name);
            if earlier_phase < later_phase && !deps[later].contains(&earlier) {
                deps[later].push(earlier);
            }
        }
    }
}

pub(super) fn add_phase_dependencies_for_passes(deps: &mut [Vec<usize>], passes: &[Box<dyn Pass>]) {
    let sigs: Vec<PassSignature> = passes.iter().map(|pass| pass.signature()).collect();
    add_phase_dependencies(deps, &sigs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_phases_add_edges_even_when_registered_backwards() {
        let sigs = vec![
            PassSignature::new("Composite"),
            PassSignature::new("GBuffer"),
        ];
        let mut deps = vec![Vec::new(); sigs.len()];

        add_phase_dependencies(&mut deps, &sigs);

        assert!(deps[0].contains(&1));
        assert!(deps[1].is_empty());
    }

    #[test]
    fn standard_passes_have_stable_phases() {
        assert!(phase_for_pass("GBuffer") < phase_for_pass("Lighting"));
        assert!(phase_for_pass("SSR") < phase_for_pass("Water"));
        assert!(phase_for_pass("WaterReflection") < phase_for_pass("Composite"));
    }
}

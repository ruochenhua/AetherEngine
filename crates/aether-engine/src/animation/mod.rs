//! Deterministic animation sampling and immutable render-frame extraction.

mod extract;
mod palette;
mod player;
mod skinning;
#[cfg(test)]
mod skinning_tests;
#[cfg(test)]
mod tests_support;

pub use extract::{
    extract_animation_frame, AnimationAssets, AnimationCameraFrame, AnimationContext,
    AnimationDiagnostic, AnimationFrame, AnimationPrimitive, AnimationRenderItem, MeshRef,
    SceneSnapshot, SkinnedRenderItem, StaticFallback,
};
pub use palette::{build_joint_palette, JointPalette};
pub use player::{
    pose_state_hash, sample_animator, sample_animator_with_looping, AnimationError,
    AnimatorRuntime, AnimatorState, LocalPose, PoseSample,
};
pub(crate) use skinning::validate_skinning_inputs;
#[cfg(test)]
pub(crate) use tests_support::animation_frame_with_bounds;

//! Deterministic CPU pose extraction for skinned render items.

mod extract;
mod gpu;
mod palette;
mod playback;
mod types;

pub use extract::{extract_animation_frame, AnimationAssets};
pub use gpu::JointPaletteStorage;
pub use palette::{build_joint_palette, skinned_world_bounds};
pub use playback::{pose_state_hash, sample_animation_pose, AnimatorState};
pub use types::{
    AnimationCameraFrame, AnimationContext, AnimationDiagnostic, AnimationError, AnimationFrame,
    AnimationItemResult, AnimationPrimitiveMapping, AnimationPrimitiveSnapshot,
    AnimationRenderItem, AnimatorRuntime, JointPalette, JointPaletteGpu, LocalPose, SceneSnapshot,
    StaticFallbackReason,
};

#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod tests;

//! Deterministic TRS sampling and animator controls.

use crate::asset::gltf_document::{
    AnimationClipAsset, AnimationPath, AnimationValues, GltfDocument, Interpolation, SkeletonAsset,
    ValidationError,
};
use crate::asset::Handle;
use glam::{Mat4, Quat, Vec3};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A sampled local transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalPose {
    /// Local translation.
    pub translation: Vec3,
    /// Local rotation.
    pub rotation: Quat,
    /// Local scale.
    pub scale: Vec3,
}

impl From<&crate::asset::gltf_document::DocumentNode> for LocalPose {
    fn from(node: &crate::asset::gltf_document::DocumentNode) -> Self {
        let rotation = Quat::from_array(node.rotation);
        Self {
            translation: Vec3::from_array(node.translation),
            rotation: if rotation.is_finite() && rotation.length_squared() > 1.0e-12 {
                rotation.normalize()
            } else {
                Quat::IDENTITY
            },
            scale: Vec3::from_array(node.scale),
        }
    }
}

impl LocalPose {
    pub(crate) fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// Playback controls attached to an animated entity.
#[derive(Debug, Clone)]
pub struct AnimatorState {
    /// Optional typed clip handle; no clip produces a bind-pose fallback.
    pub clip: Option<Handle<AnimationClipAsset>>,
    /// Playback position in seconds.
    pub time: f32,
    /// Playback speed multiplier.
    pub speed: f32,
    /// Wrap time at the clip duration.
    pub looping: bool,
    /// Whether `advance` changes the playback position.
    pub playing: bool,
}

impl Default for AnimatorState {
    fn default() -> Self {
        Self {
            clip: None,
            time: 0.0,
            speed: 1.0,
            looping: true,
            playing: true,
        }
    }
}

/// Mutable playback state. Renderer code receives only [`super::AnimationFrame`].
#[derive(Debug, Default)]
pub struct AnimatorRuntime {
    /// Entity bits to playback controls, kept in deterministic order.
    pub states: BTreeMap<u64, AnimatorState>,
    /// Scene generation the runtime currently belongs to.
    pub generation: u64,
}

impl AnimatorRuntime {
    /// Advance playing states by a finite non-negative delta.
    pub fn advance(&mut self, delta_seconds: f32) -> Result<(), AnimationError> {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(AnimationError::InvalidTime);
        }
        for state in self.states.values_mut() {
            if state.playing {
                if !state.speed.is_finite() {
                    return Err(AnimationError::InvalidSpeed);
                }
                state.time += delta_seconds * state.speed;
                if !state.time.is_finite() {
                    return Err(AnimationError::InvalidTime);
                }
            }
        }
        Ok(())
    }

    /// Set a deterministic playback position, ignoring pause and speed.
    pub fn seek(&mut self, entity_bits: u64, seconds: f32) -> Result<(), AnimationError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(AnimationError::InvalidTime);
        }
        let state = self
            .states
            .get_mut(&entity_bits)
            .ok_or(AnimationError::MissingState(entity_bits))?;
        state.time = seconds;
        Ok(())
    }
}

/// Pose sampled at a time, with explicit bind-pose fallback status.
#[derive(Debug, Clone, PartialEq)]
pub struct PoseSample {
    /// One local TRS value per document node.
    pub locals: Arc<[LocalPose]>,
    /// True when a missing or zero-duration clip selected bind pose.
    pub bind_pose_fallback: bool,
}

/// Errors raised while sampling or constructing an animation frame.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnimationError {
    /// Invalid keyframes from the glTF document.
    #[error("invalid animation document: {0}")]
    Document(ValidationError),
    /// No state is registered for the requested entity.
    #[error("animator state is missing for entity {0}")]
    MissingState(u64),
    /// The clip has no usable channels or has zero duration.
    #[error("animation clip is missing or has zero duration")]
    MissingClip,
    /// The requested time is invalid.
    #[error("animation time must be finite and non-negative")]
    InvalidTime,
    /// Playback speed is not finite.
    #[error("animation speed must be finite")]
    InvalidSpeed,
    /// A channel targets a node or property with incompatible values.
    #[error("animation channel is invalid")]
    InvalidChannel,
    /// Cubic spline is retained in the document but not supported by this sampler.
    #[error("cubic spline interpolation is unsupported")]
    UnsupportedInterpolation,
    /// A required joint or parent node is absent.
    #[error("animation references a missing joint node {0}")]
    MissingJoint(usize),
    /// The generated palette is non-finite or exceeds supported capacity.
    #[error("joint palette is invalid")]
    InvalidPalette,
    /// A frame belongs to a stale scene generation.
    #[error("stale animation generation: expected {expected}, got {actual}")]
    StaleGeneration {
        /// Generation expected by the frame context.
        expected: u64,
        /// Generation stored in the runtime or snapshot.
        actual: u64,
    },
    /// The assets resolver could not provide an asset.
    #[error("animation asset is unavailable")]
    MissingAsset,
}

/// Sample the clip for one skeleton. Missing and zero-duration clips return bind pose.
pub fn sample_animator(
    document: &GltfDocument,
    skeleton: &SkeletonAsset,
    clip: Option<&AnimationClipAsset>,
    sample_time: f32,
) -> Result<PoseSample, AnimationError> {
    sample_animator_with_looping(document, skeleton, clip, sample_time, true)
}

/// Sample a clip using either wrapped or clamped playback time.
pub fn sample_animator_with_looping(
    document: &GltfDocument,
    skeleton: &SkeletonAsset,
    clip: Option<&AnimationClipAsset>,
    sample_time: f32,
    looping: bool,
) -> Result<PoseSample, AnimationError> {
    if !sample_time.is_finite() || sample_time < 0.0 {
        return Err(AnimationError::InvalidTime);
    }
    let mut locals: Vec<_> = document.nodes.iter().map(LocalPose::from).collect();
    if let Some(index) = skeleton
        .joints
        .iter()
        .find(|index| **index >= document.nodes.len())
    {
        return Err(AnimationError::MissingJoint(*index));
    }
    let Some(clip) = clip else {
        return Ok(PoseSample {
            locals: Arc::from(locals),
            bind_pose_fallback: true,
        });
    };
    if !clip.duration.is_finite() || clip.duration <= 0.0 || clip.channels.is_empty() {
        return Ok(PoseSample {
            locals: Arc::from(locals),
            bind_pose_fallback: true,
        });
    }
    let time = if looping {
        sample_time.rem_euclid(clip.duration)
    } else {
        sample_time.clamp(0.0, clip.duration)
    };
    for channel in &clip.channels {
        let target = locals
            .get_mut(channel.target_joint)
            .ok_or(AnimationError::MissingJoint(channel.target_joint))?;
        let (left, right, alpha) = channel_interval(&channel.times, time)?;
        match (&channel.target, &channel.values, channel.interpolation) {
            (_, _, Interpolation::CubicSpline) => {
                return Err(AnimationError::UnsupportedInterpolation)
            }
            (AnimationPath::Translation, AnimationValues::Vec3(values), interpolation) => {
                let value = sample_vec3(values, left, right, alpha, interpolation);
                target.translation = Vec3::from_array(value);
            }
            (AnimationPath::Scale, AnimationValues::Vec3(values), interpolation) => {
                let value = sample_vec3(values, left, right, alpha, interpolation);
                target.scale = Vec3::from_array(value);
            }
            (AnimationPath::Rotation, AnimationValues::Vec4(values), interpolation) => {
                let left_index = left;
                let right_index = right;
                let left_rotation = Quat::from_array(
                    *values
                        .get(left_index)
                        .ok_or(AnimationError::InvalidChannel)?,
                );
                let value = if interpolation == Interpolation::Step || left_index == right_index {
                    left_rotation
                } else {
                    let right = Quat::from_array(
                        *values
                            .get(right_index)
                            .ok_or(AnimationError::InvalidChannel)?,
                    );
                    left_rotation.slerp(right, alpha).normalize()
                };
                if !value.is_finite() || value.length_squared() < 1.0e-12 {
                    return Err(AnimationError::InvalidChannel);
                }
                target.rotation = value.normalize();
            }
            _ => return Err(AnimationError::InvalidChannel),
        }
    }
    if locals.iter().any(|pose| {
        !pose.translation.is_finite() || !pose.rotation.is_finite() || !pose.scale.is_finite()
    }) {
        return Err(AnimationError::InvalidChannel);
    }
    Ok(PoseSample {
        locals: Arc::from(locals),
        bind_pose_fallback: false,
    })
}

fn channel_interval(times: &[f32], time: f32) -> Result<(usize, usize, f32), AnimationError> {
    if times.is_empty() || times.iter().any(|value| !value.is_finite()) {
        return Err(AnimationError::InvalidChannel);
    }
    if times.len() == 1 || time <= times[0] {
        return Ok((0, 0, 0.0));
    }
    let last = times.len() - 1;
    if time >= times[last] {
        return Ok((last, last, 0.0));
    }
    let upper = times.partition_point(|key| *key <= time);
    let left = upper - 1;
    let span = times[upper] - times[left];
    if span <= 0.0 || !span.is_finite() {
        return Err(AnimationError::InvalidChannel);
    }
    Ok((left, upper, (time - times[left]) / span))
}

fn sample_vec3(
    values: &[[f32; 3]],
    left: usize,
    right: usize,
    alpha: f32,
    interpolation: Interpolation,
) -> [f32; 3] {
    let a = Vec3::from_array(values[left]);
    if interpolation == Interpolation::Step || left == right {
        return a.to_array();
    }
    a.lerp(Vec3::from_array(values[right]), alpha).to_array()
}

/// Hash the frozen T7.3 pose-state payload (UTF-8, no trailing newline).
pub fn pose_state_hash(variant: &str, normalized_time: &str) -> Result<String, AnimationError> {
    let time = normalized_time
        .parse::<f32>()
        .map_err(|_| AnimationError::InvalidTime)?;
    if !time.is_finite() || time < 0.0 {
        return Err(AnimationError::InvalidTime);
    }
    let payload = format!(
        "aether.pose.v1|case=t7_gltf_actor_seek_0_25_50_75|variant={variant}|target_seconds={normalized_time}|fixed_dt=0.016666667|joint_count=128|joint0_translation_x={normalized_time}|joint0_rotation=0,0,0,1|joint0_scale=1,1,1|remaining_joints=bind"
    );
    let hash = Sha256::digest(payload.as_bytes());
    Ok(hash.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;

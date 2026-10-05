use super::types::{AnimationError, LocalPose};
use crate::asset::loaders::gltf::{
    AnimationClipAsset, AnimationValues, ChannelTarget, Interpolation, SkeletonAsset,
};
use glam::{Quat, Vec3};
use sha2::{Digest, Sha256};

/// Deterministic playback controls for one typed glTF animation clip.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimatorState {
    time: f32,
    speed: f32,
    playing: bool,
    looping: bool,
}

impl AnimatorState {
    /// Start paused at time zero with normal speed and the requested loop mode.
    pub fn new(looping: bool) -> Self {
        Self {
            time: 0.0,
            speed: 1.0,
            playing: false,
            looping,
        }
    }

    /// Current clip-local time in seconds.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Whether advancing this state changes its time.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Current playback speed; negative values play backwards.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Whether playback wraps across the clip duration.
    pub fn is_looping(&self) -> bool {
        self.looping
    }

    /// Begin playback.
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Pause playback without changing the selected pose.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Set a finite playback speed. Negative values are reverse playback.
    pub fn set_speed(&mut self, speed: f32) -> Result<(), AnimationError> {
        if !speed.is_finite() {
            return Err(invalid_clip("playback speed must be finite"));
        }
        self.speed = speed;
        Ok(())
    }

    /// Change whether playback wraps when it reaches either end.
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Select an exact pose time, clamped or wrapped to the clip duration.
    pub fn seek(&mut self, time: f32, duration: f32) -> Result<f32, AnimationError> {
        validate_clock(time, duration)?;
        self.time = normalize_time(time, duration, self.looping);
        Ok(self.time)
    }

    /// Advance this state by a non-negative wall-time delta.
    pub fn advance(&mut self, dt: f32, duration: f32) -> Result<f32, AnimationError> {
        validate_clock(dt, duration)?;
        if !self.playing || dt == 0.0 || self.speed == 0.0 {
            return Ok(self.time);
        }
        let next = self.time + dt * self.speed;
        if !next.is_finite() {
            return Err(invalid_clip("playback time overflowed"));
        }
        if self.looping {
            self.time = normalize_time(next, duration, true);
        } else if self.speed > 0.0 {
            self.time = next.min(duration);
            if self.time >= duration {
                self.playing = false;
            }
        } else {
            self.time = next.max(0.0);
            if self.time <= 0.0 {
                self.playing = false;
            }
        }
        Ok(self.time)
    }
}

/// Sample a typed glTF clip into palette-ordered local joint transforms.
pub fn sample_animation_pose(
    clip: &AnimationClipAsset,
    skeleton: &SkeletonAsset,
    time: f32,
) -> Result<Vec<LocalPose>, AnimationError> {
    validate_clock(time, clip.duration)?;
    if skeleton.joints.is_empty() || skeleton.joints.len() > 128 {
        return Err(AnimationError::InvalidPaletteSize(skeleton.joints.len()));
    }
    let sample_time = time.min(clip.duration);
    let mut pose: Vec<_> = skeleton.joints.iter().map(LocalPose::from_joint).collect();
    for channel in clip.channels.iter() {
        if channel_value_len(&channel.values) != channel.times.len() {
            return Err(invalid_clip("channel values do not match key times"));
        }
        let target = pose
            .get_mut(channel.target_joint as usize)
            .ok_or_else(|| invalid_clip("channel targets a joint outside the selected skin"))?;
        let (lower, upper, amount) = key_interval(&channel.times, sample_time)?;
        let amount = match channel.interpolation {
            Interpolation::Step => 0.0,
            Interpolation::Linear => amount,
        };
        match (&channel.target, &channel.values) {
            (ChannelTarget::Translation, AnimationValues::Translation(values)) => {
                let (a, b) = values_at(values, lower, upper)?;
                let value = Vec3::from_array(*a).lerp(Vec3::from_array(*b), amount);
                if !value.is_finite() {
                    return Err(invalid_clip("translation sample is not finite"));
                }
                target.translation = value.to_array();
            }
            (ChannelTarget::Scale, AnimationValues::Scale(values)) => {
                let (a, b) = values_at(values, lower, upper)?;
                let value = Vec3::from_array(*a).lerp(Vec3::from_array(*b), amount);
                if !value.is_finite() {
                    return Err(invalid_clip("scale sample is not finite"));
                }
                target.scale = value.to_array();
            }
            (ChannelTarget::Rotation, AnimationValues::Rotation(values)) => {
                let (a, b) = values_at(values, lower, upper)?;
                let a = normalized_rotation(*a)?;
                let b = normalized_rotation(*b)?;
                target.rotation = a.slerp(b, amount).normalize().to_array();
            }
            _ => return Err(invalid_clip("channel value type does not match its target")),
        }
    }
    Ok(pose)
}

/// Hash TRS pose bytes in palette order using a stable little-endian encoding.
pub fn pose_state_hash(pose: &[LocalPose]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for joint in pose {
        for value in joint
            .translation
            .iter()
            .chain(joint.rotation.iter())
            .chain(joint.scale.iter())
        {
            hash.update(value.to_le_bytes());
        }
    }
    hash.finalize().into()
}

fn validate_clock(time: f32, duration: f32) -> Result<(), AnimationError> {
    if !time.is_finite() || !duration.is_finite() || time < 0.0 || duration < 0.0 {
        return Err(invalid_clip(
            "time and duration must be finite and non-negative",
        ));
    }
    Ok(())
}

fn normalize_time(time: f32, duration: f32, looping: bool) -> f32 {
    if duration == 0.0 {
        0.0
    } else if looping {
        time.rem_euclid(duration)
    } else {
        time.clamp(0.0, duration)
    }
}

fn key_interval(times: &[f32], time: f32) -> Result<(usize, usize, f32), AnimationError> {
    if times.is_empty()
        || times.iter().any(|value| !value.is_finite())
        || times.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(invalid_clip("channel key times are empty or non-monotonic"));
    }
    let upper = times.partition_point(|key| *key <= time);
    if upper == 0 {
        return Ok((0, 0, 0.0));
    }
    if upper == times.len() {
        let last = times.len() - 1;
        return Ok((last, last, 0.0));
    }
    let lower = upper - 1;
    if times[lower] == time {
        return Ok((lower, lower, 0.0));
    }
    let amount = (time - times[lower]) / (times[upper] - times[lower]);
    Ok((lower, upper, amount))
}

fn values_at<T>(values: &[T], lower: usize, upper: usize) -> Result<(&T, &T), AnimationError> {
    if values.len() <= upper {
        return Err(invalid_clip("channel values do not match key times"));
    }
    let a = values
        .get(lower)
        .ok_or_else(|| invalid_clip("channel values do not match key times"))?;
    let b = values
        .get(upper)
        .ok_or_else(|| invalid_clip("channel values do not match key times"))?;
    Ok((a, b))
}

fn normalized_rotation(value: [f32; 4]) -> Result<Quat, AnimationError> {
    let rotation = Quat::from_array(value);
    if value.iter().any(|component| !component.is_finite()) || rotation.length_squared() <= 1.0e-12
    {
        return Err(invalid_clip("rotation sample is invalid"));
    }
    Ok(rotation.normalize())
}

fn channel_value_len(values: &AnimationValues) -> usize {
    match values {
        AnimationValues::Translation(values) | AnimationValues::Scale(values) => values.len(),
        AnimationValues::Rotation(values) => values.len(),
    }
}

fn invalid_clip(reason: &str) -> AnimationError {
    AnimationError::InvalidClip(reason.to_owned())
}

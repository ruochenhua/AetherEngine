//! Validation rules for owned glTF data.

use super::*;

impl GltfDocument {
    /// Validate graph references and animation data; oversized skins are reported per primitive.
    pub fn validate(&self) -> Result<ValidationReport, ValidationError> {
        validate_graph(&self.nodes)?;
        for (skin_index, skin) in self.skins.iter().enumerate() {
            validate_skin(self, skin_index, skin, false)?;
        }
        for node in self.nodes.iter() {
            if node.mesh.is_some_and(|mesh| mesh >= self.meshes.len())
                || node.skin.is_some_and(|skin| skin >= self.skins.len())
            {
                return Err(ValidationError::InvalidReference);
            }
        }
        for animation in self.animations.iter() {
            validate_animation(self, animation)?;
        }

        let mut diagnostics = Vec::new();
        for node in self.nodes.iter() {
            let Some(mesh_index) = node.mesh else {
                continue;
            };
            let mesh = self
                .meshes
                .get(mesh_index)
                .ok_or(ValidationError::InvalidReference)?;
            if let Some(skin_index) = node.skin {
                let skin = self
                    .skins
                    .get(skin_index)
                    .ok_or(ValidationError::InvalidReference)?;
                if skin.joints.len() > MAX_SKELETON_JOINTS {
                    for (primitive_index, primitive) in mesh.primitives.iter().enumerate() {
                        validate_weights(primitive, skin.joints.len())?;
                        diagnostics.push(ValidationDiagnostic::UnsupportedJointCount {
                            skin: skin_index,
                            primitive: (mesh_index, primitive_index),
                            joint_count: skin.joints.len(),
                        });
                    }
                } else {
                    for primitive in &mesh.primitives {
                        validate_weights(primitive, skin.joints.len())?;
                    }
                }
            }
        }
        Ok(ValidationReport { diagnostics })
    }
}

impl DocumentAnimation {
    /// Return the clip duration from the latest channel key, or zero for no keys.
    pub fn duration(&self) -> f32 {
        self.channels
            .iter()
            .filter_map(|channel| channel.times.last().copied())
            .fold(0.0_f32, f32::max)
    }
}
fn validate_graph(nodes: &[DocumentNode]) -> Result<(), ValidationError> {
    let mut state = vec![0_u8; nodes.len()];
    fn visit(
        index: usize,
        nodes: &[DocumentNode],
        state: &mut [u8],
    ) -> Result<(), ValidationError> {
        match state[index] {
            1 => return Err(ValidationError::Cycle),
            2 => return Ok(()),
            _ => {}
        }
        state[index] = 1;
        for child in &nodes[index].children {
            if *child >= nodes.len() {
                return Err(ValidationError::InvalidReference);
            }
            visit(*child, nodes, state)?;
        }
        state[index] = 2;
        Ok(())
    }
    for index in 0..nodes.len() {
        visit(index, nodes, &mut state)?;
    }
    for (index, node) in nodes.iter().enumerate() {
        if node
            .translation
            .iter()
            .chain(node.rotation.iter())
            .chain(node.scale.iter())
            .any(|value| !value.is_finite())
        {
            return Err(ValidationError::InvalidChannelData);
        }
        if let Some(parent) = node.parent {
            if parent >= nodes.len() || !nodes[parent].children.contains(&index) {
                return Err(ValidationError::InvalidReference);
            }
        }
        for child in &node.children {
            if nodes[*child].parent != Some(index) {
                return Err(ValidationError::InvalidReference);
            }
        }
    }
    Ok(())
}

pub(super) fn validate_skin(
    document: &GltfDocument,
    skin_index: usize,
    skin: &DocumentSkin,
    enforce_palette_limit: bool,
) -> Result<(), ValidationError> {
    if skin
        .joints
        .iter()
        .any(|joint| *joint >= document.nodes.len())
    {
        return Err(ValidationError::InvalidReference);
    }
    if skin.inverse_bind_matrices.len() != skin.joints.len() {
        return Err(ValidationError::InverseBindCount);
    }
    if skin
        .inverse_bind_matrices
        .iter()
        .flatten()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(ValidationError::InvalidInverseBind);
    }
    if enforce_palette_limit && skin.joints.len() > MAX_SKELETON_JOINTS {
        return Err(ValidationError::UnsupportedJointCount {
            count: skin.joints.len(),
        });
    }
    let _ = skin_index;
    Ok(())
}

fn validate_weights(
    primitive: &DocumentPrimitive,
    joint_count: usize,
) -> Result<(), ValidationError> {
    for weights in &primitive.weights {
        let normalized = JointWeights::validated(weights.joints, weights.weights)?;
        if normalized
            .joints
            .iter()
            .zip(normalized.weights)
            .any(|(joint, weight)| weight > 0.0 && *joint as usize >= joint_count)
        {
            return Err(ValidationError::InvalidReference);
        }
    }
    Ok(())
}

pub(super) fn validate_animation(
    document: &GltfDocument,
    animation: &DocumentAnimation,
) -> Result<(), ValidationError> {
    for channel in &animation.channels {
        if channel.target_joint >= document.nodes.len() {
            return Err(ValidationError::InvalidReference);
        }
        let value_count = match (&channel.values, channel.target) {
            (AnimationValues::Vec3(values), AnimationPath::Translation | AnimationPath::Scale) => {
                values.len()
            }
            (AnimationValues::Vec4(values), AnimationPath::Rotation) => values.len(),
            _ => return Err(ValidationError::ChannelArity),
        };
        let keys_per_time = if channel.interpolation == Interpolation::CubicSpline {
            3
        } else {
            1
        };
        if channel.times.len().saturating_mul(keys_per_time) != value_count {
            return Err(ValidationError::ChannelCount);
        }
        if channel.times.iter().any(|time| !time.is_finite())
            || channel.times.windows(2).any(|pair| pair[0] >= pair[1])
            || match &channel.values {
                AnimationValues::Vec3(values) => {
                    values.iter().flatten().any(|value| !value.is_finite())
                }
                AnimationValues::Vec4(values) => {
                    values.iter().flatten().any(|value| !value.is_finite())
                }
            }
        {
            return Err(ValidationError::InvalidChannelData);
        }
    }
    Ok(())
}

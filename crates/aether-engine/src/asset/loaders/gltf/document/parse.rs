use super::types::{
    normalize_weights, AnimationValues, ChannelTarget, GltfAnimation, GltfAnimationChannel,
    GltfDocumentAsset, GltfError, GltfImage, GltfImageFormat, GltfMesh, GltfNode, GltfPrimitive,
    GltfSkin, Interpolation,
};
use glam::Mat4;
use std::path::Path;
use std::sync::Arc;

/// Import a glTF document into CPU-owned data and validate T7.1 invariants.
pub fn load_document(path: &Path) -> Result<GltfDocumentAsset, GltfError> {
    let (document, buffers, images) = gltf::import(path)
        .map_err(|error| GltfError::Parse(format!("{}: {error}", path.display())))?;

    let nodes = parse_nodes(&document)?;
    let meshes = parse_meshes(&document, &buffers)?;
    let skins = parse_skins(&document, &buffers, nodes.len())?;
    validate_skinned_mesh_links(&nodes, &meshes, &skins)?;
    let animations = parse_animations(&document, &buffers)?;
    let images = document
        .images()
        .zip(images)
        .map(|(image, data)| {
            Arc::new(GltfImage {
                name: image.name().map(String::from),
                pixels: Arc::from(data.pixels),
                format: map_image_format(data.format),
                width: data.width,
                height: data.height,
            })
        })
        .collect();
    let buffers = buffers
        .into_iter()
        .map(|buffer| Arc::from(buffer.0))
        .collect();

    Ok(GltfDocumentAsset {
        buffers,
        images,
        nodes,
        meshes,
        skins,
        animations,
    })
}

fn parse_nodes(document: &gltf::Document) -> Result<Vec<Arc<GltfNode>>, GltfError> {
    let source_nodes: Vec<_> = document.nodes().collect();
    let node_count = source_nodes.len();
    let mut parents = vec![None; node_count];
    let mut child_lists = vec![Vec::new(); node_count];

    for node in &source_nodes {
        let parent = node.index();
        for child in node.children() {
            let child_index = child.index();
            if child_index >= node_count {
                return Err(GltfError::InvalidReference(format!(
                    "node {parent} references child {child_index}"
                )));
            }
            if parents[child_index].replace(parent).is_some() {
                return Err(GltfError::InvalidReference(format!(
                    "node {child_index} has multiple parents"
                )));
            }
            child_lists[parent].push(child_index as u32);
        }
    }
    validate_acyclic(&parents)?;

    source_nodes
        .into_iter()
        .map(|node| {
            let (translation, rotation, scale) = node.transform().decomposed();
            let local_matrix = node.transform().matrix();
            if local_matrix
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
                || translation.iter().any(|value| !value.is_finite())
                || rotation.iter().any(|value| !value.is_finite())
                || scale.iter().any(|value| !value.is_finite())
            {
                return Err(GltfError::InvalidReference(format!(
                    "node {} has a non-finite transform",
                    node.index()
                )));
            }
            Ok(Arc::new(GltfNode {
                name: node.name().map(String::from),
                parent: parents[node.index()].map(|index| index as u32),
                children: Arc::from(child_lists[node.index()].clone()),
                mesh: node.mesh().map(|mesh| mesh.index() as u32),
                skin: node.skin().map(|skin| skin.index() as u32),
                local_matrix,
                translation,
                rotation,
                scale,
            }))
        })
        .collect()
}

fn validate_acyclic(parents: &[Option<usize>]) -> Result<(), GltfError> {
    let mut states = vec![0_u8; parents.len()];
    for start in 0..parents.len() {
        if states[start] == 2 {
            continue;
        }
        let mut trail = Vec::new();
        let mut current = Some(start);
        while let Some(index) = current {
            match states[index] {
                1 => return Err(GltfError::Cycle),
                2 => break,
                _ => {
                    states[index] = 1;
                    trail.push(index);
                    current = parents[index];
                }
            }
        }
        for index in trail {
            states[index] = 2;
        }
    }
    Ok(())
}

fn parse_meshes(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Result<Vec<Arc<GltfMesh>>, GltfError> {
    document
        .meshes()
        .map(|mesh| {
            let primitives = mesh
                .primitives()
                .map(|primitive| parse_primitive(primitive, buffers))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Arc::new(GltfMesh {
                name: mesh.name().map(String::from),
                primitives: Arc::from(primitives),
            }))
        })
        .collect()
}

fn parse_primitive(
    primitive: gltf::Primitive,
    buffers: &[gltf::buffer::Data],
) -> Result<GltfPrimitive, GltfError> {
    if primitive.mode() != gltf::mesh::Mode::Triangles {
        return Err(GltfError::UnsupportedPrimitiveMode(format!(
            "primitive mode {:?}; T7.1 supports triangle lists",
            primitive.mode()
        )));
    }
    if primitive.attributes().any(|(semantic, _)| {
        matches!(
            semantic,
            gltf::Semantic::Joints(set) | gltf::Semantic::Weights(set) if set > 0
        )
    }) {
        return Err(GltfError::UnsupportedInfluenceCount(
            "more than four vertex influences".into(),
        ));
    }

    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    let positions: Arc<[[f32; 3]]> = Arc::from(
        reader
            .read_positions()
            .ok_or_else(|| GltfError::InvalidReference("primitive has no POSITION".into()))?
            .collect::<Vec<_>>(),
    );
    if positions.is_empty() {
        return Err(GltfError::InvalidReference(
            "primitive has an empty POSITION stream".into(),
        ));
    }
    let normals: Option<Arc<[[f32; 3]]>> = reader
        .read_normals()
        .map(|values| Arc::from(values.collect::<Vec<_>>()));
    let tangents: Option<Arc<[[f32; 4]]>> = reader
        .read_tangents()
        .map(|values| Arc::from(values.collect::<Vec<_>>()));
    let tex_coords: Option<Arc<[[f32; 2]]>> = reader
        .read_tex_coords(0)
        .map(|values| Arc::from(values.into_f32().collect::<Vec<_>>()));
    if normals
        .as_ref()
        .is_some_and(|values| values.len() != positions.len())
        || tangents
            .as_ref()
            .is_some_and(|values| values.len() != positions.len())
        || tex_coords
            .as_ref()
            .is_some_and(|values| values.len() != positions.len())
    {
        return Err(GltfError::ChannelArityMismatch);
    }

    let joints: Option<Arc<[[u32; 4]]>> = reader.read_joints(0).map(|values| {
        Arc::from(
            values
                .into_u16()
                .map(|joint| joint.map(u32::from))
                .collect::<Vec<_>>(),
        )
    });
    let mut weights: Option<Vec<[f32; 4]>> = reader
        .read_weights(0)
        .map(|values| values.into_f32().collect::<Vec<_>>());
    if let Some(values) = &mut weights {
        normalize_weights(values)?;
    }
    let weights: Option<Arc<[[f32; 4]]>> = weights.map(Arc::from);
    if joints
        .as_ref()
        .is_some_and(|values| values.len() != positions.len())
        || weights
            .as_ref()
            .is_some_and(|values| values.len() != positions.len())
        || joints.is_some() != weights.is_some()
    {
        return Err(GltfError::ChannelArityMismatch);
    }

    let indices = reader
        .read_indices()
        .map(|values| values.into_u32().collect::<Vec<_>>())
        .unwrap_or_else(|| (0..positions.len() as u32).collect());
    Ok(GltfPrimitive {
        positions,
        normals,
        tangents,
        tex_coords,
        indices: Arc::from(indices),
        joints,
        weights,
    })
}

fn parse_skins(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    node_count: usize,
) -> Result<Vec<Arc<GltfSkin>>, GltfError> {
    document
        .skins()
        .map(|skin| {
            let joints: Vec<_> = skin.joints().map(|node| node.index() as u32).collect();
            if joints.iter().any(|joint| *joint as usize >= node_count) {
                return Err(GltfError::InvalidReference(format!(
                    "skin {} references a node outside the document",
                    skin.index()
                )));
            }
            let has_inverse_bind_accessor = skin.inverse_bind_matrices().is_some();
            let decoded_inverse_bind = skin
                .reader(|buffer| Some(&buffers[buffer.index()]))
                .read_inverse_bind_matrices()
                .map(|matrices| {
                    matrices
                        .map(|matrix| Mat4::from_cols_array_2d(&matrix))
                        .collect::<Vec<_>>()
                });
            if has_inverse_bind_accessor && decoded_inverse_bind.is_none() {
                return Err(GltfError::InvalidInverseBind);
            }
            let inverse_bind =
                decoded_inverse_bind.unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
            if inverse_bind.len() != joints.len()
                || inverse_bind.iter().any(|matrix| {
                    matrix
                        .to_cols_array()
                        .iter()
                        .any(|value| !value.is_finite())
                })
            {
                return Err(GltfError::InvalidInverseBind);
            }
            let skeleton = skin.skeleton().map(|node| node.index() as u32);
            if skeleton.is_some_and(|node| node as usize >= node_count) {
                return Err(GltfError::InvalidReference(format!(
                    "skin {} has an invalid skeleton root",
                    skin.index()
                )));
            }
            Ok(Arc::new(GltfSkin {
                name: skin.name().map(String::from),
                skeleton,
                joints: Arc::from(joints),
                inverse_bind: Arc::from(inverse_bind),
            }))
        })
        .collect()
}

fn validate_skinned_mesh_links(
    nodes: &[Arc<GltfNode>],
    meshes: &[Arc<GltfMesh>],
    skins: &[Arc<GltfSkin>],
) -> Result<(), GltfError> {
    for (node_index, node) in nodes.iter().enumerate() {
        let mesh = node
            .mesh
            .map(|mesh_index| {
                meshes.get(mesh_index as usize).ok_or_else(|| {
                    GltfError::InvalidReference(format!(
                        "node {node_index} references mesh {mesh_index}"
                    ))
                })
            })
            .transpose()?;
        let skin = node
            .skin
            .map(|skin_index| {
                skins.get(skin_index as usize).ok_or_else(|| {
                    GltfError::InvalidReference(format!(
                        "node {node_index} references skin {skin_index}"
                    ))
                })
            })
            .transpose()?;
        if skin.is_some() && mesh.is_none() {
            return Err(GltfError::InvalidReference(format!(
                "node {node_index} has a skin assignment without a mesh"
            )));
        }
        let (Some(mesh), Some(skin)) = (mesh, skin) else {
            continue;
        };
        for (primitive_index, primitive) in mesh.primitives.iter().enumerate() {
            let (Some(joints), Some(_weights)) = (&primitive.joints, &primitive.weights) else {
                return Err(GltfError::InvalidReference(format!(
                    "skinned node {node_index} mesh primitive {primitive_index} lacks JOINTS_0/WEIGHTS_0"
                )));
            };
            if joints
                .iter()
                .flatten()
                .any(|joint| *joint as usize >= skin.joints.len())
            {
                return Err(GltfError::InvalidReference(format!(
                    "node {node_index} mesh primitive {primitive_index} references a joint outside its skin"
                )));
            }
        }
    }
    Ok(())
}

fn parse_animations(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Result<Vec<Arc<GltfAnimation>>, GltfError> {
    document
        .animations()
        .map(|animation| {
            let mut duration = 0.0_f32;
            let channels = animation
                .channels()
                .map(|channel| {
                    let sampler = channel.sampler();
                    let interpolation = match sampler.interpolation() {
                        gltf::animation::Interpolation::Step => Interpolation::Step,
                        gltf::animation::Interpolation::Linear => Interpolation::Linear,
                        unsupported => {
                            return Err(GltfError::UnsupportedAnimation(format!(
                                "interpolation {unsupported:?}"
                            )))
                        }
                    };
                    let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
                    let times = reader
                        .read_inputs()
                        .ok_or(GltfError::ChannelArityMismatch)?
                        .collect::<Vec<_>>();
                    validate_times(&times)?;
                    duration = duration.max(times.last().copied().unwrap_or(0.0));
                    let values = match reader
                        .read_outputs()
                        .ok_or(GltfError::ChannelArityMismatch)?
                    {
                        gltf::animation::util::ReadOutputs::Translations(values) => {
                            AnimationValues::Translation(Arc::from(values.collect::<Vec<_>>()))
                        }
                        gltf::animation::util::ReadOutputs::Rotations(values) => {
                            AnimationValues::Rotation(Arc::from(
                                values.into_f32().collect::<Vec<_>>(),
                            ))
                        }
                        gltf::animation::util::ReadOutputs::Scales(values) => {
                            AnimationValues::Scale(Arc::from(values.collect::<Vec<_>>()))
                        }
                        gltf::animation::util::ReadOutputs::MorphTargetWeights(_) => {
                            return Err(GltfError::UnsupportedAnimation(
                                "morph target weights".into(),
                            ))
                        }
                    };
                    let target = match channel.target().property() {
                        gltf::animation::Property::Translation => ChannelTarget::Translation,
                        gltf::animation::Property::Rotation => ChannelTarget::Rotation,
                        gltf::animation::Property::Scale => ChannelTarget::Scale,
                        gltf::animation::Property::MorphTargetWeights => {
                            return Err(GltfError::UnsupportedAnimation(
                                "morph target weights".into(),
                            ))
                        }
                    };
                    if !channel_values_match(target, &values, times.len()) {
                        return Err(GltfError::ChannelArityMismatch);
                    }
                    if channel_values_non_finite(&values) {
                        return Err(GltfError::ChannelArityMismatch);
                    }
                    Ok(GltfAnimationChannel {
                        target_node: channel.target().node().index() as u32,
                        target,
                        times: Arc::from(times),
                        values,
                        interpolation,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Arc::new(GltfAnimation {
                name: animation.name().map(String::from),
                channels: Arc::from(channels),
                duration,
            }))
        })
        .collect()
}

fn validate_times(times: &[f32]) -> Result<(), GltfError> {
    if times.iter().any(|time| !time.is_finite()) || times.windows(2).any(|pair| pair[1] <= pair[0])
    {
        return Err(GltfError::NonMonotonicTime);
    }
    Ok(())
}

fn channel_values_match(target: ChannelTarget, values: &AnimationValues, count: usize) -> bool {
    match (target, values) {
        (ChannelTarget::Translation, AnimationValues::Translation(values))
        | (ChannelTarget::Scale, AnimationValues::Scale(values)) => values.len() == count,
        (ChannelTarget::Rotation, AnimationValues::Rotation(values)) => values.len() == count,
        _ => false,
    }
}

fn channel_values_non_finite(values: &AnimationValues) -> bool {
    match values {
        AnimationValues::Translation(values) | AnimationValues::Scale(values) => {
            values.iter().flatten().any(|value| !value.is_finite())
        }
        AnimationValues::Rotation(values) => {
            values.iter().flatten().any(|value| !value.is_finite())
        }
    }
}

fn map_image_format(format: gltf::image::Format) -> GltfImageFormat {
    match format {
        gltf::image::Format::R8 => GltfImageFormat::R8,
        gltf::image::Format::R8G8 => GltfImageFormat::Rg8,
        gltf::image::Format::R8G8B8 => GltfImageFormat::Rgb8,
        gltf::image::Format::R8G8B8A8 => GltfImageFormat::Rgba8,
        gltf::image::Format::R16 => GltfImageFormat::R16,
        gltf::image::Format::R16G16 => GltfImageFormat::Rg16,
        gltf::image::Format::R16G16B16 => GltfImageFormat::Rgb16,
        gltf::image::Format::R16G16B16A16 => GltfImageFormat::Rgba16,
        gltf::image::Format::R32G32B32FLOAT => GltfImageFormat::Rgb32Float,
        gltf::image::Format::R32G32B32A32FLOAT => GltfImageFormat::Rgba32Float,
    }
}

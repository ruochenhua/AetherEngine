//! glTF file decoding into owned runtime data.

use super::*;

impl Asset for GltfDocument {
    fn load(path: &Path) -> anyhow::Result<Self> {
        let (document, buffers, images) = gltf::import(path)?;
        let buffers: Arc<Vec<Arc<[u8]>>> = Arc::new(
            buffers
                .into_iter()
                .map(|buffer| Arc::<[u8]>::from(buffer.0))
                .collect(),
        );
        let images: Arc<Vec<Arc<[u8]>>> = Arc::new(
            images
                .into_iter()
                .map(|image| Arc::<[u8]>::from(image.pixels))
                .collect(),
        );
        let mut nodes = document
            .nodes()
            .map(|node| {
                let (translation, rotation, scale) = node.transform().decomposed();
                DocumentNode {
                    children: node.children().map(|child| child.index()).collect(),
                    parent: None,
                    mesh: node.mesh().map(|mesh| mesh.index()),
                    skin: node.skin().map(|skin| skin.index()),
                    translation,
                    rotation,
                    scale,
                }
            })
            .collect::<Vec<_>>();
        for parent in 0..nodes.len() {
            for child in nodes[parent].children.clone() {
                if let Some(node) = nodes.get_mut(child) {
                    node.parent = Some(parent);
                }
            }
        }
        let meshes = document
            .meshes()
            .map(|mesh| -> anyhow::Result<DocumentMesh> {
                Ok(DocumentMesh {
                    primitives: mesh
                        .primitives()
                        .map(|primitive| -> anyhow::Result<DocumentPrimitive> {
                            let reader = primitive
                                .reader(|buffer| buffers.get(buffer.index()).map(Arc::as_ref));
                            let geometry_reader = primitive
                                .reader(|buffer| buffers.get(buffer.index()).map(Arc::as_ref));
                            let positions = geometry_reader
                                .read_positions()
                                .map(|iter| iter.collect::<Vec<_>>())
                                .unwrap_or_default();
                            let normals = geometry_reader
                                .read_normals()
                                .map(|iter| iter.collect::<Vec<_>>())
                                .unwrap_or_default();
                            let uvs = geometry_reader
                                .read_tex_coords(0)
                                .map(|iter| iter.into_f32().collect::<Vec<_>>())
                                .unwrap_or_default();
                            let tangents = geometry_reader
                                .read_tangents()
                                .map(|iter| iter.collect::<Vec<_>>())
                                .unwrap_or_default();
                            let indices = geometry_reader
                                .read_indices()
                                .map(|iter| iter.into_u32().collect::<Vec<_>>())
                                .unwrap_or_else(|| (0..positions.len() as u32).collect());
                            let mesh = (!positions.is_empty()).then_some(CpuMesh {
                                positions,
                                normals,
                                uvs,
                                tangents,
                                indices,
                                submeshes: Vec::new(),
                            });
                            let joints = reader
                                .read_joints(0)
                                .map(|values| values.into_u16().collect::<Vec<_>>());
                            let weights = reader
                                .read_weights(0)
                                .map(|values| values.into_f32().collect::<Vec<_>>());
                            let vertex_weights = match (joints, weights) {
                                (Some(joints), Some(weights)) if joints.len() == weights.len() => {
                                    joints
                                        .into_iter()
                                        .zip(weights)
                                        .map(|(joint, weight)| {
                                            JointWeights::validated(joint, weight)
                                                .map_err(anyhow::Error::from)
                                        })
                                        .collect::<anyhow::Result<Vec<_>>>()?
                                }
                                (None, None) => Vec::new(),
                                _ => {
                                    anyhow::bail!(
                                        "glTF JOINTS_0 and WEIGHTS_0 accessors do not match"
                                    )
                                }
                            };
                            Ok(DocumentPrimitive {
                                mesh,
                                weights: vertex_weights,
                            })
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let skins = document
            .skins()
            .map(|skin| {
                let inverse_bind_matrices = skin
                    .reader(|buffer| buffers.get(buffer.index()).map(Arc::as_ref))
                    .read_inverse_bind_matrices()
                    .map(|matrices| matrices.collect::<Vec<_>>())
                    .unwrap_or_default();
                DocumentSkin {
                    joints: skin.joints().map(|joint| joint.index()).collect(),
                    inverse_bind_matrices,
                }
            })
            .collect::<Vec<_>>();
        let animations = document
            .animations()
            .map(|animation| -> anyhow::Result<DocumentAnimation> {
                Ok(DocumentAnimation {
                    channels: animation
                        .channels()
                        .map(|channel| -> anyhow::Result<AnimationChannel> {
                            let reader = channel
                                .reader(|buffer| buffers.get(buffer.index()).map(Arc::as_ref));
                            let times = reader
                                .read_inputs()
                                .ok_or_else(|| {
                                    anyhow::anyhow!("animation input accessor is missing")
                                })?
                                .collect::<Vec<_>>();
                            let outputs = reader.read_outputs().ok_or_else(|| {
                                anyhow::anyhow!("animation output accessor is missing")
                            })?;
                            let (target, values) = match channel.target().property() {
                                gltf::animation::Property::Translation => {
                                    let gltf::animation::util::ReadOutputs::Translations(values) =
                                        outputs
                                    else {
                                        anyhow::bail!(
                                            "translation channel has incompatible output type"
                                        );
                                    };
                                    (
                                        AnimationPath::Translation,
                                        AnimationValues::Vec3(values.collect::<Vec<_>>().into()),
                                    )
                                }
                                gltf::animation::Property::Rotation => {
                                    let gltf::animation::util::ReadOutputs::Rotations(values) =
                                        outputs
                                    else {
                                        anyhow::bail!(
                                            "rotation channel has incompatible output type"
                                        );
                                    };
                                    (
                                        AnimationPath::Rotation,
                                        AnimationValues::Vec4(
                                            values.into_f32().collect::<Vec<_>>().into(),
                                        ),
                                    )
                                }
                                gltf::animation::Property::Scale => {
                                    let gltf::animation::util::ReadOutputs::Scales(values) =
                                        outputs
                                    else {
                                        anyhow::bail!("scale channel has incompatible output type");
                                    };
                                    (
                                        AnimationPath::Scale,
                                        AnimationValues::Vec3(values.collect::<Vec<_>>().into()),
                                    )
                                }
                                _ => anyhow::bail!("animation property is not TRS"),
                            };
                            let interpolation = match channel.sampler().interpolation() {
                                gltf::animation::Interpolation::Step => Interpolation::Step,
                                gltf::animation::Interpolation::Linear => Interpolation::Linear,
                                gltf::animation::Interpolation::CubicSpline => {
                                    Interpolation::CubicSpline
                                }
                            };
                            Ok(AnimationChannel {
                                target_joint: channel.target().node().index(),
                                target,
                                interpolation,
                                times: times.into(),
                                values,
                            })
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let owned = Self {
            buffers,
            images,
            nodes: Arc::new(nodes),
            meshes: Arc::new(meshes),
            skins: Arc::new(skins),
            animations: Arc::new(animations),
        };
        owned.validate()?;
        Ok(owned)
    }
}

impl TypedAsset for GltfDocument {
    const KIND: AssetKind = AssetKind::GltfDocument;
}

impl Asset for SkeletonAsset {
    fn load(_path: &Path) -> anyhow::Result<Self> {
        anyhow::bail!("SkeletonAsset must be derived from a validated GltfDocument")
    }
}

impl TypedAsset for SkeletonAsset {
    const KIND: AssetKind = AssetKind::Skeleton;
}

impl Asset for AnimationClipAsset {
    fn load(_path: &Path) -> anyhow::Result<Self> {
        anyhow::bail!("AnimationClipAsset must be derived from a validated GltfDocument")
    }
}

impl TypedAsset for AnimationClipAsset {
    const KIND: AssetKind = AssetKind::AnimationClip;
}

#[cfg(test)]
mod tests;

//! Scene-local glTF animation extraction for the launcher frame boundary.

use aether_engine::animation::{
    extract_animation_frame, AnimationAssets, AnimationCameraFrame, AnimationContext,
    AnimationError, AnimationFrame, AnimationPrimitive, AnimatorRuntime, AnimatorState, MeshRef,
    SceneSnapshot,
};
use aether_engine::asset::gltf_document::{
    AnimationClipAsset, GltfDocument, SkeletonAsset, MAX_SKELETON_JOINTS,
};
use aether_engine::asset::store::AssetKind;
use aether_engine::asset::{AssetManager, Handle};
use aether_engine::ecs::components::{MeshHandle, MeshSource, Name, Transform, Visibility};
use aether_engine::ecs::{Entity, World};
use aether_engine::renderer::camera::FlyCamera;
use glam::Mat4;
use std::collections::HashMap;
use std::sync::Arc;

const DOCUMENT_HANDLE: u64 = u64::MAX;
const SKELETON_HANDLE: u64 = u64::MAX - 1;
const CLIP_HANDLE: u64 = u64::MAX - 2;

struct SceneAnimationAssets {
    document: Arc<GltfDocument>,
    skeleton: Arc<SkeletonAsset>,
    clip: Arc<AnimationClipAsset>,
}

impl AnimationAssets for SceneAnimationAssets {
    fn document(&self, _handle: Handle<GltfDocument>) -> Result<Arc<GltfDocument>, AnimationError> {
        Ok(self.document.clone())
    }

    fn skeleton(
        &self,
        _handle: Handle<SkeletonAsset>,
    ) -> Result<Arc<SkeletonAsset>, AnimationError> {
        Ok(self.skeleton.clone())
    }

    fn clip(
        &self,
        _handle: Handle<AnimationClipAsset>,
    ) -> Result<Arc<AnimationClipAsset>, AnimationError> {
        Ok(self.clip.clone())
    }
}

/// Active single-document animation extracted from a scene's imported glTF actor.
pub(crate) struct SceneAnimation {
    assets: SceneAnimationAssets,
    primitives: Vec<AnimationPrimitive>,
    runtime: AnimatorRuntime,
    controls_groups: HashMap<u64, Arc<[u64]>>,
    generation: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct AnimatorControls {
    pub(crate) time: f32,
    pub(crate) duration: f32,
    pub(crate) speed: f32,
    pub(crate) looping: bool,
    pub(crate) playing: bool,
}

pub(crate) struct AnimationFrameInput<'a> {
    pub(crate) camera: &'a FlyCamera,
    pub(crate) aspect: f32,
    pub(crate) frame_id: u64,
    pub(crate) delta_time: f32,
    pub(crate) paused: bool,
    pub(crate) seek_time: Option<f32>,
}

impl SceneAnimation {
    pub(crate) fn controls(&self, entity_bits: u64) -> Option<AnimatorControls> {
        let state = self.runtime.states.get(&entity_bits)?;
        let duration = self.assets.clip.duration;
        let time = if state.looping && duration.is_finite() && duration > 0.0 {
            state.time.rem_euclid(duration)
        } else {
            state.time.clamp(0.0, duration.max(0.0))
        };
        Some(AnimatorControls {
            time,
            duration,
            speed: state.speed,
            looping: state.looping,
            playing: state.playing,
        })
    }

    pub(crate) fn apply_controls(&mut self, entity_bits: u64, controls: AnimatorControls) {
        let entities = self
            .controls_groups
            .get(&entity_bits)
            .map_or_else(|| Arc::from([entity_bits]), Arc::clone);
        for entity in entities.iter() {
            if let Some(state) = self.runtime.states.get_mut(entity) {
                state.time = controls.time.clamp(0.0, controls.duration.max(0.0));
                state.speed = controls.speed;
                state.looping = controls.looping;
                state.playing = controls.playing;
            }
        }
    }

    /// Load the first skinned, animated glTF mesh referenced by the current scene.
    pub(crate) fn from_world(
        world: &World,
        manager: &mut AssetManager,
    ) -> anyhow::Result<Option<Self>> {
        let mut paths = world
            .query::<&MeshHandle>()
            .iter()
            .filter_map(|mesh| match &mesh.source {
                MeshSource::File(path) if path.ends_with(".gltf") || path.ends_with(".glb") => {
                    Some(path.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        for path in paths {
            let Ok(document_handle) = manager.load::<GltfDocument>(&path) else {
                continue;
            };
            let Some(document) = manager.get(document_handle.clone()) else {
                continue;
            };
            let mut matching_query = world.query::<(
                Entity,
                &MeshHandle,
                &Transform,
                Option<&Visibility>,
                Option<&Name>,
            )>();
            let matching_entities = matching_query
                .iter()
                .filter_map(|(entity, mesh, transform, visibility, name)| {
                    (mesh.source == MeshSource::File(path.clone())
                        && visibility.map_or(true, |visibility| visibility.0))
                    .then_some((
                        mesh.source_node_index?,
                        mesh.source_primitive_index?,
                        entity.to_bits().get(),
                        world_transform(transform),
                        name.map(|name| {
                            name.0
                                .rsplit_once("::")
                                .map_or(name.0.as_str(), |(actor, _)| actor)
                                .to_string()
                        })
                        .unwrap_or_else(|| format!("entity:{}", entity.to_bits().get())),
                    ))
                })
                .collect::<Vec<_>>();
            let Some((node_index, skin_index, mesh_index)) =
                document.nodes.iter().enumerate().find_map(|(index, node)| {
                    let (skin_index, mesh_index) = (node.skin?, node.mesh?);
                    matching_entities
                        .iter()
                        .any(|(source_node, _, _, _, _)| *source_node == index)
                        .then_some((index, skin_index, mesh_index))
                })
            else {
                continue;
            };
            if document.animations.is_empty() {
                continue;
            }
            let skin = document
                .skins
                .get(skin_index)
                .ok_or_else(|| anyhow::anyhow!("glTF actor skin index is invalid"))?;
            if skin.joints.len() > MAX_SKELETON_JOINTS {
                continue;
            }
            let mut skeleton_id = manager.asset_id(document_handle.clone())?;
            skeleton_id.kind = AssetKind::Skeleton;
            let skeleton = Arc::new(SkeletonAsset::from_skin_with_source(
                &document,
                skin_index,
                skeleton_id,
            )?);
            let clip = Arc::new(AnimationClipAsset::from_animation(&document, 0)?);
            let document_id = manager.asset_id(document_handle.clone())?;
            let mut mesh_id = document_id.clone();
            mesh_id.kind = AssetKind::CpuMesh;

            let document_mesh = document
                .meshes
                .get(mesh_index)
                .ok_or_else(|| anyhow::anyhow!("glTF actor mesh index is invalid"))?;
            let mut primitives =
                Vec::with_capacity(document_mesh.primitives.len() * matching_entities.len());
            for (primitive_index, source) in document_mesh.primitives.iter().enumerate() {
                let Some(static_mesh) = source.mesh.as_ref() else {
                    continue;
                };
                for (_, _, entity_bits, entity_transform, _) in
                    matching_entities
                        .iter()
                        .filter(|(source_node, source_primitive, _, _, _)| {
                            *source_node == node_index && *source_primitive == primitive_index
                        })
                {
                    primitives.push(AnimationPrimitive {
                        entity_bits: *entity_bits,
                        primitive_index: primitive_index as u32,
                        node_index,
                        mesh: MeshRef {
                            asset: mesh_id.clone(),
                            generation: document_handle.generation(),
                        },
                        document: Handle::new(DOCUMENT_HANDLE),
                        skeleton: Some(Handle::new(SKELETON_HANDLE)),
                        static_mesh: Arc::new(static_mesh.clone()),
                        world_transform: *entity_transform,
                    });
                }
            }
            if primitives.is_empty() {
                continue;
            }
            let mut runtime = AnimatorRuntime {
                states: Default::default(),
                generation: 1,
            };
            let actor_names = matching_entities
                .iter()
                .map(|(_, _, entity, _, actor)| (*entity, actor.as_str()))
                .collect::<HashMap<_, _>>();
            let mut actor_members: HashMap<&str, Vec<u64>> = HashMap::new();
            for primitive in &primitives {
                let actor = actor_names
                    .get(&primitive.entity_bits)
                    .copied()
                    .unwrap_or("unknown_actor");
                actor_members
                    .entry(actor)
                    .or_default()
                    .push(primitive.entity_bits);
            }
            let mut controls_groups = HashMap::new();
            for members in actor_members.into_values() {
                let members: Arc<[u64]> = Arc::from(members);
                for entity in members.iter() {
                    controls_groups.insert(*entity, members.clone());
                }
            }
            for primitive in &primitives {
                runtime
                    .states
                    .entry(primitive.entity_bits)
                    .or_insert(AnimatorState {
                        clip: Some(Handle::new(CLIP_HANDLE)),
                        ..AnimatorState::default()
                    });
            }
            return Ok(Some(Self {
                assets: SceneAnimationAssets {
                    document,
                    skeleton,
                    clip,
                },
                primitives,
                runtime,
                controls_groups,
                generation: 1,
            }));
        }
        Ok(None)
    }

    /// Sample and publish this scene's immutable animation frame.
    pub(crate) fn extract(
        &mut self,
        world: &World,
        input: AnimationFrameInput<'_>,
    ) -> Result<AnimationFrame, AnimationError> {
        if let Some(seconds) = input.seek_time {
            for entity_bits in self.runtime.states.keys().copied().collect::<Vec<_>>() {
                self.runtime.seek(entity_bits, seconds)?;
            }
        } else if !input.paused {
            self.runtime.advance(input.delta_time)?;
        }
        let transforms = world
            .query::<(Entity, &Transform, Option<&Visibility>)>()
            .iter()
            .map(|(entity, transform, visibility)| {
                (
                    entity.to_bits().get(),
                    (
                        world_transform(transform),
                        visibility.map_or(true, |visibility| visibility.0),
                    ),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let primitives = self
            .primitives
            .iter()
            .filter_map(|primitive| {
                let (transform, visible) = transforms.get(&primitive.entity_bits)?;
                visible.then(|| {
                    let mut primitive = primitive.clone();
                    primitive.world_transform = *transform;
                    primitive
                })
            })
            .collect::<Vec<_>>();
        let snapshot = SceneSnapshot {
            generation: self.generation,
            frame_id: input.frame_id,
        };
        let camera_frame = AnimationCameraFrame {
            view_projection: input.camera.projection_matrix(input.aspect)
                * input.camera.view_matrix(),
        };
        let context = AnimationContext {
            snapshot: &snapshot,
            assets: &self.assets,
            primitives: &primitives,
            camera: &camera_frame,
            scene_generation: self.generation,
            frame_id: input.frame_id,
        };
        extract_animation_frame(&self.runtime, &context)
    }
}

fn world_transform(transform: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        transform.scale,
        transform.rotation,
        transform.translation,
    )
}

#[cfg(test)]
#[path = "animation_tests.rs"]
mod tests;

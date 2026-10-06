use aether_engine::{
    asset::{
        id::{AssetId, AssetKind, CanonicalPath},
        loaders::gltf::{load_document, AnimationClipAsset, GltfDocumentAsset, SkeletonAsset},
        mesh::{CpuMesh, GpuMesh},
    },
    renderer::{extract::RenderBatch, renderable::MaterialUniform, skinned::SkinnedRenderData},
    time::animation::{
        extract_animation_frame, pose_state_hash, sample_animation_pose, AnimationAssets,
        AnimationCameraFrame, AnimationContext, AnimationItemResult, AnimationPrimitiveMapping,
        AnimationPrimitiveSnapshot, AnimatorRuntime, AnimatorState, SceneSnapshot,
    },
};
use glam::Mat4;
use std::{path::Path, sync::Arc};

const ACTOR_DOCUMENT: &str = "tests/fixtures/t7_animated_actor.gltf";
const ACTOR_SKELETON: &str = "tests/fixtures/t7_animated_actor.skeleton";
const ACTOR_CLIP_NAME: &str = "RaiseArms";
const BIND_ENTITY: u64 = 701;
const ANIMATED_ENTITY: u64 = 702;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PlaybackCommand {
    Play,
    Pause,
}

struct FixtureAssets {
    document_id: AssetId,
    skeleton_id: AssetId,
    document: Arc<GltfDocumentAsset>,
    skeleton: Arc<SkeletonAsset>,
}

pub(crate) struct AnimationFixture {
    batches: Vec<RenderBatch>,
    assets: FixtureAssets,
    clip: AnimationClipAsset,
    animated_state: AnimatorState,
    primitive_snapshot: AnimationPrimitiveSnapshot,
    runtime: AnimatorRuntime,
    frame_id: u64,
}

impl AnimationFixture {
    pub(super) fn append_batches(
        &mut self,
        target: &mut Vec<RenderBatch>,
        queue: &wgpu::Queue,
        dt: f32,
        time: &aether_engine::time::TimeControl,
    ) {
        let animation_dt = match time.mode {
            aether_engine::time::TimeMode::WallClock => dt,
            aether_engine::time::TimeMode::FixedStep => time.fixed_dt,
            aether_engine::time::TimeMode::Seek => 0.0,
        };
        if animation_dt > 0.0
            && self.animated_state.is_playing()
            && self.animated_state.speed() != 0.0
        {
            if let Err(error) = self.update(queue, animation_dt) {
                tracing::error!(%error, "T7 animation playback update failed");
            }
        }
        target.extend(self.batches.iter().cloned());
    }

    pub(super) fn clip_name(&self) -> &str {
        ACTOR_CLIP_NAME
    }

    pub(super) fn time(&self) -> f32 {
        self.animated_state.time()
    }

    pub(super) fn duration(&self) -> f32 {
        self.clip.duration
    }

    pub(super) fn is_playing(&self) -> bool {
        self.animated_state.is_playing()
    }

    pub(super) fn speed(&self) -> f32 {
        self.animated_state.speed()
    }

    pub(super) fn is_looping(&self) -> bool {
        self.animated_state.is_looping()
    }

    pub(super) fn apply_controls(
        &mut self,
        speed: f32,
        looping: bool,
        command: Option<PlaybackCommand>,
        seek: Option<f32>,
    ) -> anyhow::Result<()> {
        apply_playback_controls(
            &mut self.animated_state,
            self.clip.duration,
            speed,
            looping,
            command,
            seek,
        )?;
        Ok(())
    }

    pub(super) fn update(&mut self, queue: &wgpu::Queue, dt: f32) -> anyhow::Result<()> {
        self.animated_state
            .advance(dt.max(0.0), self.clip.duration)?;
        self.runtime.set_pose(
            ANIMATED_ENTITY,
            sample_animation_pose(
                &self.clip,
                &self.assets.skeleton,
                self.animated_state.time(),
            )?,
        );
        self.frame_id = self.frame_id.wrapping_add(1);
        let frame = extract_animation_frame(
            &mut self.runtime,
            &AnimationContext {
                snapshot: SceneSnapshot { generation: 1 },
                assets: &self.assets,
                primitives: &self.primitive_snapshot,
                camera: AnimationCameraFrame {
                    view_projection: Mat4::IDENTITY.to_cols_array_2d(),
                },
                scene_generation: 1,
                frame_id: self.frame_id,
            },
        )?;
        let palette = frame
            .items
            .iter()
            .find_map(|item| {
                if item.entity_id != ANIMATED_ENTITY {
                    return None;
                }
                match &item.result {
                    AnimationItemResult::Skinned { palette, .. } => Some(Arc::clone(palette)),
                    AnimationItemResult::StaticFallback(_) => None,
                }
            })
            .ok_or_else(|| anyhow::anyhow!("T7 animated actor did not produce a skin palette"))?;
        let batch = self
            .batches
            .iter()
            .find(|batch| {
                batch
                    .instances
                    .iter()
                    .any(|instance| instance.entity_id == ANIMATED_ENTITY as u32)
            })
            .ok_or_else(|| anyhow::anyhow!("T7 animated render batch is missing"))?;
        let skinned = batch
            .skinned
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("T7 animated render batch has no skin data"))?;
        skinned.storage.upload(queue, &palette)?;
        Ok(())
    }
}

impl AnimationAssets for FixtureAssets {
    fn document(&self, id: &AssetId) -> Option<Arc<GltfDocumentAsset>> {
        (id == &self.document_id).then(|| Arc::clone(&self.document))
    }

    fn skeleton(&self, id: &AssetId) -> Option<Arc<SkeletonAsset>> {
        (id == &self.skeleton_id).then(|| Arc::clone(&self.skeleton))
    }
}

/// Load a real glTF skin and clip, publish palettes, then build render batches.
pub(super) fn create(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<AnimationFixture> {
    let document_id = asset_id(AssetKind::GltfDocument, ACTOR_DOCUMENT)?;
    let skeleton_id = asset_id(AssetKind::Skeleton, ACTOR_SKELETON)?;
    let document = Arc::new(load_document(Path::new(ACTOR_DOCUMENT))?);
    let skeleton = Arc::new(SkeletonAsset::from_document(
        document_id.clone(),
        &document,
        0,
    )?);
    let clip = AnimationClipAsset::from_document(document_id.clone(), &document, 0, &skeleton)?;
    let primitive = document
        .meshes
        .first()
        .and_then(|mesh| mesh.primitives.first())
        .ok_or_else(|| anyhow::anyhow!("T7 glTF fixture has no mesh primitive"))?;
    let vertices = primitive
        .skinned_vertices()
        .ok_or_else(|| anyhow::anyhow!("T7 glTF fixture has no valid skin streams"))?;
    let assets = FixtureAssets {
        document_id: document_id.clone(),
        skeleton_id: skeleton_id.clone(),
        document: Arc::clone(&document),
        skeleton,
    };

    let mut bind_state = AnimatorState::new(false);
    bind_state.seek(0.0, clip.duration)?;
    let bind_pose = sample_animation_pose(&clip, &assets.skeleton, bind_state.time())?;

    let mut animated_state = AnimatorState::new(false);
    animated_state.seek(clip.duration, clip.duration)?;
    animated_state.set_looping(true);
    animated_state.play();
    let animated_pose = sample_animation_pose(&clip, &assets.skeleton, animated_state.time())?;
    let pose_hashes = [0.0, 0.25, 0.5, 0.75]
        .into_iter()
        .map(|time| {
            let bytes = pose_state_hash(&sample_animation_pose(&clip, &assets.skeleton, time)?);
            Ok::<_, anyhow::Error>(
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    println!("T7_POSE_HASHES={}", pose_hashes.join(","));

    let mappings = [
        mapping(BIND_ENTITY, &document_id, &skeleton_id, [-1.3, 0.0, 0.0]),
        mapping(ANIMATED_ENTITY, &document_id, &skeleton_id, [1.3, 0.0, 0.0]),
    ];
    let primitive_snapshot = AnimationPrimitiveSnapshot {
        primitives: Arc::from(mappings.clone()),
    };
    let mut runtime = AnimatorRuntime::new(1);
    runtime.set_pose(BIND_ENTITY, bind_pose);
    runtime.set_pose(ANIMATED_ENTITY, animated_pose);
    let frame = extract_animation_frame(
        &mut runtime,
        &AnimationContext {
            snapshot: SceneSnapshot { generation: 1 },
            assets: &assets,
            primitives: &primitive_snapshot,
            camera: AnimationCameraFrame {
                view_projection: Mat4::IDENTITY.to_cols_array_2d(),
            },
            scene_generation: 1,
            frame_id: 1,
        },
    )?;

    let placeholder = Arc::new(GpuMesh::from_cpu(device, &CpuMesh::cube()));
    let indices = primitive.indices.as_ref();
    let batches = frame
        .items
        .iter()
        .map(|item| {
            let AnimationItemResult::Skinned { palette, .. } = &item.result else {
                anyhow::bail!(
                    "T7 glTF fixture fell back for entity {}: {:?}",
                    item.entity_id,
                    item.result
                );
            };
            let skinned = Arc::new(SkinnedRenderData::new(
                device,
                queue,
                &vertices,
                indices,
                palette.as_ref().to_owned(),
            )?);
            let pose_color = if item.entity_id == BIND_ENTITY {
                [0.08, 0.64, 0.8, 1.0]
            } else {
                [0.95, 0.34, 0.12, 1.0]
            };
            Ok(RenderBatch {
                mesh: Arc::clone(&placeholder),
                extracted_material: aether_engine::renderer::extracted_material::ExtractedMaterial::from_legacy_uniform(MaterialUniform {
                    albedo: pose_color,
                    roughness: 0.34,
                    metallic: 0.08,
                    ..MaterialUniform::default()
                }),
                transparent_material: None,
                skinned: Some(skinned),
                instances: vec![aether_engine::asset::mesh::InstanceData {
                    model_matrix: mappings
                        .iter()
                        .find(|mapping| mapping.entity_id == item.entity_id)
                        .expect("every animation item has a fixture mapping")
                        .world_transform,
                    entity_id: item.entity_id as u32,
                    _pad: [0; 3],
                }],
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(AnimationFixture {
        batches,
        assets,
        clip,
        animated_state,
        primitive_snapshot,
        runtime,
        frame_id: 1,
    })
}

fn mapping(
    entity_id: u64,
    document: &AssetId,
    skeleton: &AssetId,
    translation: [f32; 3],
) -> AnimationPrimitiveMapping {
    AnimationPrimitiveMapping {
        entity_id,
        mesh_index: 0,
        primitive_index: 0,
        joint_count: 3,
        document: document.clone(),
        skeleton: Some(skeleton.clone()),
        world_transform: Mat4::from_translation(glam::Vec3::from_array(translation))
            .to_cols_array_2d(),
    }
}

fn asset_id(kind: AssetKind, path: &str) -> anyhow::Result<AssetId> {
    Ok(AssetId::new(
        kind,
        CanonicalPath::new(Path::new("."), Path::new(path))?,
    ))
}

fn apply_playback_controls(
    state: &mut AnimatorState,
    duration: f32,
    speed: f32,
    looping: bool,
    command: Option<PlaybackCommand>,
    seek: Option<f32>,
) -> anyhow::Result<()> {
    state.set_speed(speed)?;
    state.set_looping(looping);
    if let Some(time) = seek {
        state.seek(time, duration)?;
    }
    match command {
        Some(PlaybackCommand::Play) => prepare_playback(state, duration)?,
        Some(PlaybackCommand::Pause) => state.pause(),
        None => {}
    }
    Ok(())
}

fn prepare_playback(state: &mut AnimatorState, duration: f32) -> anyhow::Result<()> {
    if !state.is_looping() {
        let time = state.time();
        let speed = state.speed();
        let restart = if speed > 0.0 && time >= duration {
            Some(0.0)
        } else if speed < 0.0 && time <= 0.0 {
            Some(duration)
        } else {
            None
        };
        if let Some(time) = restart {
            state.seek(time, duration)?;
        }
    }
    state.play();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_playback_controls, PlaybackCommand};
    use aether_engine::time::animation::AnimatorState;

    #[test]
    fn playback_controls_apply_speed_loop_seek_and_pause_or_play() {
        let mut forward = AnimatorState::new(false);
        forward.seek(0.75, 0.75).unwrap();
        apply_playback_controls(
            &mut forward,
            0.75,
            1.0,
            false,
            Some(PlaybackCommand::Play),
            None,
        )
        .unwrap();
        assert_eq!(forward.time(), 0.0);
        assert!(forward.is_playing());

        let mut reverse = AnimatorState::new(false);
        apply_playback_controls(
            &mut reverse,
            0.75,
            -1.0,
            false,
            Some(PlaybackCommand::Play),
            None,
        )
        .unwrap();
        assert_eq!(reverse.time(), 0.75);
        assert!(reverse.is_playing());

        apply_playback_controls(
            &mut reverse,
            0.75,
            0.5,
            true,
            Some(PlaybackCommand::Pause),
            Some(0.4),
        )
        .unwrap();
        assert_eq!(reverse.time(), 0.4);
        assert_eq!(reverse.speed(), 0.5);
        assert!(reverse.is_looping());
        assert!(!reverse.is_playing());
    }
}

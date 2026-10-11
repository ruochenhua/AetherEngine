use super::*;
use aether_engine::asset::mesh::{CpuMesh, GpuMesh, InstanceData};
use aether_engine::asset::Asset;
use aether_engine::ecs::components::Name;
use aether_engine::ecs::World;
use aether_engine::renderer::extract::{animation_aware_batches, RenderBatch};
use aether_engine::renderer::renderable::MaterialUniform;

fn headless_device() -> wgpu::Device {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("need adapter");
    let (device, _queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("need device");
    device
}

#[test]
fn scene_animation_matches_only_the_skinned_node_primitive() {
    let device = headless_device();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/t7_owned_document.gltf"
    );
    let cpu_mesh = CpuMesh::load(std::path::Path::new(path)).expect("fixture mesh should load");
    assert_eq!(cpu_mesh.submeshes.len(), 4);
    let gpu_mesh = Arc::new(GpuMesh::from_cpu(&device, &cpu_mesh));
    let mut world = World::new();
    let mut source_batches = Vec::new();
    let mut expected_animated_entities = Vec::new();
    let mut static_sibling_entities = Vec::new();
    for submesh in &cpu_mesh.submeshes {
        let view = Arc::new(GpuMesh::submesh_view(
            &gpu_mesh,
            submesh.index_offset as u32,
            submesh.index_count as u32,
        ));
        let entity = world.spawn((
            Transform::default(),
            MeshHandle::new(
                view.clone(),
                MeshSource::File(path.to_string()),
                "fixture primitive",
            )
            .with_gltf_primitive(submesh.source_node_index, submesh.source_primitive_index),
            Visibility(true),
            Name("fixture primitive".to_string()),
        ));
        let entity_bits = entity.to_bits().get() as u32;
        let material = MaterialUniform {
            albedo: if submesh.source_primitive_index == Some(0) {
                [0.8, 0.45, 0.2, 1.0]
            } else {
                [0.2, 0.7, 0.35, 1.0]
            },
            ..MaterialUniform::default()
        };
        source_batches.push(RenderBatch {
            mesh: view,
            material,
            albedo_texture: None,
            normal_texture: None,
            orm_texture: None,
            emissive_texture: None,
            transparent_material: None,
            skin_palette: None,
            instances: vec![InstanceData {
                model_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
                entity_id: entity_bits,
                _pad: [0; 3],
            }],
        });
        match (submesh.source_node_index, submesh.source_primitive_index) {
            (Some(0), Some(_)) => expected_animated_entities.push(entity_bits),
            (Some(2), Some(_)) => static_sibling_entities.push(entity_bits),
            other => panic!("unexpected source glTF identity: {other:?}"),
        }
    }
    let mut assets = AssetManager::new();
    let mut animation = SceneAnimation::from_world(&world, &mut assets)
        .expect("animation setup should succeed")
        .expect("fixture animation should be found");
    assert_eq!(animation.primitives.len(), 2);
    assert!(animation
        .primitives
        .iter()
        .all(|primitive| primitive.node_index == 0));
    assert_eq!(animation.runtime.states.len(), 2);
    assert_eq!(
        animation
            .primitives
            .iter()
            .map(|primitive| primitive.primitive_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        animation
            .primitives
            .iter()
            .map(|primitive| primitive.entity_bits as u32)
            .collect::<Vec<_>>(),
        expected_animated_entities
    );

    let first_entity = animation.primitives[0].entity_bits;
    let mut controls = animation
        .controls(first_entity)
        .expect("animated actor controls should exist");
    controls.time = 0.75;
    controls.speed = 1.5;
    controls.looping = false;
    controls.playing = false;
    animation.apply_controls(first_entity, controls);
    for primitive in &animation.primitives {
        let state = &animation.runtime.states[&primitive.entity_bits];
        assert_eq!(state.time, 0.75);
        assert_eq!(state.speed, 1.5);
        assert!(!state.looping);
        assert!(!state.playing);
    }

    let camera = FlyCamera::default();
    let frame = animation
        .extract(
            &world,
            AnimationFrameInput {
                camera: &camera,
                aspect: 1.0,
                frame_id: 1,
                delta_time: 0.0,
                paused: true,
                seek_time: None,
            },
        )
        .expect("animation frame should extract");
    assert!(frame.items.iter().all(|item| match item {
        aether_engine::animation::AnimationRenderItem::Skinned(item) => {
            (item.sample_time - 0.75).abs() < 1e-6
        }
        aether_engine::animation::AnimationRenderItem::StaticFallback(_) => false,
    }));
    let prepared = animation_aware_batches(&device, &Arc::from(source_batches), Some(&frame));
    let prepared_entities = prepared
        .iter()
        .flat_map(|batch| batch.instances.iter().map(|instance| instance.entity_id))
        .collect::<std::collections::HashSet<_>>();
    assert!(expected_animated_entities
        .iter()
        .all(|entity| prepared_entities.contains(entity)));
    assert!(static_sibling_entities
        .iter()
        .all(|entity| prepared_entities.contains(entity)));
    let animated_batches = prepared
        .iter()
        .filter(|batch| batch.skin_palette.is_some())
        .collect::<Vec<_>>();
    assert_eq!(animated_batches.len(), 2);
    for batch in animated_batches {
        let entity = batch.instances[0].entity_id;
        let primitive_index = animation
            .primitives
            .iter()
            .find(|primitive| primitive.entity_bits as u32 == entity)
            .unwrap()
            .primitive_index;
        assert_eq!(
            batch.material.albedo,
            if primitive_index == 0 {
                [0.8, 0.45, 0.2, 1.0]
            } else {
                [0.2, 0.7, 0.35, 1.0]
            }
        );
    }
}

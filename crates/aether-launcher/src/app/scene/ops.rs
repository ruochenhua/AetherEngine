// Editor UI operations for scene management.

use super::*;

pub(crate) fn process_post_ui_ops(app: &mut App) {
    // New scene
    if app.pending_new_scene {
        app.pending_new_scene = false;
        if let LauncherState::Running {
            ref mut world,
            ref mut lighting,
        } = app.state
        {
            let ctx = app.ctx.as_ref().unwrap();
            world.clear();
            *lighting = SceneLoader::new_empty(world);
            // Spawn a default cube so there's something to pick right away
            if let Some(cpu_mesh) = app.mesh_registry.get("cube") {
                let gpu_mesh = Arc::new(GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
                let _entity = world.spawn((
                    Transform::default(),
                    MeshHandle::new(gpu_mesh, MeshSource::Builtin("cube".into()), "cube"),
                    aether_engine::renderer::renderable::MaterialUniform {
                        albedo: [0.8, 0.3, 0.2, 1.0],
                        roughness: 0.5,
                        metallic: 0.0,
                        unlit: 0,
                        _pad: 0,
                        albedo_texture_id: 0,
                        ..Default::default()
                    },
                    Visibility::default(),
                    Name("DefaultCube".into()),
                    Selected,
                ));
            }
            app.camera = FlyCamera::default();
            info!("New scene created");
        }
    }

    // Open scene
    if app.pending_open_dialog {
        app.pending_open_dialog = false;
        if let Some(path) = rfd::FileDialog::new()
            .set_directory("./scenes")
            .add_filter("RON", &["ron"])
            .pick_file()
        {
            if let LauncherState::Running {
                ref mut world,
                ref mut lighting,
            } = app.state
            {
                let ctx = app.ctx.as_ref().unwrap();
                match SceneLoader::open_scene(
                    &path,
                    &ctx.device,
                    &app.mesh_registry,
                    &mut app.asset_manager,
                    world,
                ) {
                    Ok(new_lighting) => {
                        *lighting = new_lighting;
                        if let Some((pos, yaw, pitch, fov, speed, near, far)) =
                            read_camera_from_world(world)
                        {
                            app.camera.position = pos;
                            app.camera.yaw = yaw;
                            app.camera.pitch = pitch;
                            app.camera.fov = fov;
                            app.camera.speed = speed;
                            app.camera.base_speed = speed;
                            app.camera.near = near;
                            app.camera.far = far;
                            app.camera.active = false;
                        }
                        info!("Opened scene from {:?}", path);
                        app.pending_terrain_pipeline_rebuild = true;
                    }
                    Err(e) => {
                        error!("Open scene error: {:?}", e);
                    }
                }
            }
        }
    }

    // Import scene
    if app.pending_import_dialog {
        app.pending_import_dialog = false;
        if let Some(path) = rfd::FileDialog::new()
            .set_directory("./scenes")
            .add_filter("RON", &["ron"])
            .pick_file()
        {
            if let LauncherState::Running { ref mut world, .. } = app.state {
                let ctx = app.ctx.as_ref().unwrap();
                match SceneLoader::import_scene(
                    &path,
                    &ctx.device,
                    &app.mesh_registry,
                    &mut app.asset_manager,
                    world,
                ) {
                    Ok(()) => {
                        info!("Imported scene from {:?}", path);
                        app.pending_terrain_pipeline_rebuild = true;
                    }
                    Err(e) => {
                        error!("Import scene error: {:?}", e);
                    }
                }
            }
        }
    }

    // Save scene
    if app.pending_save_dialog {
        app.pending_save_dialog = false;
        if let LauncherState::Running {
            ref mut world,
            ref lighting,
        } = app.state
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_directory("./scenes")
                .set_file_name("scene.ron")
                .add_filter("RON scenes", &["ron"])
                .save_file()
            {
                let path = if !path.extension().is_some_and(|ext| ext == "ron") {
                    path.with_extension("ron")
                } else {
                    path
                };
                write_camera_to_world(&app.camera, world);
                let desc =
                    aether_engine::scene::serializer::serialize_world(world, lighting, "Untitled");
                info!(
                    "Saving scene: {} objects, {} lights, camera at {:?}",
                    desc.objects.len(),
                    desc.lights.len(),
                    desc.camera.position
                );
                match aether_engine::scene::serializer::to_ron_string(&desc) {
                    Ok(ron) => {
                        if let Err(e) = std::fs::write(&path, ron) {
                            error!("Save scene error: {:?}", e);
                        } else {
                            info!("Saved scene to {:?}", path);
                        }
                    }
                    Err(e) => {
                        error!("Serialize scene error: {:?}", e);
                    }
                }
            }
        }
    }

    // Handle add object actions
    let mut added_entity: Option<Entity> = None;

    if app.pending_add_cube {
        app.pending_add_cube = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            let ctx = app.ctx.as_ref().unwrap();
            if let Some(cpu_mesh) = app.mesh_registry.get("cube") {
                let gpu_mesh = Arc::new(GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
                let entity = world.spawn((
                    Transform::default(),
                    MeshHandle::new(gpu_mesh, MeshSource::Builtin("cube".into()), "cube"),
                    aether_engine::renderer::renderable::MaterialUniform {
                        albedo: [0.8, 0.3, 0.2, 1.0],
                        roughness: 0.5,
                        metallic: 0.0,
                        unlit: 0,
                        _pad: 0,
                        albedo_texture_id: 0,
                        ..Default::default()
                    },
                    Visibility::default(),
                    Name("Cube".into()),
                ));
                added_entity = Some(entity);
                info!("Added cube entity {:?}", entity);
            }
        }
    }

    if app.pending_add_sphere {
        app.pending_add_sphere = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            let ctx = app.ctx.as_ref().unwrap();
            if let Some(cpu_mesh) = app.mesh_registry.get("sphere") {
                let gpu_mesh = Arc::new(GpuMesh::from_cpu(&ctx.device, &cpu_mesh));
                let entity = world.spawn((
                    Transform::default(),
                    MeshHandle::new(gpu_mesh, MeshSource::Builtin("sphere".into()), "sphere"),
                    aether_engine::renderer::renderable::MaterialUniform {
                        albedo: [0.2, 0.5, 0.8, 1.0],
                        roughness: 0.05,
                        metallic: 0.0,
                        unlit: 0,
                        _pad: 0,
                        albedo_texture_id: 0,
                        ..Default::default()
                    },
                    Visibility::default(),
                    Name("Sphere".into()),
                ));
                added_entity = Some(entity);
                info!("Added sphere entity {:?}", entity);
            }
        }
    }

    if app.pending_add_terrain {
        app.pending_add_terrain = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Replace any existing terrain entity.
            let existing: Vec<Entity> = world
                .query::<(Entity, &Terrain)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in existing {
                let _ = world.despawn(e);
            }

            let terrain = Terrain {
                source: aether_engine::scene::TerrainSource::Procedural {
                    seed: 1,
                    frequency: 0.05,
                    amplitude: 32.0,
                },
                geometry: aether_engine::scene::TerrainGeometry {
                    extent: 256.0,
                    chunk_size: 64,
                    max_lod: 4,
                    albedo_tiling: 64.0,
                },
                material: aether_engine::asset::terrain_material::TerrainMaterial::default(),
                splatmap_path: None,
                layer_configs: vec![
                    aether_engine::scene::TerrainLayerConfig::default(),
                    aether_engine::scene::TerrainLayerConfig::default(),
                    aether_engine::scene::TerrainLayerConfig::default(),
                    aether_engine::scene::TerrainLayerConfig::default(),
                ],
            };
            let entity = world.spawn((Transform::default(), terrain, Name("Terrain".into())));
            added_entity = Some(entity);
            info!("Added terrain entity {:?}", entity);
        }
        app.pending_terrain_pipeline_rebuild = true;
    }

    if app.pending_add_water {
        app.pending_add_water = false;
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Replace any existing water entity.
            let existing: Vec<Entity> = world
                .query::<(Entity, &Water)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in existing {
                let _ = world.despawn(e);
            }

            let water = Water {
                config: aether_engine::scene::WaterConfig::default(),
                dudv_texture: None,
                normal_texture: None,
            };
            let entity = world.spawn((Transform::default(), water, Name("Water".into())));
            added_entity = Some(entity);
            info!("Added water entity {:?}", entity);
        }
    }

    // Auto-select newly added entity
    if let Some(entity) = added_entity {
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Deselect all: collect first, then modify
            let to_deselect: Vec<_> = world
                .query::<(Entity, &Selected)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in to_deselect {
                let _ = world.remove::<(Selected,)>(e);
            }
            // Select new entity
            let _ = world.insert(entity, (Selected,));
        }
    }

    // Handle hierarchy panel selection
    if let Some(entity) = app.pending_select_entity.take() {
        if let LauncherState::Running { ref mut world, .. } = app.state {
            // Deselect all
            let to_deselect: Vec<_> = world
                .query::<(Entity, &Selected)>()
                .iter()
                .map(|(e, _)| e)
                .collect();
            for e in to_deselect {
                let _ = world.remove::<(Selected,)>(e);
            }
            // Select chosen entity
            let _ = world.insert(entity, (Selected,));
        }
    }

    // Handle despawn (delete entity)
    if let Some(entity) = app.pending_despawn_entity.take() {
        if let LauncherState::Running { ref mut world, .. } = app.state {
            let _ = world.despawn(entity);
        }
        app.pending_terrain_pipeline_rebuild = true;
    }
}

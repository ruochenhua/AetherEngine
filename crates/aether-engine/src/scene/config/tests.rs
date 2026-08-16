use super::*;
use crate::renderer::light::LightType;

#[test]
fn parse_empty_scene() {
    let ron = r#"
            SceneDescription(
                name: "Empty",
                camera: (position: (0.0, 0.0, 0.0)),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    assert_eq!(scene.name, "Empty");
    assert!(scene.objects.is_empty());
    assert!(scene.lights.is_empty());
    assert_eq!(scene.ambient, 0.0);
}

#[test]
fn parse_single_object_with_builtin_mesh() {
    let ron = r#"
            SceneDescription(
                name: "One Cube",
                camera: (position: (3.0, 3.0, 3.0)),
                objects: [
                    (
                        name: "MyCube",
                        mesh: Builtin("cube"),
                        transform: (
                            translation: (-0.8, 0.0, 0.0),
                        ),
                        material: (
                            albedo: (0.8, 0.3, 0.2, 1.0),
                            roughness: 0.5,
                            metallic: 0.0,
                        ),
                    ),
                ],
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    assert_eq!(scene.objects.len(), 1);
    let obj = &scene.objects[0];
    assert_eq!(obj.name, "MyCube");
    assert_eq!(obj.mesh, MeshRef::Builtin("cube".into()));
    assert_eq!(obj.transform.translation, [-0.8, 0.0, 0.0]);
    assert_eq!(obj.material.albedo, [0.8, 0.3, 0.2, 1.0]);
    assert_eq!(obj.material.roughness, 0.5);
}

#[test]
fn parse_multiple_objects_with_light() {
    let ron = r#"
            SceneDescription(
                name: "Two Objects",
                camera: (position: (3.0, 3.0, 3.0)),
                ambient: 0.05,
                lights: [
                    (
                        light_type: Directional,
                        direction: (0.0, -1.0, 0.0),
                        color: (1.0, 1.0, 1.0),
                        intensity: 1.0,
                    ),
                ],
                objects: [
                    (
                        mesh: Builtin("cube"),
                        transform: (translation: (-0.8, 0.0, 0.0)),
                    ),
                    (
                        mesh: Builtin("sphere"),
                        transform: (translation: (0.8, 0.0, 0.0)),
                        material: (roughness: 0.05),
                    ),
                ],
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    assert_eq!(scene.objects.len(), 2);
    assert_eq!(scene.lights.len(), 1);
    assert_eq!(scene.ambient, 0.05);
    assert_eq!(scene.lights[0].light_type, LightType::Directional);
    assert_eq!(scene.lights[0].direction, [0.0, -1.0, 0.0]);
    // Second object should have default albedo
    assert_eq!(scene.objects[1].material.albedo, [0.8, 0.8, 0.8, 1.0]);
}

#[test]
fn parse_with_file_mesh_reference() {
    let ron = r#"
            SceneDescription(
                name: "File Mesh",
                camera: (position: (0.0, 0.0, 0.0)),
                objects: [
                    (
                        name: "Dragon",
                        mesh: File("assets/models/dragon.obj"),
                    ),
                ],
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    assert_eq!(scene.objects.len(), 1);
    assert_eq!(
        scene.objects[0].mesh,
        MeshRef::File("assets/models/dragon.obj".into())
    );
}

#[test]
fn parse_camera_full_config() {
    let ron = r#"
            SceneDescription(
                name: "Camera Test",
                camera: (
                    position: (5.0, 10.0, 5.0),
                    yaw: -1.5,
                    pitch: -0.5,
                    speed: 8.0,
                    fov: 60.0,
                ),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    assert_eq!(scene.camera.position, [5.0, 10.0, 5.0]);
    assert_eq!(scene.camera.yaw, -1.5);
    assert_eq!(scene.camera.pitch, -0.5);
    assert_eq!(scene.camera.speed, 8.0);
    assert_eq!(scene.camera.fov, 60.0);
}

#[test]
fn invalid_ron_returns_error() {
    let ron = "not valid ron {";
    let result = SceneDescription::from_ron(ron);
    assert!(result.is_err());
}

#[test]
fn missing_camera_field_returns_error() {
    let ron = r#"
            SceneDescription(
                name: "No Camera",
            )
        "#;
    let result = SceneDescription::from_ron(ron);
    assert!(result.is_err());
}

#[test]
fn all_defaults_populated() {
    let ron = r#"
            SceneDescription(
                name: "Defaults",
                camera: (position: (0.0, 0.0, 0.0)),
                objects: [
                    (mesh: Builtin("quad"),),
                ],
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    let obj = &scene.objects[0];
    assert_eq!(obj.name, ""); // default
    assert_eq!(obj.transform.translation, [0.0; 3]);
    assert_eq!(obj.transform.rotation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(obj.transform.scale, [1.0; 3]);
    assert_eq!(obj.material.albedo, [0.8, 0.8, 0.8, 1.0]);
    assert_eq!(obj.material.roughness, 0.5);
    assert_eq!(obj.material.metallic, 0.0);
}

#[test]
fn parse_shadow_demo_has_7_objects() {
    let content = include_str!("../../../../../scenes/03_shadow_demo.ron");
    let desc = SceneDescription::from_ron(content).expect("should parse");
    assert_eq!(desc.objects.len(), 7, "Expected 7 objects");
}

#[test]
fn parse_terrain_scene_file() {
    let content = include_str!("../../../../../scenes/08_terrain.ron");
    let desc = SceneDescription::from_ron(content).expect("should parse");
    assert_eq!(desc.name, "Terrain Foundation");
    let terrain = desc.terrain.expect("terrain should be present");
    assert_eq!(terrain.geometry.extent, 512.0);
    assert_eq!(terrain.geometry.chunk_size, 64);
    assert_eq!(terrain.geometry.max_lod, 5);
}

#[test]
fn parse_terrain_with_heightmap_source() {
    let ron = r#"
            SceneDescription(
                name: "Terrain",
                camera: (position: (0.0, 0.0, 0.0)),
                terrain: Some((
                    source: Heightmap("assets/terrain/heightmap.png"),
                    geometry: (extent: 512.0, chunk_size: 128, max_lod: 5),
                )),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    let terrain = scene.terrain.expect("terrain should be present");
    assert_eq!(
        terrain.source,
        TerrainSource::Heightmap("assets/terrain/heightmap.png".into())
    );
    assert_eq!(terrain.geometry.extent, 512.0);
    assert_eq!(terrain.geometry.chunk_size, 128);
    assert_eq!(terrain.geometry.max_lod, 5);
}

#[test]
fn parse_terrain_with_procedural_source() {
    let ron = r#"
            SceneDescription(
                name: "Procedural Terrain",
                camera: (position: (0.0, 0.0, 0.0)),
                terrain: Some((
                    source: Procedural(seed: 42, frequency: 0.1, amplitude: 64.0),
                )),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    let terrain = scene.terrain.expect("terrain should be present");
    assert_eq!(
        terrain.source,
        TerrainSource::Procedural {
            seed: 42,
            frequency: 0.1,
            amplitude: 64.0,
        }
    );
    assert_eq!(terrain.geometry, TerrainGeometry::default());
}

#[test]
fn parse_terrain_with_perlin_source() {
    let ron = r#"
            SceneDescription(
                name: "Perlin Terrain",
                camera: (position: (0.0, 0.0, 0.0)),
                terrain: Some((
                    source: Perlin(seed: 42, frequency: 0.01, amplitude: 64.0, octaves: 6, persistence: 0.45, lacunarity: 2.2, exponent: 1.1),
                )),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    let terrain = scene.terrain.expect("terrain should be present");
    assert_eq!(
        terrain.source,
        TerrainSource::Perlin {
            seed: 42,
            frequency: 0.01,
            amplitude: 64.0,
            octaves: 6,
            persistence: 0.45,
            lacunarity: 2.2,
            exponent: 1.1,
        }
    );
    assert_eq!(terrain.geometry, TerrainGeometry::default());
}

#[test]
fn scene_without_terrain_defaults_to_none() {
    let ron = r#"
            SceneDescription(
                name: "No Terrain",
                camera: (position: (0.0, 0.0, 0.0)),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    assert!(scene.terrain.is_none());
}

#[test]
fn parse_terrain_lod_demo_scene_file() {
    let content = include_str!("../../../../../scenes/09_terrain.ron");
    let desc = SceneDescription::from_ron(content).expect("should parse");
    assert_eq!(desc.name, "Terrain LOD Demo");
    let terrain = desc.terrain.expect("terrain should be present");
    assert_eq!(terrain.geometry.extent, 512.0);
    assert_eq!(terrain.geometry.chunk_size, 64);
    assert_eq!(terrain.geometry.max_lod, 5);
    assert_eq!(terrain.layers.len(), 4);
}

#[test]
fn parse_terrain_with_splatmap_and_layers() {
    let ron = r#"
            SceneDescription(
                name: "Splat Terrain",
                camera: (position: (0.0, 0.0, 0.0)),
                terrain: Some((
                    source: Procedural(seed: 1, frequency: 0.05, amplitude: 32.0),
                    geometry: (extent: 256.0, chunk_size: 64, max_lod: 4),
                    splatmap: Some("assets/terrain/splatmap.png"),
                    layers: [
                        (albedo: (0.8, 0.2, 0.2, 1.0), roughness: 0.9),
                        (albedo: (0.2, 0.8, 0.2, 1.0), roughness: 0.8),
                        (albedo: (0.2, 0.2, 0.8, 1.0), roughness: 0.7),
                        (albedo: (0.8, 0.8, 0.2, 1.0), roughness: 0.6),
                    ],
                )),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    let terrain = scene.terrain.expect("terrain should be present");
    assert_eq!(terrain.splatmap, Some("assets/terrain/splatmap.png".into()));
    assert_eq!(terrain.layers.len(), 4);
    assert_eq!(terrain.layers[0].albedo, [0.8, 0.2, 0.2, 1.0]);
    assert_eq!(terrain.layers[0].roughness, 0.9);
    assert_eq!(terrain.layers[3].albedo, [0.8, 0.8, 0.2, 1.0]);
}

#[test]
fn parse_scene_with_atmosphere_config() {
    let ron = r#"
            SceneDescription(
                name: "Atmosphere",
                camera: (position: (0.0, 0.0, 0.0)),
                atmosphere: Some((
                    sun_direction: (0.0, 0.1, -1.0),
                    planet_radius: 6360.0,
                    atmosphere_height: 100.0,
                    rayleigh_scattering: (0.0058, 0.0136, 0.0331),
                    mie_asymmetry: 0.76,
                )),
            )
        "#;
    let scene = SceneDescription::from_ron(ron).expect("should parse");
    let atmos = scene.atmosphere.expect("atmosphere should be present");
    assert_eq!(atmos.sun_direction, [0.0, 0.1, -1.0]);
    assert_eq!(atmos.planet_radius, 6360.0);
    assert_eq!(atmos.rayleigh_scattering, [0.0058, 0.0136, 0.0331]);
    assert_eq!(atmos.mie_asymmetry, 0.76);
}

mod extra;

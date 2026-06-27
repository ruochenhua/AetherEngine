//! Scene configuration types.
//!
//! This module contains the data structures that describe individual elements
//! of a scene, such as cameras, lights, objects, terrain, atmosphere, water,
//! clouds, and god rays.

pub mod atmosphere;
pub mod camera;
pub mod clouds;
pub mod god_ray;
pub mod light;
pub mod object;
pub mod terrain;
pub mod water;

pub use atmosphere::*;
pub use camera::*;
pub use clouds::*;
pub use god_ray::*;
pub use light::*;
pub use object::*;
pub use terrain::*;
pub use water::*;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Scene description — the root of a RON scene file
// ---------------------------------------------------------------------------

/// Top-level scene description, deserialized from a `.ron` file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneDescription {
    /// Human-readable scene name.
    pub name: String,
    /// Camera initial configuration.
    pub camera: CameraConfig,
    /// Lights in the scene.
    #[serde(default)]
    pub lights: Vec<LightConfig>,
    /// Ambient light intensity (0.0 – 1.0).
    #[serde(default)]
    pub ambient: f32,
    /// Objects in the scene.
    #[serde(default)]
    pub objects: Vec<ObjectConfig>,
    /// Optional global terrain configuration.
    #[serde(default)]
    pub terrain: Option<TerrainConfig>,
    /// Optional physical atmosphere configuration.
    #[serde(default)]
    pub atmosphere: Option<AtmosphereConfig>,
    /// Optional water surface configuration.
    #[serde(default)]
    pub water: Option<WaterConfig>,
    /// Optional volumetric cloud configuration.
    #[serde(default)]
    pub clouds: Option<CloudConfig>,
    /// Optional god ray (volumetric light) configuration.
    #[serde(default)]
    pub god_ray: Option<GodRayConfig>,
}

// ---------------------------------------------------------------------------
// RON parsing
// ---------------------------------------------------------------------------

impl SceneDescription {
    /// Parse a scene from a RON string.
    pub fn from_ron(content: &str) -> anyhow::Result<Self> {
        let desc: SceneDescription = ron::de::from_str(content)?;
        Ok(desc)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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

    #[test]
    fn scene_without_atmosphere_defaults_to_none() {
        let ron = r#"
            SceneDescription(
                name: "No Atmosphere",
                camera: (position: (0.0, 0.0, 0.0)),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert!(scene.atmosphere.is_none());
    }

    #[test]
    fn atmosphere_config_roundtrips_through_ron() {
        let desc = SceneDescription {
            name: "Atmosphere Roundtrip".into(),
            camera: CameraConfig::default(),
            lights: vec![],
            ambient: 0.0,
            terrain: None,
            atmosphere: Some(AtmosphereConfig {
                sun_direction: [0.0, 0.1, -1.0],
                planet_radius: 6360.0,
                atmosphere_height: 100.0,
                rayleigh_scattering: [0.0058, 0.0136, 0.0331],
                rayleigh_scale_height: 8.0,
                mie_scattering: [0.004, 0.004, 0.004],
                mie_scale_height: 1.2,
                mie_asymmetry: 0.76,
                sun_intensity: 20.0,
            }),
            water: None,
            clouds: None,
            god_ray: None,
            objects: vec![],
        };
        let ron = ron::ser::to_string(&desc).expect("should serialize");
        let parsed = SceneDescription::from_ron(&ron).expect("should deserialize");
        assert_eq!(parsed, desc);
    }

    #[test]
    fn parse_scene_with_water_config() {
        let ron = r#"
            SceneDescription(
                name: "Water",
                camera: (position: (0.0, 0.0, 0.0)),
                water: Some((
                    level: -0.5,
                    wave_direction: (1.0, 0.5),
                    wave_amplitude: 0.5,
                    wave_wavelength: 10.0,
                    water_color: (0.0, 0.3, 0.4),
                    fresnel_power: 2.5,
                )),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        let water = scene.water.expect("water should be present");
        assert_eq!(water.level, -0.5);
        assert_eq!(water.wave_direction, [1.0, 0.5]);
        assert_eq!(water.water_color, [0.0, 0.3, 0.4]);
        assert_eq!(water.fresnel_power, 2.5);
    }

    #[test]
    fn scene_without_water_defaults_to_none() {
        let ron = r#"
            SceneDescription(
                name: "No Water",
                camera: (position: (0.0, 0.0, 0.0)),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert!(scene.water.is_none());
    }

    #[test]
    fn water_config_roundtrips_through_ron() {
        let desc = SceneDescription {
            name: "Water Roundtrip".into(),
            camera: CameraConfig::default(),
            lights: vec![],
            ambient: 0.0,
            terrain: None,
            atmosphere: None,
            water: Some(WaterConfig {
                level: -0.5,
                wave_direction: [1.0, 0.5],
                wave_amplitude: 0.5,
                wave_wavelength: 10.0,
                wave_speed: 1.5,
                wave_steepness: 0.5,
                water_color: [0.0, 0.3, 0.4],
                deep_color: [0.0, 0.1, 0.2],
                fresnel_power: 2.5,
                refraction_scale: 0.03,
                reflectivity: 0.5,
            }),
            clouds: None,
            god_ray: None,
            objects: vec![],
        };
        let ron = ron::ser::to_string(&desc).expect("should serialize");
        let parsed = SceneDescription::from_ron(&ron).expect("should deserialize");
        assert_eq!(parsed, desc);
    }

    #[test]
    fn terrain_config_roundtrips_through_ron() {
        let desc = SceneDescription {
            name: "Roundtrip".into(),
            camera: CameraConfig::default(),
            lights: vec![],
            ambient: 0.0,
            terrain: Some(TerrainConfig {
                source: TerrainSource::Procedural {
                    seed: 7,
                    frequency: 0.2,
                    amplitude: 16.0,
                },
                geometry: TerrainGeometry {
                    extent: 128.0,
                    chunk_size: 32,
                    max_lod: 2,
                },
                splatmap: Some("assets/terrain/splatmap.png".into()),
                layers: vec![
                    TerrainLayerConfig {
                        albedo: [0.8, 0.2, 0.2, 1.0],
                        roughness: 0.9,
                        metallic: 0.0,
                        ..Default::default()
                    },
                    TerrainLayerConfig {
                        albedo: [0.2, 0.8, 0.2, 1.0],
                        roughness: 0.8,
                        metallic: 0.0,
                        ..Default::default()
                    },
                    TerrainLayerConfig {
                        albedo: [0.2, 0.2, 0.8, 1.0],
                        roughness: 0.7,
                        metallic: 0.0,
                        ..Default::default()
                    },
                    TerrainLayerConfig {
                        albedo: [0.8, 0.8, 0.2, 1.0],
                        roughness: 0.6,
                        metallic: 0.0,
                        ..Default::default()
                    },
                ],
            }),
            atmosphere: None,
            water: None,
            clouds: None,
            god_ray: None,
            objects: vec![],
        };
        let ron = ron::ser::to_string(&desc).expect("should serialize");
        let parsed = SceneDescription::from_ron(&ron).expect("should deserialize");
        assert_eq!(parsed, desc);
    }

    #[test]
    fn parse_scene_with_cloud_config() {
        let ron = r#"
            SceneDescription(
                name: "Clouds",
                camera: (position: (0.0, 0.0, 0.0)),
                clouds: Some((
                    bottom_altitude: 60.0,
                    top_altitude: 100.0,
                    coverage: 0.6,
                    density: 1.2,
                    wind_direction: (1.0, 0.5),
                    wind_speed: 3.0,
                )),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        let clouds = scene.clouds.expect("clouds should be present");
        assert_eq!(clouds.bottom_altitude, 60.0);
        assert_eq!(clouds.top_altitude, 100.0);
        assert_eq!(clouds.coverage, 0.6);
        assert_eq!(clouds.density, 1.2);
        assert_eq!(clouds.wind_direction, [1.0, 0.5]);
        assert_eq!(clouds.wind_speed, 3.0);
    }

    #[test]
    fn scene_without_clouds_defaults_to_none() {
        let ron = r#"
            SceneDescription(
                name: "No Clouds",
                camera: (position: (0.0, 0.0, 0.0)),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert!(scene.clouds.is_none());
    }

    #[test]
    fn cloud_config_roundtrips_through_ron() {
        let desc = SceneDescription {
            name: "Cloud Roundtrip".into(),
            camera: CameraConfig::default(),
            lights: vec![],
            ambient: 0.0,
            terrain: None,
            atmosphere: None,
            water: None,
            clouds: Some(CloudConfig {
                bottom_altitude: 60.0,
                top_altitude: 100.0,
                coverage: 0.6,
                density: 1.2,
                wind_direction: [1.0, 0.5],
                wind_speed: 3.0,
            }),
            god_ray: None,
            objects: vec![],
        };
        let ron = ron::ser::to_string(&desc).expect("should serialize");
        let parsed = SceneDescription::from_ron(&ron).expect("should deserialize");
        assert_eq!(parsed, desc);
    }

    #[test]
    fn parse_scene_with_godray_config() {
        let ron = r#"
            SceneDescription(
                name: "God Rays",
                camera: (position: (0.0, 0.0, 0.0)),
                god_ray: Some((
                    samples: 80,
                    density: 0.6,
                    decay: 0.92,
                    weight: 0.7,
                    exposure: 0.4,
                )),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        let gr = scene.god_ray.expect("god_ray should be present");
        assert_eq!(gr.samples, 80);
        assert_eq!(gr.density, 0.6);
        assert_eq!(gr.decay, 0.92);
        assert_eq!(gr.weight, 0.7);
        assert_eq!(gr.exposure, 0.4);
    }

    #[test]
    fn scene_without_godray_defaults_to_none() {
        let ron = r#"
            SceneDescription(
                name: "No God Ray",
                camera: (position: (0.0, 0.0, 0.0)),
            )
        "#;
        let scene = SceneDescription::from_ron(ron).expect("should parse");
        assert!(scene.god_ray.is_none());
    }

    #[test]
    fn godray_config_roundtrips_through_ron() {
        let desc = SceneDescription {
            name: "GodRay Roundtrip".into(),
            camera: CameraConfig::default(),
            lights: vec![],
            ambient: 0.0,
            terrain: None,
            atmosphere: None,
            water: None,
            clouds: None,
            god_ray: Some(GodRayConfig {
                samples: 80,
                density: 0.6,
                decay: 0.92,
                weight: 0.7,
                exposure: 0.4,
            }),
            objects: vec![],
        };
        let ron = ron::ser::to_string(&desc).expect("should serialize");
        let parsed = SceneDescription::from_ron(&ron).expect("should deserialize");
        assert_eq!(parsed, desc);
    }
}

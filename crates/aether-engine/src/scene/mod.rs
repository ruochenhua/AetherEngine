//! Scene management module.
//!
//! Data types for describing 3D scenes declaratively. Scenes are serialized
//! as RON files and loaded by the Launcher.

pub mod loader;
pub mod serializer;

use crate::renderer::light::LightType;
use serde::{Deserialize, Serialize};

/// Mesh reference — either a built-in shape or an external file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MeshRef {
    /// Built-in mesh identified by name ("cube", "sphere", "quad").
    Builtin(String),
    /// External mesh file path.
    File(String),
}

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

/// FlyCamera initial parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CameraConfig {
    /// World-space starting position [x, y, z].
    pub position: [f32; 3],
    /// Initial yaw angle in radians.
    #[serde(default)]
    pub yaw: f32,
    /// Initial pitch angle in radians.
    #[serde(default)]
    pub pitch: f32,
    /// Movement speed (units per second).
    #[serde(default = "default_camera_speed")]
    pub speed: f32,
    /// Vertical field of view in degrees.
    #[serde(default = "default_fov")]
    pub fov: f32,
}

fn default_camera_speed() -> f32 {
    4.0
}
fn default_fov() -> f32 {
    45.0
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            position: [3.0, 3.0, 3.0],
            yaw: -2.356,
            pitch: -0.785,
            speed: default_camera_speed(),
            fov: default_fov(),
        }
    }
}

/// Light configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LightConfig {
    /// Type of light.
    pub light_type: LightType,
    /// Light direction [x, y, z] (for Directional lights).
    #[serde(default)]
    pub direction: [f32; 3],
    /// Light color [r, g, b].
    #[serde(default = "default_light_color")]
    pub color: [f32; 3],
    /// Light intensity.
    #[serde(default = "default_intensity")]
    pub intensity: f32,
}

fn default_light_color() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn default_intensity() -> f32 {
    1.0
}

impl Default for LightConfig {
    fn default() -> Self {
        Self {
            light_type: LightType::Directional,
            direction: [0.0, -1.0, 0.0],
            color: default_light_color(),
            intensity: default_intensity(),
        }
    }
}

/// Object (renderable entity) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectConfig {
    /// Human-readable name for debugging.
    #[serde(default)]
    pub name: String,
    /// Mesh reference.
    pub mesh: MeshRef,
    /// Transform.
    #[serde(default)]
    pub transform: TransformConfig,
    /// PBR material parameters.
    #[serde(default)]
    pub material: MaterialConfig,
}

/// Transform data for an object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransformConfig {
    /// Translation [x, y, z].
    #[serde(default)]
    pub translation: [f32; 3],
    /// Rotation quaternion [x, y, z, w].
    #[serde(default = "default_rotation")]
    pub rotation: [f32; 4],
    /// Scale [x, y, z].
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}
fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: default_rotation(),
            scale: default_scale(),
        }
    }
}

/// PBR material parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaterialConfig {
    /// Albedo color [r, g, b, a].
    #[serde(default = "default_albedo")]
    pub albedo: [f32; 4],
    /// Surface roughness (0 = mirror, 1 = matte).
    #[serde(default)]
    pub roughness: f32,
    /// Metalness (0 = dielectric, 1 = metal).
    #[serde(default)]
    pub metallic: f32,
}

fn default_albedo() -> [f32; 4] {
    [0.8, 0.8, 0.8, 1.0]
}

impl Default for MaterialConfig {
    fn default() -> Self {
        Self {
            albedo: default_albedo(),
            roughness: 0.5,
            metallic: 0.0,
        }
    }
}

/// Terrain configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerrainConfig {
    /// Height data source.
    pub source: TerrainSource,
    /// Geometry generation strategy.
    #[serde(default)]
    pub geometry: TerrainGeometry,
    /// Optional splat map texture path.
    #[serde(default)]
    pub splatmap: Option<String>,
    /// Material layers for splatting.
    #[serde(default = "default_terrain_layers")]
    pub layers: Vec<TerrainLayerConfig>,
}

fn default_terrain_layers() -> Vec<TerrainLayerConfig> {
    vec![
        TerrainLayerConfig::default(),
        TerrainLayerConfig::default(),
        TerrainLayerConfig::default(),
        TerrainLayerConfig::default(),
    ]
}

/// Configuration for a single terrain material layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerrainLayerConfig {
    /// Albedo color [r, g, b, a].
    #[serde(default = "default_layer_albedo")]
    pub albedo: [f32; 4],
    /// Surface roughness.
    #[serde(default = "default_layer_roughness")]
    pub roughness: f32,
    /// Surface metallic.
    #[serde(default = "default_layer_metallic")]
    pub metallic: f32,
    /// Optional albedo texture path.
    #[serde(default)]
    pub albedo_texture: Option<String>,
    /// Optional normal map path.
    #[serde(default)]
    pub normal_texture: Option<String>,
    /// Optional packed roughness/metallic texture path.
    #[serde(default)]
    pub roughness_metallic_texture: Option<String>,
}

fn default_layer_albedo() -> [f32; 4] {
    [0.5, 0.5, 0.5, 1.0]
}
fn default_layer_roughness() -> f32 {
    0.8
}
fn default_layer_metallic() -> f32 {
    0.0
}

impl Default for TerrainLayerConfig {
    fn default() -> Self {
        Self {
            albedo: default_layer_albedo(),
            roughness: default_layer_roughness(),
            metallic: default_layer_metallic(),
            albedo_texture: None,
            normal_texture: None,
            roughness_metallic_texture: None,
        }
    }
}

/// Height data source for terrain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TerrainSource {
    /// Load height from an image file (grayscale heightmap).
    Heightmap(String),
    /// Procedurally generated height field.
    Procedural {
        /// Random seed.
        seed: u64,
        /// Base noise frequency.
        #[serde(default = "default_noise_frequency")]
        frequency: f32,
        /// Maximum displacement amplitude.
        #[serde(default = "default_noise_amplitude")]
        amplitude: f32,
    },
}

fn default_noise_frequency() -> f32 {
    0.05
}
fn default_noise_amplitude() -> f32 {
    32.0
}

/// Geometry generation strategy for terrain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerrainGeometry {
    /// World-space half-extent from the origin.
    #[serde(default = "default_terrain_extent")]
    pub extent: f32,
    /// Number of vertices along each chunk edge.
    #[serde(default = "default_terrain_chunk_size")]
    pub chunk_size: u32,
    /// Maximum LOD level (0 = single chunk).
    #[serde(default = "default_terrain_max_lod")]
    pub max_lod: u32,
}

fn default_terrain_extent() -> f32 {
    256.0
}
fn default_terrain_chunk_size() -> u32 {
    64
}
fn default_terrain_max_lod() -> u32 {
    4
}

impl Default for TerrainGeometry {
    fn default() -> Self {
        Self {
            extent: default_terrain_extent(),
            chunk_size: default_terrain_chunk_size(),
            max_lod: default_terrain_max_lod(),
        }
    }
}

/// Physical atmosphere configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtmosphereConfig {
    /// Direction toward the sun [x, y, z]. Defaults to a low sun angle.
    #[serde(default = "default_sun_direction")]
    pub sun_direction: [f32; 3],
    /// Planet radius in world units. The camera is assumed to sit on the surface.
    #[serde(default = "default_planet_radius")]
    pub planet_radius: f32,
    /// Atmosphere shell thickness above the planet surface.
    #[serde(default = "default_atmosphere_height")]
    pub atmosphere_height: f32,
    /// Rayleigh scattering coefficients (RGB).
    #[serde(default = "default_rayleigh_scattering")]
    pub rayleigh_scattering: [f32; 3],
    /// Rayleigh density scale height.
    #[serde(default = "default_rayleigh_scale_height")]
    pub rayleigh_scale_height: f32,
    /// Mie scattering coefficients (RGB).
    #[serde(default = "default_mie_scattering")]
    pub mie_scattering: [f32; 3],
    /// Mie density scale height.
    #[serde(default = "default_mie_scale_height")]
    pub mie_scale_height: f32,
    /// Mie asymmetry parameter (g) in [-1, 1].
    #[serde(default = "default_mie_asymmetry")]
    pub mie_asymmetry: f32,
    /// Sun intensity multiplier.
    #[serde(default = "default_sun_intensity")]
    pub sun_intensity: f32,
}

impl Default for AtmosphereConfig {
    fn default() -> Self {
        Self {
            sun_direction: default_sun_direction(),
            planet_radius: default_planet_radius(),
            atmosphere_height: default_atmosphere_height(),
            rayleigh_scattering: default_rayleigh_scattering(),
            rayleigh_scale_height: default_rayleigh_scale_height(),
            mie_scattering: default_mie_scattering(),
            mie_scale_height: default_mie_scale_height(),
            mie_asymmetry: default_mie_asymmetry(),
            sun_intensity: default_sun_intensity(),
        }
    }
}

fn default_sun_direction() -> [f32; 3] {
    [0.0, 0.2, -1.0]
}

fn default_planet_radius() -> f32 {
    6360.0
}

fn default_atmosphere_height() -> f32 {
    100.0
}

fn default_rayleigh_scattering() -> [f32; 3] {
    [0.005802, 0.013558, 0.033100]
}

fn default_rayleigh_scale_height() -> f32 {
    8.0
}

fn default_mie_scattering() -> [f32; 3] {
    [0.004000, 0.004000, 0.004000]
}

fn default_mie_scale_height() -> f32 {
    1.2
}

fn default_mie_asymmetry() -> f32 {
    0.758
}

fn default_sun_intensity() -> f32 {
    20.0
}

/// Water surface configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaterConfig {
    /// Water level (world-space Y).
    #[serde(default = "default_water_level")]
    pub level: f32,
    /// Wave travel direction on the XZ plane [x, z].
    #[serde(default = "default_water_wave_direction")]
    pub wave_direction: [f32; 2],
    /// Wave amplitude.
    #[serde(default = "default_water_wave_amplitude")]
    pub wave_amplitude: f32,
    /// Wave wavelength.
    #[serde(default = "default_water_wave_wavelength")]
    pub wave_wavelength: f32,
    /// Wave speed.
    #[serde(default = "default_water_wave_speed")]
    pub wave_speed: f32,
    /// Wave steepness (0 = sine, 1 = sharp crests).
    #[serde(default = "default_water_wave_steepness")]
    pub wave_steepness: f32,
    /// Shallow water color (RGB).
    #[serde(default = "default_water_color")]
    pub water_color: [f32; 3],
    /// Deep water color (RGB).
    #[serde(default = "default_water_deep_color")]
    pub deep_color: [f32; 3],
    /// Fresnel power.
    #[serde(default = "default_water_fresnel_power")]
    pub fresnel_power: f32,
    /// Refraction UV distortion scale.
    #[serde(default = "default_water_refraction_scale")]
    pub refraction_scale: f32,
    /// Reflection intensity multiplier.
    #[serde(default = "default_water_reflectivity")]
    pub reflectivity: f32,
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            level: default_water_level(),
            wave_direction: default_water_wave_direction(),
            wave_amplitude: default_water_wave_amplitude(),
            wave_wavelength: default_water_wave_wavelength(),
            wave_speed: default_water_wave_speed(),
            wave_steepness: default_water_wave_steepness(),
            water_color: default_water_color(),
            deep_color: default_water_deep_color(),
            fresnel_power: default_water_fresnel_power(),
            refraction_scale: default_water_refraction_scale(),
            reflectivity: default_water_reflectivity(),
        }
    }
}

fn default_water_level() -> f32 {
    0.0
}

fn default_water_wave_direction() -> [f32; 2] {
    [1.0, 0.5]
}

fn default_water_wave_amplitude() -> f32 {
    0.3
}

fn default_water_wave_wavelength() -> f32 {
    8.0
}

fn default_water_wave_speed() -> f32 {
    2.0
}

fn default_water_wave_steepness() -> f32 {
    0.6
}

fn default_water_color() -> [f32; 3] {
    [0.0, 0.35, 0.45]
}

fn default_water_deep_color() -> [f32; 3] {
    [0.0, 0.15, 0.25]
}

fn default_water_fresnel_power() -> f32 {
    3.0
}

fn default_water_refraction_scale() -> f32 {
    0.02
}

fn default_water_reflectivity() -> f32 {
    0.6
}

/// Volumetric cloud configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudConfig {
    /// Bottom altitude of the cloud slab (world-space Y).
    #[serde(default = "default_cloud_bottom_altitude")]
    pub bottom_altitude: f32,
    /// Top altitude of the cloud slab (world-space Y).
    #[serde(default = "default_cloud_top_altitude")]
    pub top_altitude: f32,
    /// Cloud coverage threshold in [0, 1].
    #[serde(default = "default_cloud_coverage")]
    pub coverage: f32,
    /// Overall density multiplier.
    #[serde(default = "default_cloud_density")]
    pub density: f32,
    /// Wind direction on the XZ plane [x, z].
    #[serde(default = "default_cloud_wind_direction")]
    pub wind_direction: [f32; 2],
    /// Wind speed in world units per second.
    #[serde(default = "default_cloud_wind_speed")]
    pub wind_speed: f32,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            bottom_altitude: default_cloud_bottom_altitude(),
            top_altitude: default_cloud_top_altitude(),
            coverage: default_cloud_coverage(),
            density: default_cloud_density(),
            wind_direction: default_cloud_wind_direction(),
            wind_speed: default_cloud_wind_speed(),
        }
    }
}

fn default_cloud_bottom_altitude() -> f32 {
    80.0
}

fn default_cloud_top_altitude() -> f32 {
    120.0
}

fn default_cloud_coverage() -> f32 {
    0.5
}

fn default_cloud_density() -> f32 {
    1.0
}

fn default_cloud_wind_direction() -> [f32; 2] {
    [1.0, 0.0]
}

fn default_cloud_wind_speed() -> f32 {
    2.0
}

/// God ray (volumetric light shafts) configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GodRayConfig {
    /// Number of ray-marching samples.
    #[serde(default = "default_godray_samples")]
    pub samples: u32,
    /// Density falloff along the ray.
    #[serde(default = "default_godray_density")]
    pub density: f32,
    /// Decay factor per sample.
    #[serde(default = "default_godray_decay")]
    pub decay: f32,
    /// Intensity weight.
    #[serde(default = "default_godray_weight")]
    pub weight: f32,
    /// Final exposure multiplier.
    #[serde(default = "default_godray_exposure")]
    pub exposure: f32,
}

impl Default for GodRayConfig {
    fn default() -> Self {
        Self {
            samples: default_godray_samples(),
            density: default_godray_density(),
            decay: default_godray_decay(),
            weight: default_godray_weight(),
            exposure: default_godray_exposure(),
        }
    }
}

fn default_godray_samples() -> u32 {
    64
}

fn default_godray_density() -> f32 {
    0.5
}

fn default_godray_decay() -> f32 {
    0.95
}

fn default_godray_weight() -> f32 {
    0.5
}

fn default_godray_exposure() -> f32 {
    0.3
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
        let content = include_str!("../../../../scenes/03_shadow_demo.ron");
        let desc = SceneDescription::from_ron(content).expect("should parse");
        assert_eq!(desc.objects.len(), 7, "Expected 7 objects");
    }

    #[test]
    fn parse_terrain_scene_file() {
        let content = include_str!("../../../../scenes/08_terrain.ron");
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
        let content = include_str!("../../../../scenes/09_terrain.ron");
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

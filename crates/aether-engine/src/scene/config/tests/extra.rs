use super::*;

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
            ..Default::default()
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
            dudv_map: Some("assets/textures/water/waterDUDV.png".into()),
            normal_map: Some("assets/textures/water/waterNormal.png".into()),
            texture_scale: 4.0,
            dudv_strength: 0.02,
            normal_strength: 1.0,
            depth_scale: 0.15,
            flow_speed: [0.03, 0.01],
            flow_speed_2: [-0.02, 0.015],
            secondary_scale: 0.7,
            specular_power: 128.0,
            reflection_enabled: true,
            reflection_resolution_scale: 0.5,
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
                albedo_tiling: 64.0,
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
            planet_radius: 6360.0,
            bottom_altitude: 60.0,
            top_altitude: 100.0,
            coverage: 0.6,
            wind_direction: [1.0, 0.5],
            wind_speed: 3.0,
            quality: CloudQuality::Medium,
            weather_scale: 1.0,
            base_noise_scale: 1.0,
            high_freq_noise_scale: 1.0,
            high_freq_uv_scale: 150.0,
            high_freq_h_scale: 4.0,
            cloud_top_offset: 0.0,
            cloud_type: 0.5,
            max_render_dist: 30000.0,
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

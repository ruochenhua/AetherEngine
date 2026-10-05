//! T2 lighting-frame extraction and deterministic local-light selection.

use crate::ecs::components::Light;
use crate::ecs::World;
use crate::renderer::light::{DirectionalLight, LightType, LocalLight, MAX_LOCAL_LIGHTS};

/// Lighting data that cannot be represented by the current single-sun contract.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LightingError {
    /// The scene contains more than one directional light.
    #[error(
        "MultipleSuns: directional lights on entities {first_entity_bits} and {second_entity_bits}"
    )]
    MultipleSuns {
        /// Stable entity bits for the first sun.
        first_entity_bits: u64,
        /// Stable entity bits for the second sun.
        second_entity_bits: u64,
    },
    /// A light component contains a value that cannot be sent to the GPU.
    #[error("InvalidLight: entity {entity_bits} has invalid {field}")]
    InvalidLight {
        /// ECS entity containing the invalid light.
        entity_bits: u64,
        /// Name of the invalid light field.
        field: &'static str,
    },
}

/// The extracted lighting state consumed by all render passes.
#[derive(Clone, Debug, PartialEq)]
pub struct LightingFrame {
    /// The only directional light, or the engine fallback sun.
    pub sun: DirectionalLight,
    /// Deterministically ordered point and spot lights, capped at 32.
    pub locals: Vec<LocalLight>,
    /// Scene ambient intensity.
    pub ambient: f32,
    /// Number of valid local lights omitted by the stable cap.
    pub dropped_local_lights: usize,
}

impl Default for LightingFrame {
    fn default() -> Self {
        Self {
            sun: DirectionalLight {
                direction: [0.0, -1.0, 0.0],
                _pad: 0.0,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            },
            locals: Vec::new(),
            ambient: 0.0,
            dropped_local_lights: 0,
        }
    }
}

/// Extract lighting from ECS without exposing ECS queries to render passes.
pub fn extract_lighting_frame(world: &World, ambient: f32) -> Result<LightingFrame, LightingError> {
    let mut directional = Vec::new();
    let mut locals = Vec::new();

    for (entity, transform, light) in world
        .query::<(hecs::Entity, &crate::ecs::components::Transform, &Light)>()
        .iter()
    {
        let entity_bits = entity.to_bits().get();
        if !light
            .color
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
        {
            return Err(LightingError::InvalidLight {
                entity_bits,
                field: "color",
            });
        }
        if !light.intensity.is_finite() || light.intensity < 0.0 {
            return Err(LightingError::InvalidLight {
                entity_bits,
                field: "intensity",
            });
        }
        if !transform.translation.is_finite() {
            return Err(LightingError::InvalidLight {
                entity_bits,
                field: "position",
            });
        }
        let direction = (transform.rotation * glam::Vec3::NEG_Y).normalize_or_zero();
        match light.light_type {
            LightType::Directional => {
                if !direction.is_finite() || direction == glam::Vec3::ZERO {
                    return Err(LightingError::InvalidLight {
                        entity_bits,
                        field: "direction",
                    });
                }
                directional.push((
                    entity_bits,
                    DirectionalLight {
                        direction: direction.to_array(),
                        _pad: 0.0,
                        color: light.color,
                        intensity: light.intensity,
                    },
                ));
            }
            LightType::Point | LightType::Spot => {
                if !light.range.is_finite() || light.range <= 0.0 {
                    return Err(LightingError::InvalidLight {
                        entity_bits,
                        field: "range",
                    });
                }
                if light.light_type == LightType::Spot
                    && (!direction.is_finite()
                        || direction == glam::Vec3::ZERO
                        || !light.inner_cone_angle.is_finite()
                        || !light.outer_cone_angle.is_finite()
                        || light.inner_cone_angle < 0.0
                        || light.inner_cone_angle > light.outer_cone_angle
                        || light.outer_cone_angle > std::f32::consts::PI)
                {
                    return Err(LightingError::InvalidLight {
                        entity_bits,
                        field: "spot_cone",
                    });
                }
                locals.push(LocalLight {
                    entity_bits,
                    position: transform.translation.to_array(),
                    range: light.range,
                    color: light.color,
                    intensity: light.intensity,
                    direction: if light.light_type == LightType::Spot {
                        direction.to_array()
                    } else {
                        [0.0; 3]
                    },
                    inner_cos: light.inner_cone_angle.cos(),
                    outer_cos: light.outer_cone_angle.cos(),
                    light_type: light.light_type,
                });
            }
        }
    }

    directional.sort_by_key(|(entity_bits, _)| *entity_bits);
    if let (Some((first_entity_bits, _)), Some((second_entity_bits, _))) =
        (directional.first(), directional.get(1))
    {
        return Err(LightingError::MultipleSuns {
            first_entity_bits: *first_entity_bits,
            second_entity_bits: *second_entity_bits,
        });
    }
    locals.sort_by_key(|light| light.entity_bits);
    let dropped_local_lights = locals.len().saturating_sub(MAX_LOCAL_LIGHTS);
    locals.truncate(MAX_LOCAL_LIGHTS);

    Ok(LightingFrame {
        sun: directional
            .into_iter()
            .next()
            .map(|(_, light)| light)
            .unwrap_or_else(|| DirectionalLight {
                direction: [0.0, -1.0, 0.0],
                _pad: 0.0,
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            }),
        locals,
        ambient,
        dropped_local_lights,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::{Light, Name, Transform};
    use crate::ecs::World;
    use crate::renderer::light::{LightType, MAX_LOCAL_LIGHTS};

    #[test]
    fn extraction_sorts_local_lights_and_preserves_distinct_colors() {
        let mut world = World::new();
        let red = world.spawn((
            Transform {
                translation: glam::Vec3::new(-2.0, 1.0, 0.0),
                ..Default::default()
            },
            Light::point([1.0, 0.0, 0.0], 3.0),
            Name("RedLocal".into()),
        ));
        let green = world.spawn((
            Transform {
                translation: glam::Vec3::new(0.0, 1.0, 0.0),
                ..Default::default()
            },
            Light::point([0.0, 1.0, 0.0], 3.0),
            Name("GreenLocal".into()),
        ));
        let blue = world.spawn((
            Transform {
                translation: glam::Vec3::new(2.0, 1.0, 0.0),
                ..Default::default()
            },
            Light::point([0.0, 0.0, 1.0], 3.0),
            Name("BlueLocal".into()),
        ));

        let frame = extract_lighting_frame(&world, 0.05).unwrap();
        assert_eq!(frame.locals.len(), 3);
        assert!(frame
            .locals
            .windows(2)
            .all(|pair| pair[0].entity_bits < pair[1].entity_bits));
        assert_eq!(frame.locals[0].entity_bits, red.to_bits().get());
        assert_eq!(frame.locals[1].entity_bits, green.to_bits().get());
        assert_eq!(frame.locals[2].entity_bits, blue.to_bits().get());
        assert_eq!(frame.locals[0].color, [1.0, 0.0, 0.0]);
        assert_eq!(frame.locals[1].color, [0.0, 1.0, 0.0]);
        assert_eq!(frame.locals[2].color, [0.0, 0.0, 1.0]);
        assert_eq!(frame.locals[0].light_type, LightType::Point);
        assert_eq!(frame.dropped_local_lights, 0);
        assert_eq!(MAX_LOCAL_LIGHTS, 32);
    }

    #[test]
    fn extraction_accepts_zero_and_one_local_light() {
        let mut world = World::new();
        assert!(extract_lighting_frame(&world, 0.0)
            .unwrap()
            .locals
            .is_empty());

        world.spawn((
            crate::ecs::components::Transform::default(),
            Light::point([0.7, 0.4, 0.2], 2.0),
        ));
        let frame = extract_lighting_frame(&world, 0.0).unwrap();
        assert_eq!(frame.locals.len(), 1);
        assert_eq!(frame.dropped_local_lights, 0);
    }

    #[test]
    fn extraction_caps_local_lights_and_reports_dropped_count() {
        let mut world = World::new();
        for index in 0..33 {
            world.spawn((
                Transform {
                    translation: glam::Vec3::new(index as f32, 1.0, 0.0),
                    ..Default::default()
                },
                Light::point([1.0, 0.5, 0.25], 1.0),
                Name(format!("Local{index}")),
            ));
        }

        let frame = extract_lighting_frame(&world, 0.05).unwrap();
        assert_eq!(frame.locals.len(), MAX_LOCAL_LIGHTS);
        assert_eq!(frame.dropped_local_lights, 1);
        assert!(frame
            .locals
            .windows(2)
            .all(|pair| pair[0].entity_bits < pair[1].entity_bits));
    }

    #[test]
    fn extraction_rejects_multiple_suns_with_stable_entity_diagnostic() {
        let mut world = World::new();
        let first = world.spawn((Transform::default(), Light::directional([1.0; 3], 1.0)));
        let second = world.spawn((Transform::default(), Light::directional([1.0; 3], 1.0)));

        let error = extract_lighting_frame(&world, 0.05).unwrap_err();
        let LightingError::MultipleSuns {
            first_entity_bits,
            second_entity_bits,
        } = error
        else {
            panic!("unexpected error: {error:?}");
        };
        assert_eq!(
            first_entity_bits,
            first.to_bits().get().min(second.to_bits().get())
        );
        assert_eq!(
            second_entity_bits,
            first.to_bits().get().max(second.to_bits().get())
        );
    }

    #[test]
    fn extraction_rejects_invalid_local_light_with_entity_and_field() {
        let mut world = World::new();
        let mut light = Light::point([1.0, 1.0, 1.0], 1.0);
        light.range = f32::NAN;
        let entity = world.spawn((Transform::default(), light));

        assert_eq!(
            extract_lighting_frame(&world, 0.05).unwrap_err(),
            LightingError::InvalidLight {
                entity_bits: entity.to_bits().get(),
                field: "range",
            }
        );
    }

    #[test]
    fn extraction_rejects_invalid_spot_cone_with_entity_and_field() {
        let mut world = World::new();
        let mut light = Light::spot([1.0, 1.0, 1.0], 1.0);
        light.inner_cone_angle = 0.8;
        light.outer_cone_angle = 0.4;
        let entity = world.spawn((Transform::default(), light));

        assert_eq!(
            extract_lighting_frame(&world, 0.05).unwrap_err(),
            LightingError::InvalidLight {
                entity_bits: entity.to_bits().get(),
                field: "spot_cone",
            }
        );
    }
}

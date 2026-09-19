//! T2 lighting-frame extraction and deterministic local-light selection.

use crate::ecs::components::{Light, Transform};
use crate::ecs::World;
use crate::renderer::light::{DirectionalLight, LightType, LocalLight, MAX_LOCAL_LIGHTS};

/// The extracted lighting state consumed by all render passes.
#[derive(Clone, Debug, PartialEq)]
pub struct LightingFrame {
    /// The first directional light, or the engine fallback sun.
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
pub fn extract_lighting_frame(world: &World, ambient: f32) -> LightingFrame {
    let mut directional = Vec::new();
    let mut locals = Vec::new();

    for (entity, transform, light) in world.query::<(hecs::Entity, &Transform, &Light)>().iter() {
        let direction = (transform.rotation * glam::Vec3::NEG_Y).normalize_or_zero();
        match light.light_type {
            LightType::Directional => directional.push((
                entity.to_bits().get(),
                DirectionalLight {
                    direction: direction.to_array(),
                    _pad: 0.0,
                    color: light.color,
                    intensity: light.intensity,
                },
            )),
            LightType::Point | LightType::Spot => locals.push(LocalLight {
                entity_bits: entity.to_bits().get(),
                position: transform.translation.to_array(),
                range: light.range,
                color: light.color,
                intensity: light.intensity,
                direction: direction.to_array(),
                inner_cos: light.inner_cone_angle.cos(),
                outer_cos: light.outer_cone_angle.cos(),
                light_type: light.light_type,
            }),
        }
    }

    directional.sort_by_key(|(entity_bits, _)| *entity_bits);
    locals.sort_by_key(|light| light.entity_bits);
    let dropped_local_lights = locals.len().saturating_sub(MAX_LOCAL_LIGHTS);
    locals.truncate(MAX_LOCAL_LIGHTS);

    LightingFrame {
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
    }
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

        let frame = extract_lighting_frame(&world, 0.05);
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

        let frame = extract_lighting_frame(&world, 0.05);
        assert_eq!(frame.locals.len(), MAX_LOCAL_LIGHTS);
        assert_eq!(frame.dropped_local_lights, 1);
        assert!(frame
            .locals
            .windows(2)
            .all(|pair| pair[0].entity_bits < pair[1].entity_bits));
    }
}

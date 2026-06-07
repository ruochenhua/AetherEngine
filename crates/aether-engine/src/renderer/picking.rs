//! CPU ray-casting picking system.
//!
//! Casts a ray from the camera through the mouse position and tests against
//! entity AABBs in the ECS World. The closest hit entity gets the `Selected`
/// component.

use crate::ecs::components::{MeshHandle, Selected, Transform};
use crate::ecs::World;
use crate::math::Aabb;
use glam::{Mat4, Vec3, Vec4};

/// A ray in world space.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    /// Ray origin.
    pub origin: Vec3,
    /// Normalized ray direction.
    pub dir: Vec3,
}

/// Build a world-space ray from the camera through a screen pixel.
///
/// `mouse_x`, `mouse_y` are in screen pixels (top-left origin).
/// `width`, `height` are the viewport dimensions in pixels.
pub fn screen_ray(
    mouse_x: f32,
    mouse_y: f32,
    width: f32,
    height: f32,
    view: Mat4,
    proj: Mat4,
    camera_pos: Vec3,
) -> Ray {
    let x_ndc = (2.0 * mouse_x / width) - 1.0;
    let y_ndc = 1.0 - (2.0 * mouse_y / height);

    let clip = Vec4::new(x_ndc, y_ndc, -1.0, 1.0);
    let inv_proj = proj.inverse();
    let eye = inv_proj * clip;
    let eye = Vec4::new(eye.x, eye.y, -1.0, 0.0);

    let inv_view = view.inverse();
    let world = inv_view * eye;
    let dir = Vec3::new(world.x, world.y, world.z).normalize();

    Ray {
        origin: camera_pos,
        dir,
    }
}

/// Ray-AABB intersection (slab method).
///
/// Returns the distance along the ray to the intersection point, or `None`
/// if the ray misses the AABB.
pub fn ray_aabb_intersect(ray: &Ray, aabb: &Aabb, model_matrix: Mat4) -> Option<f32> {
    // Transform ray to model space
    let inv_model = model_matrix.inverse();
    let local_origin = inv_model.transform_point3(ray.origin);
    let local_dir = inv_model.transform_vector3(ray.dir).normalize();

    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;

    for i in 0..3 {
        let o = local_origin[i];
        let d = local_dir[i];
        let min = aabb.min[i];
        let max = aabb.max[i];

        if d.abs() < 1e-6 {
            // Ray is parallel to this slab
            if o < min || o > max {
                return None;
            }
        } else {
            let inv_d = 1.0 / d;
            let mut t1 = (min - o) * inv_d;
            let mut t2 = (max - o) * inv_d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }

    if tmax < 0.0 {
        return None; // AABB is behind the ray origin
    }

    Some(if tmin < 0.0 { tmax } else { tmin })
}

/// Pick the closest visible entity under the mouse cursor.
///
/// Queries the world for entities with `(Transform, MeshHandle)` and tests
/// each against the ray. If an entity is hit, it becomes selected and any
/// previous selection is removed. If nothing is hit, the current selection
/// is kept (clicking empty space does not deselect).
pub fn pick_entity(world: &mut World, ray: &Ray) -> Option<hecs::Entity> {
    let mut closest: Option<(hecs::Entity, f32)> = None;

    for (entity, transform, mesh_handle) in
        world.query::<(hecs::Entity, &Transform, &MeshHandle)>().iter()
    {
        let model = Mat4::from_scale_rotation_translation(
            transform.scale,
            transform.rotation,
            transform.translation,
        );
        if let Some(t) = ray_aabb_intersect(ray, &mesh_handle.mesh.aabb, model) {
            if closest.map_or(true, |(_, d)| t < d) {
                closest = Some((entity, t));
            }
        }
    }

    // Only change selection when something is actually hit.
    // Clicking empty space keeps the current selection.
    if let Some((entity, _)) = closest {
        // Remove Selected from all entities
        let selected_entities: Vec<hecs::Entity> = world
            .query::<(hecs::Entity, &Selected)>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        for e in selected_entities {
            let _ = world.remove::<(Selected,)>(e);
        }
        // Add Selected to hit entity
        let _ = world.insert(entity, (Selected,));
        Some(entity)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Vec3};

    #[test]
    fn test_ray_aabb_intersect_centered_cube() {
        let ray = Ray {
            origin: Vec3::new(3.0, 3.0, 3.0),
            dir: Vec3::new(-1.0, -1.0, -1.0).normalize(),
        };
        let aabb = Aabb::new(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));
        let model = Mat4::IDENTITY;
        let t = ray_aabb_intersect(&ray, &aabb, model);
        assert!(t.is_some(), "Ray should hit the cube");
        let t = t.unwrap();
        assert!(t > 0.0, "Hit distance should be positive");
        // Expected: from (3,3,3) along (-1,-1,-1), hit at t ≈ 2.5*sqrt(3) ≈ 4.33
        // But actually the face is at 0.5, so t = (3 - 0.5) / (1/sqrt(3)) = 2.5 * sqrt(3) ≈ 4.33
        let expected = 2.5 * 3.0f32.sqrt();
        assert!((t - expected).abs() < 0.01, "Expected t≈{expected}, got {t}");
    }

    #[test]
    fn test_ray_aabb_intersect_miss() {
        let ray = Ray {
            origin: Vec3::new(3.0, 3.0, 3.0),
            dir: Vec3::new(1.0, 0.0, 0.0).normalize(),
        };
        let aabb = Aabb::new(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));
        let model = Mat4::IDENTITY;
        let t = ray_aabb_intersect(&ray, &aabb, model);
        assert!(t.is_none(), "Ray should miss the cube");
    }

    #[test]
    fn test_screen_ray_center() {
        // Camera at (3,3,3) looking at origin
        let camera_pos = Vec3::new(3.0, 3.0, 3.0);
        let view = Mat4::look_at_rh(camera_pos, Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(45.0f32.to_radians(), 1.0, 0.1, 100.0);
        let width = 800.0;
        let height = 600.0;
        let ray = screen_ray(width / 2.0, height / 2.0, width, height, view, proj, camera_pos);
        // Ray direction should point roughly toward origin
        let expected_dir = (Vec3::ZERO - camera_pos).normalize();
        let dot = ray.dir.dot(expected_dir);
        assert!(
            dot > 0.99,
            "Center-screen ray should point toward origin, expected dir={expected_dir:?}, got dir={:?}, dot={dot}",
            ray.dir
        );
    }
}

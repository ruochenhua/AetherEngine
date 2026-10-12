//! CPU ray-casting picking system.
//!
//! Casts a ray from the camera through the mouse position and tests against
//! entity AABBs in the ECS World. The closest hit entity gets the `Selected` component.

use crate::ecs::components::{MeshHandle, Selected, Transform};
use crate::ecs::World;
use crate::math::Aabb;
use glam::{Mat4, Vec2, Vec3, Vec4};

/// A ray in world space.
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    /// Ray origin.
    pub origin: Vec3,
    /// Normalized ray direction.
    pub dir: Vec3,
}

/// A half-open rectangle in physical screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    /// Inclusive top-left corner.
    pub min: Vec2,
    /// Exclusive bottom-right corner.
    pub max: Vec2,
}

impl ScreenRect {
    /// Convert an egui point rectangle to physical pixels.
    pub fn from_points(min: Vec2, max: Vec2, pixels_per_point: f32) -> Option<Self> {
        if !min.is_finite()
            || !max.is_finite()
            || !pixels_per_point.is_finite()
            || pixels_per_point <= 0.0
            || max.x <= min.x
            || max.y <= min.y
        {
            return None;
        }

        Some(Self {
            min: min * pixels_per_point,
            max: max * pixels_per_point,
        })
    }

    /// Test a physical pixel against the half-open rectangle `[min, max)`.
    pub fn contains(self, point: Vec2) -> bool {
        self.is_valid()
            && point.is_finite()
            && point.x >= self.min.x
            && point.y >= self.min.y
            && point.x < self.max.x
            && point.y < self.max.y
    }

    fn is_valid(self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.max.x > self.min.x
            && self.max.y > self.min.y
    }
}

/// Build a full-window projection ray only when the pointer is inside the drop rectangle.
pub fn screen_ray_for_pointer(
    point: Vec2,
    drop_rect: ScreenRect,
    window_size: Vec2,
    view: Mat4,
    proj: Mat4,
    camera_pos: Vec3,
) -> Option<Ray> {
    if !drop_rect.contains(point)
        || !window_size.is_finite()
        || window_size.x <= 0.0
        || window_size.y <= 0.0
        || point.x < 0.0
        || point.y < 0.0
        || point.x >= window_size.x
        || point.y >= window_size.y
        || !camera_pos.is_finite()
    {
        return None;
    }

    let ray = screen_ray(
        point.x,
        point.y,
        window_size.x,
        window_size.y,
        view,
        proj,
        camera_pos,
    );
    if ray.dir.is_finite() && ray.dir.length_squared() > 0.0 {
        Some(ray)
    } else {
        None
    }
}

/// Intersect a ray with a horizontal plane, accepting only forward intersections.
pub fn intersect_horizontal_plane(ray: &Ray, plane_y: f32) -> Option<Vec3> {
    if !ray.origin.is_finite() || !ray.dir.is_finite() || !plane_y.is_finite() {
        return None;
    }

    let denom = ray.dir.y;
    if denom.abs() <= 1.0e-6 {
        return None;
    }
    let t = (plane_y - ray.origin.y) / denom;
    if !t.is_finite() || t <= 0.0 {
        return None;
    }

    let point = ray.origin + ray.dir * t;
    point.is_finite().then_some(point)
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

    for (entity, transform, mesh_handle) in world
        .query::<(hecs::Entity, &Transform, &MeshHandle)>()
        .iter()
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
    use crate::math::{look_at, perspective_projection};

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
        assert!(
            (t - expected).abs() < 0.01,
            "Expected t≈{expected}, got {t}"
        );
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
        let view = look_at(camera_pos, Vec3::ZERO, Vec3::Y);
        let proj = perspective_projection(45.0f32.to_radians(), 1.0, 0.1, 100.0);
        let width = 800.0;
        let height = 600.0;
        let ray = screen_ray(
            width / 2.0,
            height / 2.0,
            width,
            height,
            view,
            proj,
            camera_pos,
        );
        // Ray direction should point roughly toward origin
        let expected_dir = (Vec3::ZERO - camera_pos).normalize();
        let dot = ray.dir.dot(expected_dir);
        assert!(
            dot > 0.99,
            "Center-screen ray should point toward origin, expected dir={expected_dir:?}, got dir={:?}, dot={dot}",
            ray.dir
        );
    }

    #[test]
    fn physical_viewport_rect_scales_points_and_uses_half_open_bounds() {
        let rect =
            ScreenRect::from_points(Vec2::new(10.0, 20.0), Vec2::new(110.0, 70.0), 2.0).unwrap();

        assert_eq!(rect.min, Vec2::new(20.0, 40.0));
        assert_eq!(rect.max, Vec2::new(220.0, 140.0));
        assert!(rect.contains(Vec2::new(20.0, 40.0)));
        assert!(rect.contains(Vec2::new(219.99, 139.99)));
        assert!(!rect.contains(Vec2::new(220.0, 80.0)));
        assert!(!rect.contains(Vec2::new(80.0, 140.0)));
        assert!(!rect.contains(Vec2::new(19.99, 80.0)));
    }

    #[test]
    fn screen_ray_uses_window_coordinates_after_drop_rect_hit_test() {
        let view = look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let projection = perspective_projection(1.0, 1.0, 0.1, 100.0);
        let drop_rect = ScreenRect {
            min: Vec2::new(100.0, 50.0),
            max: Vec2::new(900.0, 850.0),
        };

        let center = screen_ray_for_pointer(
            Vec2::new(500.0, 450.0),
            drop_rect,
            Vec2::new(1000.0, 900.0),
            view,
            projection,
            Vec3::new(0.0, 0.0, 5.0),
        )
        .unwrap();
        assert!(center.dir.dot(Vec3::NEG_Z) > 0.999);
        let off_center = screen_ray_for_pointer(
            Vec2::new(700.0, 450.0),
            drop_rect,
            Vec2::new(1000.0, 900.0),
            view,
            projection,
            Vec3::new(0.0, 0.0, 5.0),
        )
        .unwrap();
        let expected = screen_ray(
            700.0,
            450.0,
            1000.0,
            900.0,
            view,
            projection,
            Vec3::new(0.0, 0.0, 5.0),
        );
        assert!(off_center.dir.dot(expected.dir) > 0.9999);
        assert!(screen_ray_for_pointer(
            Vec2::new(900.0, 450.0),
            drop_rect,
            Vec2::new(1000.0, 900.0),
            view,
            projection,
            Vec3::new(0.0, 0.0, 5.0),
        )
        .is_none());
        assert!(screen_ray_for_pointer(
            Vec2::new(500.0, 450.0),
            drop_rect,
            Vec2::new(0.0, 600.0),
            view,
            projection,
            Vec3::new(0.0, 0.0, 5.0),
        )
        .is_none());
        let oversized_drop_rect = ScreenRect {
            min: Vec2::new(-10.0, -10.0),
            max: Vec2::new(1010.0, 910.0),
        };
        for outside in [
            Vec2::new(-0.01, 10.0),
            Vec2::new(10.0, -0.01),
            Vec2::new(1000.0, 10.0),
            Vec2::new(10.0, 900.0),
        ] {
            assert!(screen_ray_for_pointer(
                outside,
                oversized_drop_rect,
                Vec2::new(1000.0, 900.0),
                view,
                projection,
                Vec3::new(0.0, 0.0, 5.0),
            )
            .is_none());
        }
    }

    #[test]
    fn horizontal_ground_intersection_rejects_parallel_and_non_forward_rays() {
        let point = intersect_horizontal_plane(
            &Ray {
                origin: Vec3::new(-2.0, 4.0, -3.0),
                dir: Vec3::new(0.0, -1.0, 0.0),
            },
            0.0,
        )
        .unwrap();
        assert_eq!(point, Vec3::new(-2.0, 0.0, -3.0));
        assert!(intersect_horizontal_plane(
            &Ray {
                origin: Vec3::new(0.0, 4.0, 0.0),
                dir: Vec3::X,
            },
            0.0,
        )
        .is_none());
        assert!(intersect_horizontal_plane(
            &Ray {
                origin: Vec3::new(0.0, 4.0, 0.0),
                dir: Vec3::Y,
            },
            0.0,
        )
        .is_none());
        assert!(intersect_horizontal_plane(
            &Ray {
                origin: Vec3::ZERO,
                dir: Vec3::NEG_Y,
            },
            0.0,
        )
        .is_none());
    }
}

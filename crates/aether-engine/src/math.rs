//! Math utilities and type aliases.
//!
//! Re-exports `glam` types and provides engine-specific extensions.

pub use glam::*;

/// Convert a `glam::Mat4` to a raw array for GPU uniform buffers.
pub fn mat4_to_array(m: &Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

/// Convert a `glam::Vec3` to a raw array.
pub fn vec3_to_array(v: Vec3) -> [f32; 3] {
    v.to_array()
}

/// Convert a `glam::Vec4` to a raw array.
pub fn vec4_to_array(v: Vec4) -> [f32; 4] {
    v.to_array()
}

/// Create a standard perspective projection matrix.
pub fn perspective_projection(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    Mat4::perspective_rh(fov_y_rad, aspect, near, far)
}

/// Create a look-at view matrix.
pub fn look_at(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    Mat4::look_at_rh(eye, center, up)
}

// -----------------------------------------------------------------------------
// Planes and frustums
// -----------------------------------------------------------------------------

/// A plane in 3D space stored as a normal and a signed distance.
///
/// The plane equation is `normal · p + distance = 0`. Normals in a
/// [`Frustum`] point outward, so the inside half-space is
/// `normal · p + distance <= 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// Unit normal vector pointing outward from the volume.
    pub normal: Vec3,
    /// Signed distance from the origin along the normal.
    pub distance: f32,
}

impl Plane {
    /// Create a plane from a normal and a point on the plane.
    pub fn from_point_normal(point: Vec3, normal: Vec3) -> Self {
        let n = normal.normalize();
        Self {
            normal: n,
            distance: -n.dot(point),
        }
    }

    /// Signed distance from `point` to the plane.
    ///
    /// Positive values mean the point is on the outside of the plane
    /// (the side the normal points to). Negative values are inside.
    pub fn signed_distance(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }

    /// Normalize the plane so the normal has unit length.
    pub fn normalize(&mut self) {
        let len = self.normal.length();
        if len > 1e-6 {
            self.normal /= len;
            self.distance /= len;
        }
    }
}

/// Visibility result of an AABB against a frustum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CullingVisibility {
    /// The AABB is fully inside the frustum.
    Visible,
    /// The AABB is fully outside at least one frustum plane.
    Invisible,
    /// The AABB intersects the frustum boundary.
    Partial,
}

/// A view frustum defined by six outward-pointing planes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    /// Left, right, bottom, top, near, far planes.
    pub planes: [Plane; 6],
}

impl Frustum {
    /// Index of the left clipping plane.
    pub const LEFT: usize = 0;
    /// Index of the right clipping plane.
    pub const RIGHT: usize = 1;
    /// Index of the bottom clipping plane.
    pub const BOTTOM: usize = 2;
    /// Index of the top clipping plane.
    pub const TOP: usize = 3;
    /// Index of the near clipping plane.
    pub const NEAR: usize = 4;
    /// Index of the far clipping plane.
    pub const FAR: usize = 5;

    /// Build a frustum from a combined view-projection matrix.
    ///
    /// `view_proj` should be `projection * view` (glam convention). The
    /// resulting planes point outward from the visible volume.
    pub fn from_view_projection(view_proj: Mat4) -> Self {
        // glam stores matrices in column-major order; transpose so that m[i]
        // is the i-th mathematical row, which is what the clipping-plane
        // extraction formula expects.
        let m = view_proj.transpose().to_cols_array_2d();

        let mut planes = [
            // Left: row 0 + row 3, negated so the normal points outward.
            Plane {
                normal: Vec3::new(
                    -(m[0][0] + m[3][0]),
                    -(m[0][1] + m[3][1]),
                    -(m[0][2] + m[3][2]),
                ),
                distance: -(m[0][3] + m[3][3]),
            },
            // Right: -(row 3 - row 0) = row 0 - row 3
            Plane {
                normal: Vec3::new(m[0][0] - m[3][0], m[0][1] - m[3][1], m[0][2] - m[3][2]),
                distance: m[0][3] - m[3][3],
            },
            // Bottom: row 1 + row 3, negated
            Plane {
                normal: Vec3::new(
                    -(m[1][0] + m[3][0]),
                    -(m[1][1] + m[3][1]),
                    -(m[1][2] + m[3][2]),
                ),
                distance: -(m[1][3] + m[3][3]),
            },
            // Top: row 1 - row 3
            Plane {
                normal: Vec3::new(m[1][0] - m[3][0], m[1][1] - m[3][1], m[1][2] - m[3][2]),
                distance: m[1][3] - m[3][3],
            },
            // Near: row 2 + row 3, negated
            Plane {
                normal: Vec3::new(
                    -(m[2][0] + m[3][0]),
                    -(m[2][1] + m[3][1]),
                    -(m[2][2] + m[3][2]),
                ),
                distance: -(m[2][3] + m[3][3]),
            },
            // Far: row 2 - row 3
            Plane {
                normal: Vec3::new(m[2][0] - m[3][0], m[2][1] - m[3][1], m[2][2] - m[3][2]),
                distance: m[2][3] - m[3][3],
            },
        ];

        for plane in &mut planes {
            plane.normalize();
        }

        Self { planes }
    }
}

// -----------------------------------------------------------------------------
// Axis-aligned bounding box
// -----------------------------------------------------------------------------

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl PartialEq for Aabb {
    fn eq(&self, other: &Self) -> bool {
        self.min.abs_diff_eq(other.min, 1e-6) && self.max.abs_diff_eq(other.max, 1e-6)
    }
}

impl Aabb {
    /// Create an AABB from min/max corners.
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Alias for [`Aabb::new`] matching the issue naming.
    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        Self::new(min, max)
    }

    /// Compute the center of the AABB.
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Compute the half-extents (size / 2).
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Compute the eight corners of the AABB.
    pub fn corners(&self) -> [Vec3; 8] {
        [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ]
    }

    /// Build an AABB from a slice of vertex positions.
    pub fn from_mesh(positions: &[[f32; 3]]) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for p in positions {
            let v = Vec3::from_array(*p);
            min = min.min(v);
            max = max.max(v);
        }
        Self::new(min, max)
    }

    /// Transform an AABB by a matrix and compute the tight axis-aligned bounds
    /// of the transformed corners.
    pub fn transform(&self, matrix: Mat4) -> Self {
        let corners = self.corners();
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for c in corners {
            let t = matrix.transform_point3(c);
            min = min.min(t);
            max = max.max(t);
        }
        Self::new(min, max)
    }

    /// Test this AABB against a frustum.
    ///
    /// Returns [`CullingVisibility::Visible`] if the box is fully inside,
    /// [`CullingVisibility::Invisible`] if it is fully outside at least one
    /// plane, and [`CullingVisibility::Partial`] otherwise.
    pub fn intersects_frustum(&self, frustum: &Frustum) -> CullingVisibility {
        let corners = self.corners();
        let mut fully_inside = true;

        for plane in &frustum.planes {
            let mut all_outside = true;
            let mut any_outside = false;

            for corner in &corners {
                let d = plane.signed_distance(*corner);
                if d <= 0.0 {
                    all_outside = false;
                } else {
                    any_outside = true;
                }
            }

            if all_outside {
                return CullingVisibility::Invisible;
            }

            if any_outside {
                fully_inside = false;
            }
        }

        if fully_inside {
            CullingVisibility::Visible
        } else {
            CullingVisibility::Partial
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_from_min_max() {
        let aabb = Aabb::from_min_max(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0));
        assert!(aabb.center().abs_diff_eq(Vec3::ZERO, 1e-6));
        assert!(aabb
            .half_extents()
            .abs_diff_eq(Vec3::new(1.0, 2.0, 3.0), 1e-6));
    }

    #[test]
    fn aabb_transform_by_matrix() {
        let aabb = Aabb::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 1.0, 1.0),
            Quat::IDENTITY,
            Vec3::new(5.0, 0.0, 0.0),
        );
        let transformed = aabb.transform(transform);
        assert!(transformed
            .min
            .abs_diff_eq(Vec3::new(3.0, -1.0, -1.0), 1e-4));
        assert!(transformed.max.abs_diff_eq(Vec3::new(7.0, 1.0, 1.0), 1e-4));
    }

    #[test]
    fn aabb_from_mesh_and_transform() {
        let positions = vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
        let aabb = Aabb::from_mesh(&positions);
        assert_eq!(aabb.min, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(aabb.max, Vec3::new(1.0, 2.0, 0.0));
    }

    #[test]
    fn aabb_inside_frustum() {
        // Identity VP maps world space to NDC; the unit cube at the origin is inside.
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let aabb = Aabb::from_min_max(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));
        assert_eq!(
            aabb.intersects_frustum(&frustum),
            CullingVisibility::Visible
        );
    }

    #[test]
    fn aabb_outside_frustum() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let aabb = Aabb::from_min_max(Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 1.0));
        assert_eq!(
            aabb.intersects_frustum(&frustum),
            CullingVisibility::Invisible
        );
    }

    #[test]
    fn aabb_intersect_frustum() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let aabb = Aabb::from_min_max(Vec3::new(0.5, -0.5, -0.5), Vec3::new(1.5, 0.5, 0.5));
        assert_eq!(
            aabb.intersects_frustum(&frustum),
            CullingVisibility::Partial
        );
    }

    #[test]
    fn plane_signed_distance() {
        // Plane at z=5 with outward normal +Z. Points beyond z=5 are outside.
        let plane = Plane::from_point_normal(Vec3::new(0.0, 0.0, 5.0), Vec3::Z);
        assert!(plane.signed_distance(Vec3::new(0.0, 0.0, 7.0)) > 0.0);
        assert!(plane.signed_distance(Vec3::new(0.0, 0.0, 3.0)) < 0.0);
        assert!(plane.signed_distance(Vec3::new(1.0, 2.0, 5.0)).abs() < 1e-6);
    }

    #[test]
    fn frustum_from_identity_planes_point_outward() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);

        // Left plane normal should point in -X (outward).
        assert!(frustum.planes[Frustum::LEFT].normal.x < 0.0);
        // Right plane normal should point in +X.
        assert!(frustum.planes[Frustum::RIGHT].normal.x > 0.0);
        // Bottom plane normal should point in -Y.
        assert!(frustum.planes[Frustum::BOTTOM].normal.y < 0.0);
        // Top plane normal should point in +Y.
        assert!(frustum.planes[Frustum::TOP].normal.y > 0.0);
        // Near plane normal should point in -Z.
        assert!(frustum.planes[Frustum::NEAR].normal.z < 0.0);
        // Far plane normal should point in +Z.
        assert!(frustum.planes[Frustum::FAR].normal.z > 0.0);
    }
}

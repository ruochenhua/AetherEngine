use super::ortho_wgpu;
use glam::Vec4;

#[test]
fn orthographic_depth_maps_light_space_bounds_to_zero_and_one() {
    let near_plane = 3.0;
    let far_plane = 11.0;
    let projection = ortho_wgpu(-2.0, 2.0, -2.0, 2.0, near_plane, far_plane);

    let near_clip = projection * Vec4::new(0.0, 0.0, -near_plane, 1.0);
    let far_clip = projection * Vec4::new(0.0, 0.0, -far_plane, 1.0);
    assert!((near_clip.z / near_clip.w).abs() < 1e-6);
    assert!(((far_clip.z / far_clip.w) - 1.0).abs() < 1e-6);
}

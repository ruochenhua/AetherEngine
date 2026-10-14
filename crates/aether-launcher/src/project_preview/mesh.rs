use super::{Limits, Thumbnail};

pub(super) fn project(
    positions: &[[f32; 3]],
    indices: &[[usize; 3]],
    limits: Limits,
) -> Result<Thumbnail, String> {
    if positions.is_empty() || indices.is_empty() {
        return Err("Mesh is empty".into());
    }
    if positions.len() > limits.vertices {
        return Err("Mesh vertex limit exceeded".into());
    }
    if indices.len() > limits.triangles {
        return Err("Generated triangle limit exceeded".into());
    }
    if positions.iter().flatten().any(|p| !p.is_finite()) {
        return Err("Mesh points must be finite".into());
    }
    if indices.iter().flatten().any(|&i| i >= positions.len()) {
        return Err("Malformed mesh index".into());
    }
    let edge = limits.thumbnail_edge.min(384) as usize;
    if edge < 8 {
        return Err("Thumbnail size is too small".into());
    }
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for position in positions {
        for axis in 0..3 {
            low[axis] = low[axis].min(f64::from(position[axis]));
            high[axis] = high[axis].max(f64::from(position[axis]));
        }
    }
    let center = std::array::from_fn::<_, 3, _>(|i| (low[i] + high[i]) * 0.5);
    let points: Vec<[f64; 3]> = positions
        .iter()
        .map(|p| {
            let [x, y, z] = std::array::from_fn(|i| f64::from(p[i]) - center[i]);
            [
                0.8 * x + 0.6 * z,
                -0.3 * x + 0.8660254 * y + 0.4 * z,
                -0.5196152 * x - 0.5 * y + 0.6928203 * z,
            ]
        })
        .collect();
    let mut minimum = [f64::INFINITY; 2];
    let mut maximum = [f64::NEG_INFINITY; 2];
    for point in &points {
        for axis in 0..2 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    let extent = (maximum[0] - minimum[0]).max(maximum[1] - minimum[1]);
    if extent <= f64::EPSILON {
        return Err("Mesh has no visible extent".into());
    }
    let scale = (edge as f64 - 16.) / extent;
    let points: Vec<_> = points
        .iter()
        .map(|p| {
            [
                (p[0] - (minimum[0] + maximum[0]) * 0.5) * scale + edge as f64 * 0.5,
                edge as f64 * 0.5 - (p[1] - (minimum[1] + maximum[1]) * 0.5) * scale,
                p[2] / extent,
            ]
        })
        .collect();
    let mut rgba = [28, 32, 38, 255].repeat(edge * edge);
    let mut depth = vec![f64::NEG_INFINITY; edge * edge];
    let mut work = 0usize;
    let mut visible = false;
    for indices in indices {
        let [a, b, c] = indices.map(|i| points[i]);
        let area = cross(a, b, c);
        if area.abs() < 1e-10 {
            continue;
        }
        let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.) as usize;
        let max_x = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(edge - 1);
        let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.) as usize;
        let max_y = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(edge - 1);
        work = work.saturating_add((max_x - min_x + 1) * (max_y - min_y + 1));
        if work > 16 * 1024 * 1024 {
            return Err("Mesh projection work limit exceeded".into());
        }
        let original = indices.map(|i| glam::DVec3::from_array(positions[i].map(f64::from)));
        let normal = (original[1] - original[0])
            .cross(original[2] - original[0])
            .normalize_or_zero();
        let shade = 0.35
            + 0.65
                * normal
                    .dot(glam::DVec3::new(-0.4, 0.8, 0.6).normalize())
                    .abs();
        let color = [
            (150. * shade) as u8,
            (180. * shade) as u8,
            (175. * shade) as u8,
            255,
        ];
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let point = [x as f64 + 0.5, y as f64 + 0.5, 0.];
                let wa = cross(b, c, point) / area;
                let wb = cross(c, a, point) / area;
                let wc = 1. - wa - wb;
                if wa < -1e-8 || wb < -1e-8 || wc < -1e-8 {
                    continue;
                }
                let z = wa * a[2] + wb * b[2] + wc * c[2];
                let pixel = y * edge + x;
                if z > depth[pixel] {
                    depth[pixel] = z;
                    rgba[pixel * 4..pixel * 4 + 4].copy_from_slice(&color);
                    visible = true;
                }
            }
        }
    }
    if !visible {
        return Err("Mesh has no visible triangles".into());
    }
    Ok(Thumbnail {
        size: [edge, edge],
        rgba,
        dimensions: format!(
            "AABB: {:.3} × {:.3} × {:.3}",
            high[0] - low[0],
            high[1] - low[1],
            high[2] - low[2]
        ),
    })
}

fn cross(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

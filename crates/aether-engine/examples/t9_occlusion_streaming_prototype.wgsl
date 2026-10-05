struct Candidate { tile: u32, depth_milli: u32 };
struct Occluder { x_min: u32, y_min: u32, x_max: u32, y_max: u32, depth_milli: u32 };
@group(0) @binding(0) var<storage, read> occluders: array<Occluder>;
@group(0) @binding(1) var<storage, read_write> tile_depths: array<u32>;
@group(1) @binding(0) var<storage, read> candidates: array<Candidate>;
@group(1) @binding(1) var<storage, read> cull_depths: array<u32>;
@group(1) @binding(2) var<storage, read_write> visibility: array<u32>;

@compute @workgroup_size(64)
fn build_depth(@builtin(global_invocation_id) id: vec3<u32>) {
    let tile = id.x;
    if (tile >= 1296u) { return; }
    let x = tile % 48u;
    let y = tile / 48u;
    var nearest = 1001u;
    for (var i = 0u; i < 8u; i += 1u) {
        let rect = occluders[i];
        if (x >= rect.x_min && x < rect.x_max && y >= rect.y_min && y < rect.y_max) {
            nearest = min(nearest, rect.depth_milli);
        }
    }
    tile_depths[tile] = nearest;
}

@compute @workgroup_size(64)
fn classify(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= 10000u) { return; }
    let candidate = candidates[index];
    visibility[index] = select(0u, 1u, candidate.depth_milli <= cull_depths[candidate.tile]);
}

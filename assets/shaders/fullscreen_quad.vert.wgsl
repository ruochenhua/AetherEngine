// Fullscreen quad vertex shader
// Used for post-processing passes

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Generate a fullscreen triangle from vertex index
    // Vertex 0: (-1, -1), Vertex 1: (3, -1), Vertex 2: (-1, 3)
    var pos = vec2<f32>(
        f32(vertex_index % 2u) * 4.0 - 1.0,  // x: 0->-1, 1->3
        f32(vertex_index / 2u) * 4.0 - 1.0   // y: 0->-1, 1->3
    );

    var out: VertexOutput;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = pos * 0.5 + 0.5;
    return out;
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// The cell board's size (xy) and the window's size (zw), in pixels. The board
// is anchored at the top-left and kept at exactly `scale` px per cell, so
// cursor (px) / scale always maps to the right cell.
@group(0) @binding(2)
var<uniform> rect: vec4<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {
    let vertices = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = vertices[vertex_index];
    var out: VsOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    // NDC -1..1 -> window pixel coords with the origin at the top-left
    // (NDC `y` points up, window `y` points down), then to the cell board's
    // uv space (its top-left is uv (0, 0)).
    let ndc = p * 0.5 + 0.5;
    let px = vec2<f32>(ndc.x, 1.0 - ndc.y) * rect.zw;
    out.uv = px / rect.xy;
    return out;
}

@group(0) @binding(0)
var board: texture_2d<f32>;

@group(0) @binding(1)
var board_sampler: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(board, board_sampler, in.uv);
}

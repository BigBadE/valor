// Border rendering shader with support for border-radius
// Spec: CSS Backgrounds and Borders Module Level 3
// https://www.w3.org/TR/css-backgrounds-3/#borders

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) frag_pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_width: f32,
    @location(3) border_radius: f32,
    @location(4) rect_size: vec2<f32>,
};

struct BorderUniforms {
    transform: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: BorderUniforms;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_width: f32,
    @location(3) border_radius: f32,
    @location(4) rect_size: vec2<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(position, 0.0, 1.0);
    out.frag_pos = position;
    out.color = color;
    out.border_width = border_width;
    out.border_radius = border_radius;
    out.rect_size = rect_size;
    return out;
}

// Signed distance function for rounded rectangle
fn sd_rounded_box(p: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let half_size = size * 0.5;
    let q = abs(p) - half_size + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let half_size = in.rect_size * 0.5;
    let center_pos = in.frag_pos - half_size;

    // Distance to outer edge
    let outer_dist = sd_rounded_box(center_pos, in.rect_size, in.border_radius);

    // Distance to inner edge
    let inner_size = in.rect_size - vec2<f32>(in.border_width * 2.0);
    let inner_radius = max(in.border_radius - in.border_width, 0.0);
    let inner_dist = sd_rounded_box(center_pos, inner_size, inner_radius);

    // Border is the ring between outer and inner edges
    let border_mask = step(outer_dist, 0.0) * step(0.0, inner_dist);

    // Anti-aliasing
    let edge_softness = 1.0;
    let outer_alpha = 1.0 - smoothstep(-edge_softness, edge_softness, outer_dist);
    let inner_alpha = smoothstep(-edge_softness, edge_softness, inner_dist);
    let alpha = outer_alpha * inner_alpha;

    // Premultiply alpha
    let c = in.color;
    return vec4<f32>(c.xyz * c.w * alpha, c.w * alpha);
}

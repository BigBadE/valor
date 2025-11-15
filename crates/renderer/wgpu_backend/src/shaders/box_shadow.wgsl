// Box shadow rendering shader with Gaussian blur approximation
// Spec: CSS Backgrounds and Borders Module Level 3
// https://www.w3.org/TR/css-backgrounds-3/#box-shadow

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) frag_pos: vec2<f32>,
    @location(1) shadow_center: vec2<f32>,
    @location(2) shadow_size: vec2<f32>,
    @location(3) blur_radius: f32,
    @location(4) spread_radius: f32,
    @location(5) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) shadow_center: vec2<f32>,
    @location(2) shadow_size: vec2<f32>,
    @location(3) blur_radius: f32,
    @location(4) spread_radius: f32,
    @location(5) color: vec4<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(position, 0.0, 1.0);
    out.frag_pos = position;
    out.shadow_center = shadow_center;
    out.shadow_size = shadow_size;
    out.blur_radius = blur_radius;
    out.spread_radius = spread_radius;
    out.color = color;
    return out;
}

// Gaussian blur approximation using distance falloff
fn gaussian_falloff(distance: f32, sigma: f32) -> f32 {
    let variance = sigma * sigma;
    return exp(-(distance * distance) / (2.0 * variance));
}

// Signed distance to a rounded rectangle
fn sd_rounded_box(p: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let half_size = size * 0.5;
    let q = abs(p) - half_size + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Calculate position relative to shadow center
    let rel_pos = in.frag_pos - in.shadow_center;

    // Apply spread radius (makes shadow bigger)
    let shadow_size_with_spread = in.shadow_size + vec2<f32>(in.spread_radius * 2.0);

    // Calculate distance to shadow box (with spread)
    let dist = sd_rounded_box(rel_pos, shadow_size_with_spread, 0.0);

    // Blur using Gaussian approximation
    // sigma = blur_radius / 2 for a reasonable blur
    let sigma = max(in.blur_radius / 2.0, 0.5);
    let blur_factor = gaussian_falloff(abs(dist), sigma);

    // Shadow is only visible outside the box
    let shadow_alpha = select(0.0, blur_factor, dist > 0.0);

    // Premultiply alpha
    let c = in.color;
    let final_alpha = c.w * shadow_alpha;
    return vec4<f32>(c.xyz * final_alpha, final_alpha);
}

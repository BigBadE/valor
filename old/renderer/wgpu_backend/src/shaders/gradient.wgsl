// Gradient rendering shaders (linear and radial)
// Spec: CSS Images Module Level 3
// https://www.w3.org/TR/css-images-3/#gradients

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) gradient_type: u32, // 0 = linear, 1 = radial
    @location(2) angle: f32, // For linear gradients (in radians)
};

struct GradientStop {
    offset: f32,    // 0.0 to 1.0
    color: vec4<f32>, // RGBA
};

// Maximum gradient stops supported
const MAX_STOPS: u32 = 16u;

struct GradientUniforms {
    stops: array<GradientStop, MAX_STOPS>,
    stop_count: u32,
    _padding: vec3<f32>,
};

@group(0) @binding(0) var<uniform> gradient: GradientUniforms;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) gradient_type: u32,
    @location(3) angle: f32,
) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    out.gradient_type = gradient_type;
    out.angle = angle;
    return out;
}

// Linear interpolation between two colors
fn lerp_color(color1: vec4<f32>, color2: vec4<f32>, t: f32) -> vec4<f32> {
    return color1 * (1.0 - t) + color2 * t;
}

// Sample gradient at position t (0.0 to 1.0)
fn sample_gradient(t: f32) -> vec4<f32> {
    let clamped_t = clamp(t, 0.0, 1.0);

    // Find the two stops to interpolate between
    var color = gradient.stops[0].color;

    for (var i = 0u; i < gradient.stop_count - 1u; i++) {
        let stop1 = gradient.stops[i];
        let stop2 = gradient.stops[i + 1u];

        if (clamped_t >= stop1.offset && clamped_t <= stop2.offset) {
            let local_t = (clamped_t - stop1.offset) / (stop2.offset - stop1.offset);
            color = lerp_color(stop1.color, stop2.color, local_t);
            break;
        }
    }

    // Handle case where t is beyond last stop
    if (clamped_t >= gradient.stops[gradient.stop_count - 1u].offset) {
        color = gradient.stops[gradient.stop_count - 1u].color;
    }

    return color;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    var t: f32;

    if (in.gradient_type == 0u) {
        // Linear gradient
        // Convert angle to direction vector
        let dir = vec2<f32>(cos(in.angle), sin(in.angle));

        // Project UV onto direction vector
        // Center UV at (0.5, 0.5) for proper rotation
        let centered_uv = in.uv - vec2<f32>(0.5, 0.5);
        t = dot(centered_uv, dir) + 0.5;
    } else {
        // Radial gradient
        // Distance from center
        let centered_uv = in.uv - vec2<f32>(0.5, 0.5);
        t = length(centered_uv) * 2.0; // Scale to fill the box
    }

    let color = sample_gradient(t);

    // Premultiply alpha
    return vec4<f32>(color.xyz * color.w, color.w);
}

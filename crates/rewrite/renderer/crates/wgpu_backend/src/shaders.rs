//! WGSL shaders for rendering primitives.

pub const RECT_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    // Convert from pixel coordinates to clip space (-1 to 1)
    // Assuming a 800x600 viewport for now - should be a uniform
    output.position = vec4<f32>(
        (input.position.x / 400.0) - 1.0,
        1.0 - (input.position.y / 300.0),
        0.0,
        1.0
    );
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

pub const TEXT_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@group(0) @binding(0)
var glyph_texture: texture_2d<f32>;
@group(0) @binding(1)
var glyph_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        (input.position.x / 400.0) - 1.0,
        1.0 - (input.position.y / 300.0),
        0.0,
        1.0
    );
    output.tex_coords = input.tex_coords;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(glyph_texture, glyph_sampler, input.tex_coords).r;
    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}
"#;

pub const ROUNDED_RECT_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) center: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) radius: vec4<f32>, // top-left, top-right, bottom-right, bottom-left
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) center: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) radius: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        (input.position.x / 400.0) - 1.0,
        1.0 - (input.position.y / 300.0),
        0.0,
        1.0
    );
    output.color = input.color;
    output.local_pos = input.position - input.center;
    output.center = input.center;
    output.size = input.size;
    output.radius = input.radius;
    return output;
}

fn sdf_rounded_rect(pos: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    // Select the appropriate corner radius
    var r = radius.x; // top-left by default
    if pos.x > 0.0 && pos.y < 0.0 {
        r = radius.y; // top-right
    } else if pos.x > 0.0 && pos.y > 0.0 {
        r = radius.z; // bottom-right
    } else if pos.x < 0.0 && pos.y > 0.0 {
        r = radius.w; // bottom-left
    }

    let half_size = size * 0.5;
    let d = abs(pos) - half_size + vec2<f32>(r, r);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0, 0.0))) - r;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let dist = sdf_rounded_rect(input.local_pos, input.size, input.radius);
    let alpha = 1.0 - smoothstep(0.0, 1.0, dist);
    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}
"#;

pub const GRADIENT_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
}

struct GradientUniforms {
    gradient_type: u32, // 0 = linear, 1 = radial, 2 = conic
    angle: f32,
    center: vec2<f32>,
    // Color stops packed into a texture or storage buffer
}

@group(0) @binding(0)
var<uniform> gradient: GradientUniforms;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(
        (input.position.x / 400.0) - 1.0,
        1.0 - (input.position.y / 300.0),
        0.0,
        1.0
    );
    output.local_pos = input.position;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Gradient interpolation would go here
    // For now, return a placeholder color
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
"#;

pub const BLUR_SHADER: &str = r#"
@group(0) @binding(0)
var input_texture: texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var output: VertexOutput;
    let x = f32((vertex_index & 1u) << 1u);
    let y = f32(vertex_index & 2u);
    output.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    output.tex_coords = vec2<f32>(x, y);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Simple box blur - should be a proper Gaussian blur
    let texel_size = 1.0 / vec2<f32>(textureDimensions(input_texture));
    var color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    for (var x = -2; x <= 2; x = x + 1) {
        for (var y = -2; y <= 2; y = y + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
            color = color + textureSample(input_texture, texture_sampler, input.tex_coords + offset);
        }
    }

    return color / 25.0;
}
"#;

struct Parameters {
    bounds: vec4<f32>,
    first: vec4<f32>,
    second: vec4<f32>,
    data: vec4<u32>,
}

@group(1) @binding(0) var<storage, read> checker_parameters: array<Parameters>;
@group(1) @binding(1) var<uniform> parameters: Parameters;
@group(1) @binding(2) var source_texture: texture_2d<f32>;
@group(1) @binding(3) var original_texture: texture_2d<f32>;

struct CheckerOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) index: u32,
}

@vertex
fn checker_quad(@builtin(vertex_index) index: u32, @builtin(instance_index) instance: u32) -> CheckerOutput {
    let parameters = checker_parameters[instance];
    let corner = vec2(f32(index & 1u), f32(index >> 1u));
    let pixel = parameters.bounds.xy + corner * parameters.bounds.zw;
    var output: CheckerOutput;
    output.position = vec4(pixel * frame.transform.xy + frame.transform.zw, 0.0, 1.0);
    output.index = instance;
    return output;
}

@vertex
fn quad(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2(f32(index & 1u), f32(index >> 1u));
    let pixel = parameters.bounds.xy + corner * parameters.bounds.zw;
    return vec4(pixel * frame.transform.xy + frame.transform.zw, 0.0, 1.0);
}

@vertex
fn blur_quad(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2(f32(index & 1u), f32(index >> 1u));
    let pixel = parameters.bounds.xy + corner * parameters.bounds.zw;
    let size = vec2<f32>(textureDimensions(source_texture));
    return vec4(pixel * vec2(2.0, -2.0) / size + vec2(-1.0, 1.0), 0.0, 1.0);
}

@fragment
fn checker(input: CheckerOutput) -> @location(0) vec4<f32> {
    let parameters = checker_parameters[input.index];
    let tile = vec2<u32>(floor(input.position.xy / f32(parameters.data.y)));
    let color = select(parameters.first, parameters.second, ((tile.x + tile.y) & 1u) != 0u);
    return color * clip_coverage(input.position.xy, parameters.data.x);
}

@fragment
fn horizontal(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let radius = i32(parameters.data.z);
    let size = vec2<i32>(textureDimensions(source_texture));
    var color = vec4(0.0);
    for (var offset = -radius; offset <= radius; offset += 1) {
        let pixel = clamp(vec2<i32>(position.xy) + vec2(offset, 0), vec2(0), size - vec2(1));
        color += textureLoad(source_texture, pixel, 0);
    }
    return color / f32(radius * 2 + 1);
}

@fragment
fn vertical(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let radius = i32(parameters.data.z);
    let size = vec2<i32>(textureDimensions(source_texture));
    var color = vec4(0.0);
    for (var offset = -radius; offset <= radius; offset += 1) {
        let pixel = clamp(vec2<i32>(position.xy) + vec2(0, offset), vec2(0), size - vec2(1));
        color += textureLoad(source_texture, pixel, 0);
    }
    let original = textureLoad(original_texture, vec2<i32>(position.xy), 0);
    return mix(original, color / f32(radius * 2 + 1), clip_coverage(position.xy, parameters.data.x));
}

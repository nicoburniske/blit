struct Frame {
    transform: vec4<f32>,
}

struct Instance {
    shape: vec4<f32>,
    draw: vec4<f32>,
    radii: vec4<f32>,
    inner_color: vec4<f32>,
    border_color: vec4<f32>,
    params: vec4<f32>,
    data: vec4<u32>,
}

struct Clip {
    rect: vec4<f32>,
    radii: vec4<f32>,
    data: vec4<u32>,
}

struct GradientStop {
    color: vec4<f32>,
    data: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) instance: u32,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var<storage, read> clips: array<Clip>;
@group(0) @binding(3) var<storage, read> stops: array<GradientStop>;
@group(1) @binding(0) var image_texture: texture_2d<f32>;

const OUTSET_SHADOW: u32 = 1u;
const INSET_SHADOW: u32 = 2u;

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let corners = array<vec2<f32>, 4>(
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
    );
    let draw = instances[instance_index].draw;
    let pixel = draw.xy + draw.zw * corners[vertex_index];
    var output: VertexOutput;
    output.position = vec4(pixel * frame.transform.xy + frame.transform.zw, 0.0, 1.0);
    output.instance = instance_index;
    return output;
}

fn corner_distance(point: vec2<f32>, radius: f32) -> f32 {
    let outside = max(point, vec2(0.0));
    return length(outside) + min(max(point.x, point.y), 0.0) - radius;
}

fn rounded_distance(position: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let center = rect.xy + rect.zw * 0.5;
    let point = position - center;
    let half_size = rect.zw * 0.5;
    if all(radii <= vec4(min(half_size.x, half_size.y))) {
        var radius = radii.x;
        if point.y < 0.0 {
            if point.x >= 0.0 {
                radius = radii.y;
            }
        } else if point.x >= 0.0 {
            radius = radii.z;
        } else {
            radius = radii.w;
        }
        let distance = abs(point) - half_size + vec2(radius);
        return length(max(distance, vec2(0.0)))
            + min(max(distance.x, distance.y), 0.0)
            - radius;
    }

    let end = rect.xy + rect.zw;
    let top_left = rect.xy + vec2(radii.x);
    let top_right = vec2(end.x - radii.y, rect.y + radii.y);
    let bottom_right = end - vec2(radii.z);
    let bottom_left = vec2(rect.x + radii.w, end.y - radii.w);
    let top_left_distance = corner_distance(top_left - position, radii.x);
    let top_right_distance = corner_distance(
        vec2(position.x - top_right.x, top_right.y - position.y),
        radii.y,
    );
    let bottom_right_distance = corner_distance(position - bottom_right, radii.z);
    let bottom_left_distance = corner_distance(
        vec2(bottom_left.x - position.x, position.y - bottom_left.y),
        radii.w,
    );
    return max(
        max(top_left_distance, top_right_distance),
        max(bottom_right_distance, bottom_left_distance),
    );
}

fn rounded_coverage(position: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    if any(rect.zw <= vec2(0.0)) {
        return 0.0;
    }
    return clamp(0.5 - rounded_distance(position, rect, radii), 0.0, 1.0);
}

fn blurred_coverage(
    position: vec2<f32>,
    rect: vec4<f32>,
    radii: vec4<f32>,
    blur: f32,
) -> f32 {
    if blur <= 0.0 {
        return rounded_coverage(position, rect, radii);
    }
    return 1.0 - smoothstep(-blur, blur, rounded_distance(position, rect, radii));
}

fn clip_coverage(position: vec2<f32>, first_clip: u32) -> f32 {
    var coverage = 1.0;
    var clip_id = first_clip;
    while clip_id != 0u {
        let clip = clips[clip_id];
        coverage *= rounded_coverage(position, clip.rect, clip.radii);
        clip_id = clip.data.x;
    }
    return coverage;
}

fn image_source(value: f32, size: f32, wrap: bool) -> f32 {
    if wrap {
        return value - floor(value / size) * size;
    }
    return clamp(value, 0.0, size - 1.0);
}

fn image_texel(instance: Instance, source: vec2<i32>) -> vec4<f32> {
    let coordinate = bitcast<vec2<i32>>(instance.radii.xy) + source;
    let texel = textureLoad(image_texture, coordinate, 0);
    if (instance.data.y & 8u) != 0u {
        return instance.border_color * texel.r;
    }
    return texel;
}

fn multiply_modulo(left: u32, right: u32, modulo: u32) -> u32 {
    if (left | right) <= 0xffffu {
        return left * right % modulo;
    }

    var product = 0u;
    var value = max(left, right) % modulo;
    var multiplier = min(left, right);
    while multiplier != 0u {
        if (multiplier & 1u) != 0u {
            product = (product + value) % modulo;
        }
        value = (value + value) % modulo;
        multiplier >>= 1u;
    }
    return product;
}

fn nearest_source(
    pixel: u32,
    step: vec2<u32>,
    phase: vec2<u32>,
    size: u32,
    wrap: bool,
) -> u32 {
    let mask = 0xffffu;
    let integer_step = (step.x >> 16u) | (step.y << 16u);
    let fraction = (pixel >> 16u) * (step.x & mask)
        + (((pixel & mask) * (step.x & mask) + (phase.x & mask)) >> 16u);
    let origin = (phase.x >> 16u) | (phase.y << 16u);
    if !wrap {
        return min(origin + pixel * integer_step + fraction, size - 1u);
    }

    var source = origin % size;
    source = (source + multiply_modulo(pixel, integer_step, size)) % size;
    source = (source + fraction % size) % size;
    return source;
}

fn sample_image(instance: Instance, position: vec2<f32>) -> vec4<f32> {
    let wrap_x = (instance.data.y & 1u) != 0u;
    let wrap_y = (instance.data.y & 2u) != 0u;
    if (instance.data.y & 4u) == 0u {
        let pixel = vec2<u32>(floor(position) - instance.draw.xy);
        let phase = bitcast<vec2<u32>>(instance.params.xy);
        let high = bitcast<vec4<u32>>(instance.shape);
        let size = bitcast<vec2<u32>>(instance.radii.zw);
        let source = vec2<i32>(vec2(
            nearest_source(
                pixel.x,
                vec2(instance.data.z, high.x),
                vec2(phase.x, high.z),
                size.x,
                wrap_x,
            ),
            nearest_source(
                pixel.y,
                vec2(instance.data.w, high.y),
                vec2(phase.y, high.w),
                size.y,
                wrap_y,
            ),
        ));
        return image_texel(instance, source);
    }

    let size = instance.radii.zw;
    let phase = bitcast<vec2<f32>>(instance.data.zw);
    var source = phase + (floor(position) - instance.draw.xy) * instance.params.xy;
    source.x = image_source(source.x, size.x, wrap_x);
    source.y = image_source(source.y, size.y, wrap_y);
    let first = floor(source);
    var second = first + vec2(1.0);
    if second.x >= size.x {
        second.x = select(first.x, 0.0, wrap_x);
    }
    if second.y >= size.y {
        second.y = select(first.y, 0.0, wrap_y);
    }
    let amount = source - first;
    let top = mix(
        image_texel(instance, vec2<i32>(first)),
        image_texel(instance, vec2<i32>(vec2(second.x, first.y))),
        amount.x,
    );
    let bottom = mix(
        image_texel(instance, vec2<i32>(vec2(first.x, second.y))),
        image_texel(instance, vec2<i32>(second)),
        amount.x,
    );
    return mix(top, bottom, amount.y);
}

@fragment
fn clear(input: VertexOutput) -> @location(0) vec4<f32> {
    return instances[input.instance].inner_color;
}

@fragment
fn shape(input: VertexOutput) -> @location(0) vec4<f32> {
    let instance = instances[input.instance];
    let position = input.position.xy;
    if instance.data.w == OUTSET_SHADOW || instance.data.w == INSET_SHADOW {
        var coverage = 0.0;
        if instance.data.w == OUTSET_SHADOW {
            coverage = blurred_coverage(
                position,
                instance.shape,
                instance.radii,
                instance.params.x,
            );
        } else {
            let spread = instance.params.w;
            let hole_size = instance.shape.zw - vec2(spread * 2.0);
            var hole_coverage = 0.0;
            if all(hole_size > vec2(0.0)) {
                let hole = vec4(
                    instance.shape.xy + instance.params.yz + vec2(spread),
                    hole_size,
                );
                hole_coverage = blurred_coverage(
                    position,
                    hole,
                    max(instance.radii - vec4(spread), vec4(0.0)),
                    instance.params.x,
                );
            }
            coverage = rounded_coverage(position, instance.shape, instance.radii)
                * (1.0 - hole_coverage);
        }
        coverage *= clip_coverage(position, instance.data.x);
        if coverage <= 0.0 {
            discard;
        }
        return instance.inner_color * coverage;
    }

    var coverage = clip_coverage(position, instance.data.x);
    if any(instance.radii != vec4(0.0)) {
        coverage *= rounded_coverage(position, instance.shape, instance.radii);
    }
    if coverage <= 0.0 {
        discard;
    }

    var color = instance.inner_color;
    let border_width = instance.params.x;
    if border_width > 0.0 {
        var border = instance.border_color;
        if instance.data.z >= 2u {
            let pixel = position - vec2(0.5);
            let gradient_position = instance.params.w
                + instance.params.y * (pixel.x - instance.shape.x)
                + instance.params.z * (pixel.y - instance.shape.y);
            var index = 0u;
            while index + 1u < instance.data.z - 1u
                && gradient_position >= stops[instance.data.y + index + 1u].data.x
            {
                index += 1u;
            }
            let first = stops[instance.data.y + index];
            let second = stops[instance.data.y + index + 1u];
            let amount = clamp(
                (gradient_position - first.data.x) / (second.data.x - first.data.x),
                0.0,
                1.0,
            );
            border = mix(first.color, second.color, amount);
        }

        let inner_size = instance.shape.zw - vec2(border_width * 2.0);
        var inner_coverage = 0.0;
        if inner_size.x > 0.0 && inner_size.y > 0.0 {
            let inner_rect = vec4(
                instance.shape.xy + vec2(border_width),
                inner_size,
            );
            inner_coverage = rounded_coverage(
                position,
                inner_rect,
                max(instance.radii - vec4(border_width), vec4(0.0)),
            );
        }
        color = mix(border, instance.inner_color, inner_coverage);
    }
    return color * coverage;
}

@fragment
fn image(input: VertexOutput) -> @location(0) vec4<f32> {
    let instance = instances[input.instance];
    let position = input.position.xy;
    let coverage = clip_coverage(position, instance.data.x);
    if coverage <= 0.0 {
        discard;
    }
    var color = sample_image(instance, position);
    if (instance.data.y & 16u) != 0u {
        color = instance.inner_color * color.a;
    }
    return color * instance.params.z * coverage;
}

@fragment
fn text(input: VertexOutput) -> @location(0) vec4<f32> {
    let instance = instances[input.instance];
    let position = input.position.xy;
    let coordinate =
        vec2<i32>(instance.data.yz)
        + vec2<i32>(floor(position) - instance.shape.xy);
    let coverage = textureLoad(image_texture, coordinate, 0).r
        * clip_coverage(position, instance.data.x);
    if coverage <= 0.0 {
        discard;
    }
    return instance.inner_color * coverage;
}

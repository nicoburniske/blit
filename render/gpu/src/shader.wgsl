// one instanced quad per command, drawn in command order into a premultiplied target

const KIND_RECTANGLE: u32 = 0u;
const KIND_GLYPH: u32 = 1u;
const KIND_SHADOW: u32 = 2u;
const KIND_INSET_SHADOW: u32 = 3u;
const KIND_IMAGE: u32 = 4u;
const KIND_GRADIENT: u32 = 5u;

const SHADOW_SAMPLES: i32 = 9;

struct Instance {
    // clipped area of the quad, in physical pixels
    quad: vec4<f32>,
    // geometry the shape is derived from, in physical pixels
    shape: vec4<f32>,
    // top left, top right, bottom right, bottom left
    radii: vec4<f32>,
    color: vec4<f32>,
    border_color: vec4<f32>,
    // rounded clip shape, width is zero without one
    clip_shape: vec4<f32>,
    clip_radii: vec4<f32>,
    // atlas texels, or the gradient stop range
    texture: vec4<f32>,
    // kind, border width, blur sigma, opacity
    params: vec4<f32>,
    // shadow offset and spread, or the gradient direction
    extra: vec4<f32>,
}

struct Stop {
    color: vec4<f32>,
    position: vec4<f32>,
}

struct Frame {
    viewport: vec2<f32>,
    // glyph atlas size, image atlas size
    atlas: vec2<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var<storage, read> stops: array<Stop>;
@group(0) @binding(3) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(4) var image_atlas: texture_2d<f32>;
@group(0) @binding(5) var atlas_sampler: sampler;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) point: vec2<f32>,
    @location(1) @interpolate(flat) index: u32,
}

@vertex
fn vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) index: u32) -> Vertex {
    let quad = instances[index].quad;
    let corner = vec2<f32>(f32(vertex & 1u), f32((vertex >> 1u) & 1u));
    let point = quad.xy + corner * quad.zw;
    var out: Vertex;
    out.position = vec4<f32>(
        point / frame.viewport * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0),
        0.0,
        1.0,
    );
    out.point = point;
    out.index = index;
    return out;
}

fn premultiply(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

fn rounded_distance(point: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let side = select(radii.wz, radii.xy, point.y < 0.0);
    let radius = select(side.y, side.x, point.x < 0.0);
    let corner = abs(point) - half_size + vec2<f32>(radius);
    return min(max(corner.x, corner.y), 0.0) + length(max(corner, vec2<f32>(0.0))) - radius;
}

fn rounded_coverage(point: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let half_size = max(rect.zw, vec2<f32>(0.0)) * 0.5;
    let distance = rounded_distance(point - rect.xy - half_size, half_size, radii);
    return clamp(0.5 - distance, 0.0, 1.0);
}

// Abramowitz and Stegun 7.1.26
fn error_function(x: vec2<f32>) -> vec2<f32> {
    let magnitude = abs(x);
    var series = 1.0 + (0.278393 + (0.230389 + 0.078108 * magnitude * magnitude) * magnitude)
        * magnitude;
    series = series * series;
    return sign(x) - sign(x) / (series * series);
}

/// fraction of a horizontal span covered after a gaussian blur
fn blurred_span(start: f32, end: f32, point: f32, sigma: f32) -> f32 {
    let integral = 0.5 + 0.5 * error_function(
        vec2<f32>(point - start, point - end) * (inverseSqrt(2.0) / sigma),
    );
    return integral.x - integral.y;
}

fn corner_inset(distance: f32, radius: f32) -> f32 {
    if radius <= 0.0 || distance >= radius {
        return 0.0;
    }
    let offset = radius - distance;
    return radius - sqrt(max(radius * radius - offset * offset, 0.0));
}

fn gaussian(offset: f32, sigma: f32) -> f32 {
    return exp(-0.5 * offset * offset / (sigma * sigma)) * inverseSqrt(6.283185 * sigma * sigma);
}

/// vertical integration of the horizontal blur, sampled over the visible rows
fn blurred_rounded(point: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>, sigma: f32) -> f32 {
    if rect.z <= 0.0 || rect.w <= 0.0 {
        return 0.0;
    }
    if sigma <= 0.0 {
        return rounded_coverage(point, rect, radii);
    }
    let start = max(rect.y, point.y - 3.0 * sigma);
    let end = min(rect.y + rect.w, point.y + 3.0 * sigma);
    let step = (end - start) / f32(SHADOW_SAMPLES);
    if step <= 0.0 {
        return 0.0;
    }
    var total = 0.0;
    for (var sample = 0; sample < SHADOW_SAMPLES; sample++) {
        let y = start + step * (f32(sample) + 0.5);
        let top = y - rect.y;
        let bottom = rect.y + rect.w - y;
        let left = max(corner_inset(top, radii.x), corner_inset(bottom, radii.w));
        let right = max(corner_inset(top, radii.y), corner_inset(bottom, radii.z));
        total += gaussian(y - point.y, sigma)
            * blurred_span(rect.x + left, rect.x + rect.z - right, point.x, sigma)
            * step;
    }
    return clamp(total, 0.0, 1.0);
}

fn atlas_texel(instance: Instance, point: vec2<f32>, nearest: bool) -> vec2<f32> {
    let offset = (point - instance.shape.xy) / max(instance.shape.zw, vec2<f32>(1.0));
    let texel = instance.texture.xy + offset * instance.texture.zw;
    if nearest {
        return floor(texel) + vec2<f32>(0.5);
    }
    return texel;
}

fn gradient_color(instance: Instance, point: vec2<f32>) -> vec4<f32> {
    let direction = instance.extra.xy;
    let size = instance.shape.zw;
    let extent = abs(direction.x) * size.x + abs(direction.y) * size.y;
    let minimum = min(direction.x, 0.0) * size.x + min(direction.y, 0.0) * size.y;
    let position = (dot(direction, point - instance.shape.xy) - minimum) / extent;
    let start = u32(instance.texture.x);
    let count = u32(instance.texture.y);
    var index = 0u;
    for (var next = 0u; next + 2u < count; next++) {
        if position < stops[start + next + 1u].position.x {
            break;
        }
        index = next + 1u;
    }
    let first = stops[start + index];
    let second = stops[start + index + 1u];
    let span = max(second.position.x - first.position.x, 1e-6);
    let amount = clamp((position - first.position.x) / span, 0.0, 1.0);
    return mix(premultiply(first.color), premultiply(second.color), amount);
}

@fragment
fn fragment(input: Vertex) -> @location(0) vec4<f32> {
    let instance = instances[input.index];
    let point = input.point;
    let kind = u32(instance.params.x);
    var color = vec4<f32>(0.0);
    if kind == KIND_RECTANGLE || kind == KIND_GRADIENT {
        let outer = rounded_coverage(point, instance.shape, instance.radii);
        color = premultiply(instance.color) * outer;
        let width = instance.params.y;
        if width > 0.0 {
            let inner = vec4<f32>(
                instance.shape.xy + width,
                max(instance.shape.zw - width * 2.0, vec2<f32>(0.0)),
            );
            let ring = outer - rounded_coverage(
                point,
                inner,
                max(instance.radii - vec4<f32>(width), vec4<f32>(0.0)),
            );
            var edge = premultiply(instance.border_color);
            if kind == KIND_GRADIENT {
                edge = gradient_color(instance, point);
            }
            color = edge * ring + color * (1.0 - edge.a * ring);
        }
        color *= instance.params.w;
    } else if kind == KIND_GLYPH {
        let texel = atlas_texel(instance, point, true);
        let coverage = textureSampleLevel(
            glyph_atlas,
            atlas_sampler,
            texel / frame.atlas.x,
            0.0,
        ).r;
        color = premultiply(instance.color) * coverage;
    } else if kind == KIND_IMAGE {
        let texel = atlas_texel(instance, point, instance.params.z < 0.5);
        let sampled = textureSampleLevel(
            image_atlas,
            atlas_sampler,
            texel / frame.atlas.y,
            0.0,
        );
        if instance.extra.x > 0.5 {
            color = premultiply(instance.color) * sampled.a;
        } else {
            color = sampled;
        }
        color *= instance.params.w;
    } else if kind == KIND_SHADOW {
        let coverage = blurred_rounded(point, instance.shape, instance.radii, instance.params.z);
        color = premultiply(instance.color) * coverage;
    } else if kind == KIND_INSET_SHADOW {
        let spread = instance.extra.z;
        let hole = vec4<f32>(
            instance.shape.xy + instance.extra.xy + spread,
            max(instance.shape.zw - spread * 2.0, vec2<f32>(0.0)),
        );
        let inside = blurred_rounded(
            point,
            hole,
            max(instance.radii - vec4<f32>(spread), vec4<f32>(0.0)),
            instance.params.z,
        );
        let shape = rounded_coverage(point, instance.shape, instance.radii);
        color = premultiply(instance.color) * (1.0 - inside) * shape;
    }
    if instance.clip_shape.z > 0.0 {
        color *= rounded_coverage(point, instance.clip_shape, instance.clip_radii);
    }
    return color;
}

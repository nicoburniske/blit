struct Frame {
    transform: vec4<f32>,
}

struct Clip {
    rect: vec4<f32>,
    radii: vec4<f32>,
    data: vec4<u32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(2) var<storage, read> clips: array<Clip>;

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

fn clip_coverage(position: vec2<f32>, first_clip: u32) -> f32 {
    var coverage = 1.0;
    var clip_id = first_clip & 0x7fffffffu;
    while clip_id != 0u {
        let clip = clips[clip_id];
        coverage *= rounded_coverage(position, clip.rect, clip.radii);
        clip_id = clip.data.x;
    }
    return coverage;
}


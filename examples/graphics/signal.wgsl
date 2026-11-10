struct SignalParameters {
    bounds: vec4<f32>,
    phase: f32,
    clip: u32,
    padding: vec2<u32>,
}

@group(1) @binding(0) var<uniform> signal: SignalParameters;

@vertex
fn signal_quad(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2(f32(index & 1u), f32(index >> 1u));
    let pixel = signal.bounds.xy + corner * signal.bounds.zw;
    return vec4(pixel * frame.transform.xy + frame.transform.zw, 0.0, 1.0);
}

@fragment
fn signal_fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (position.xy - signal.bounds.xy) / signal.bounds.zw;
    let aspect = signal.bounds.z / signal.bounds.w;
    let p = (uv - vec2(0.56, 0.54)) * vec2(aspect, 1.0) * 3.0;
    let time = signal.phase * 6.2831853;
    let a = p + vec2(sin(time), cos(time)) * 0.38;
    let b = p - vec2(cos(time), sin(time)) * 0.5;
    let field = length(a) * 4.0 - length(b) * 2.8
        + sin(p.x * 2.0 + time) * cos(p.y * 2.5 - time) * 0.7;
    let contour = abs(fract(field * 3.0) - 0.5);
    let line = 1.0 - smoothstep(0.025, 0.025 + fwidth(field) * 4.5, contour);
    let light = exp(-abs(field - 1.4) * 1.5);
    let graphite = vec3(0.065, 0.08, 0.09);
    let silver = vec3(0.64, 0.69, 0.67);
    let amber = vec3(0.88, 0.58, 0.25);
    let ink = mix(silver, amber, smoothstep(0.1, 1.1, field));
    var color = graphite + ink * (line * 0.7 + light * 0.13);
    let scanline = 0.97 + 0.03 * sin(position.y * 3.1415927);
    color *= scanline;
    let fade = smoothstep(0.0, 0.32, uv.y) * (1.0 - smoothstep(0.82, 1.15, uv.y));
    color = mix(graphite, color, fade);
    let coverage = clip_coverage(position.xy, signal.clip);
    return vec4(color * coverage, coverage);
}

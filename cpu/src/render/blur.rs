use std::simd::{
    Simd, StdFloat,
    num::{SimdFloat, SimdInt, SimdUint},
};

type I32x8 = Simd<i32, 8>;
type U8x8 = Simd<u8, 8>;

pub fn stack(alpha: &mut [u8], width: usize, height: usize, radius: u32) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }

    let radius = radius as usize;
    let div = radius * 2 + 1;
    let scale = 1.0 / ((radius + 1) * (radius + 1)) as f32;
    let mut stack = vec![I32x8::splat(0); div];

    let mut y = 0;
    while y + 8 <= height {
        simd_pass(
            alpha,
            width,
            radius,
            scale,
            &mut stack,
            |alpha, x| {
                I32x8::from_array(std::array::from_fn(|lane| {
                    alpha[(y + lane) * width + x] as i32
                }))
            },
            |alpha, x, output| {
                for (lane, output) in output.to_array().into_iter().enumerate() {
                    alpha[(y + lane) * width + x] = output;
                }
            },
        );
        y += 8;
    }
    for row in y..height {
        line(alpha, row * width, 1, width, radius, scale, &mut stack);
    }

    let mut x = 0;
    while x + 8 <= width {
        simd_pass(
            alpha,
            height,
            radius,
            scale,
            &mut stack,
            |alpha, y| {
                let start = y * width + x;
                U8x8::from_slice(&alpha[start..start + 8]).cast::<i32>()
            },
            |alpha, y, output| {
                let start = y * width + x;
                output.copy_to_slice(&mut alpha[start..start + 8]);
            },
        );
        x += 8;
    }
    for column in x..width {
        line(alpha, column, width, height, radius, scale, &mut stack);
    }
}

fn simd_pass(
    alpha: &mut [u8],
    length: usize,
    radius: usize,
    scale: f32,
    stack: &mut [I32x8],
    mut load: impl FnMut(&[u8], usize) -> I32x8,
    mut store: impl FnMut(&mut [u8], usize, U8x8),
) {
    let div = radius * 2 + 1;
    let first = load(alpha, 0);
    let mut sum = I32x8::splat(0);
    let mut sum_in = I32x8::splat(0);
    let mut sum_out = I32x8::splat(0);

    for (i, slot) in stack.iter_mut().take(radius + 1).enumerate() {
        *slot = first;
        sum += first * I32x8::splat(i as i32 + 1);
        sum_out += first;
    }
    for i in 1..=radius {
        let source = load(alpha, i.min(length - 1));
        stack[i + radius] = source;
        sum += source * I32x8::splat((radius + 1 - i) as i32);
        sum_in += source;
    }

    let mut source = radius.min(length - 1);
    let mut position = radius;
    for output in 0..length {
        store(
            alpha,
            output,
            (sum.cast::<f32>() * Simd::splat(scale))
                .round()
                .cast::<i32>()
                .cast::<u8>(),
        );

        sum -= sum_out;
        let mut leaving = position + div - radius;
        if leaving >= div {
            leaving -= div;
        }
        sum_out -= stack[leaving];
        source = (source + 1).min(length - 1);
        let input = load(alpha, source);
        stack[leaving] = input;
        sum_in += input;
        sum += sum_in;
        position += 1;
        if position >= div {
            position = 0;
        }
        sum_out += stack[position];
        sum_in -= stack[position];
    }
}

fn line(
    alpha: &mut [u8],
    start: usize,
    step: usize,
    length: usize,
    radius: usize,
    scale: f32,
    stack: &mut [I32x8],
) {
    let div = radius * 2 + 1;
    let first = alpha[start] as i32;
    let mut sum = 0;
    let mut sum_in = 0;
    let mut sum_out = 0;

    for (i, slot) in stack.iter_mut().take(radius + 1).enumerate() {
        slot[0] = first;
        sum += first * (i as i32 + 1);
        sum_out += first;
    }
    for i in 1..=radius {
        let source = alpha[start + i.min(length - 1) * step] as i32;
        stack[i + radius][0] = source;
        sum += source * (radius + 1 - i) as i32;
        sum_in += source;
    }

    let mut source = radius.min(length - 1);
    let mut position = radius;
    for output in 0..length {
        alpha[start + output * step] = (sum as f32 * scale).round() as u8;
        sum -= sum_out;
        let mut leaving = position + div - radius;
        if leaving >= div {
            leaving -= div;
        }
        sum_out -= stack[leaving][0];
        source = (source + 1).min(length - 1);
        stack[leaving][0] = alpha[start + source * step] as i32;
        sum_in += stack[leaving][0];
        sum += sum_in;
        position += 1;
        if position >= div {
            position = 0;
        }
        sum_out += stack[position][0];
        sum_in -= stack[position][0];
    }
}

#[test]
fn simd_matches_scalar() {
    let (width, height, radius) = (97, 71, 16);
    let mut actual = (0..width * height)
        .map(|i| ((i * 37 + i / 7 * 19) % 256) as u8)
        .collect::<Vec<_>>();
    let mut expected = actual.clone();
    let div = radius * 2 + 1;
    let scale = 1.0 / ((radius + 1) * (radius + 1)) as f32;
    let mut scratch = vec![I32x8::splat(0); div];
    for row in 0..height {
        line(
            &mut expected,
            row * width,
            1,
            width,
            radius,
            scale,
            &mut scratch,
        );
    }
    for column in 0..width {
        line(
            &mut expected,
            column,
            width,
            height,
            radius,
            scale,
            &mut scratch,
        );
    }
    stack(&mut actual, width, height, radius as u32);
    assert_eq!(actual, expected);
}

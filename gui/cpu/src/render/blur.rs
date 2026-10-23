use fearless_simd::{Level, Simd, dispatch, f32x8, i32x8, prelude::*};

pub fn stack(alpha: &mut [u8], width: usize, height: usize, radius: u32) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }

    let radius = radius as usize;
    let div = radius * 2 + 1;
    let scale = 1.0 / ((radius + 1) * (radius + 1)) as f32;
    let mut stack = vec![[0; 8]; div];

    dispatch!(Level::new(), simd => {
        let mut y = 0;
        while y + 8 <= height {
            simd_pass(
                simd,
                alpha,
                width,
                radius,
                &mut stack,
                #[inline(always)]
                |alpha, x| std::array::from_fn(|lane| alpha[(y + lane) * width + x] as i32),
                #[inline(always)]
                |alpha, x, output| {
                    for (lane, output) in output.into_iter().enumerate() {
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
                simd,
                alpha,
                height,
                radius,
                &mut stack,
                #[inline(always)]
                |alpha, y| {
                    let start = y * width + x;
                    alpha[start..start + 8].as_chunks::<8>().0[0].map(i32::from)
                },
                #[inline(always)]
                |alpha, y, output| {
                    let start = y * width + x;
                    alpha[start..start + 8].copy_from_slice(&output);
                },
            );
            x += 8;
        }
        for column in x..width {
            line(alpha, column, width, height, radius, scale, &mut stack);
        }
    });
}

#[inline(always)]
fn simd_pass<S: Simd>(
    simd: S,
    alpha: &mut [u8],
    length: usize,
    radius: usize,
    stack: &mut [[i32; 8]],
    mut load: impl FnMut(&[u8], usize) -> [i32; 8],
    mut store: impl FnMut(&mut [u8], usize, [u8; 8]),
) {
    let div = radius * 2 + 1;
    let scale = 1.0 / ((radius + 1) * (radius + 1)) as f32;
    let first = load(alpha, 0);
    let first_vector = i32x8::simd_from(simd, first);
    let mut sum = i32x8::splat(simd, 0);
    let mut sum_in = i32x8::splat(simd, 0);
    let mut sum_out = i32x8::splat(simd, 0);

    for (i, slot) in stack.iter_mut().take(radius + 1).enumerate() {
        *slot = first;
        sum += first_vector * (i as i32 + 1);
        sum_out += first_vector;
    }
    for i in 1..=radius {
        let source = load(alpha, i.min(length - 1));
        stack[i + radius] = source;
        let source = i32x8::simd_from(simd, source);
        sum += source * (radius + 1 - i) as i32;
        sum_in += source;
    }

    let mut source = radius.min(length - 1);
    let mut position = radius;
    for output in 0..length {
        let blurred = (sum.to_float::<f32x8<S>>() * scale).round().to_int::<i32x8<S>>();
        store(alpha, output, <[i32; 8]>::from(blurred).map(|value| value as u8));

        sum -= sum_out;
        let mut leaving = position + div - radius;
        if leaving >= div {
            leaving -= div;
        }
        sum_out -= i32x8::simd_from(simd, stack[leaving]);
        source = (source + 1).min(length - 1);
        let input = load(alpha, source);
        stack[leaving] = input;
        sum_in += i32x8::simd_from(simd, input);
        sum += sum_in;
        position += 1;
        if position >= div {
            position = 0;
        }
        let entering = i32x8::simd_from(simd, stack[position]);
        sum_out += entering;
        sum_in -= entering;
    }
}

fn line(alpha: &mut [u8], start: usize, step: usize, length: usize, radius: usize, scale: f32, stack: &mut [[i32; 8]]) {
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
    let mut scratch = vec![[0; 8]; div];
    for row in 0..height {
        line(&mut expected, row * width, 1, width, radius, scale, &mut scratch);
    }
    for column in 0..width {
        line(&mut expected, column, width, height, radius, scale, &mut scratch);
    }
    stack(&mut actual, width, height, radius as u32);
    assert_eq!(actual, expected);
}

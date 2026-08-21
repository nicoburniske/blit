use blit::{
    command_list::{BoxShadow, Rectangle},
    geometry::{LogicalRect, PhysicalRect},
    image::{
        ImageData, ImageFit, ImageFormat, ImageHandle, ImagePixels, ImageRequest, ImageSampling,
        ImageTiling, NineSlice,
    },
    style::BorderRadius,
};
use slotmap::SlotMap;

use super::{
    blur,
    rounded::{Radii, RoundedLine},
};
use crate::{RendererImageId, StoredImage};

#[derive(Clone, Copy, PartialEq, Eq)]
struct KeyData {
    width: i32,
    height: i32,
    blur: i32,
    radii: Radii,
    offset_x: i32,
    offset_y: i32,
    spread: i32,
    inset: bool,
}

struct Entry {
    key: KeyData,
    image: ImageHandle,
    bytes: usize,
    nine_slice: NineSlice,
    last_used: u64,
}

#[derive(Default)]
pub struct Cache {
    pub capacity: usize,
    entries: Vec<Entry>,
    bytes: usize,
    clock: u64,
}

pub enum Prepared {
    Rectangle(Rectangle<'static>),
    Image(ImageRequest),
}

impl Cache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            ..Default::default()
        }
    }

    pub fn prepare(
        &mut self,
        images: &mut SlotMap<RendererImageId, StoredImage>,
        shadow: &BoxShadow,
        scale_factor: f32,
    ) -> Option<Prepared> {
        if shadow.color.alpha == 0 {
            return None;
        }
        let (key, bounds) = if shadow.inset {
            let shape = shadow.area.to_physical(scale_factor);
            if shape.width <= 0 || shape.height <= 0 {
                return None;
            }
            (
                KeyData {
                    width: shape.width,
                    height: shape.height,
                    blur: (shadow.blur.max(0.0) * scale_factor).ceil() as i32,
                    radii: Radii::new(shadow.radius, scale_factor, shape.width, shape.height),
                    offset_x: (shadow.offset_x * scale_factor).round() as i32,
                    offset_y: (shadow.offset_y * scale_factor).round() as i32,
                    spread: (shadow.spread * scale_factor).round() as i32,
                    inset: true,
                },
                shape,
            )
        } else {
            let area = LogicalRect {
                x: shadow.area.x + shadow.offset_x - shadow.spread,
                y: shadow.area.y + shadow.offset_y - shadow.spread,
                width: shadow.area.width + shadow.spread * 2.0,
                height: shadow.area.height + shadow.spread * 2.0,
            };
            if area.width <= 0.0 || area.height <= 0.0 {
                return None;
            }
            let radius = BorderRadius {
                top_left: (shadow.radius.top_left + shadow.spread).max(0.0),
                top_right: (shadow.radius.top_right + shadow.spread).max(0.0),
                bottom_right: (shadow.radius.bottom_right + shadow.spread).max(0.0),
                bottom_left: (shadow.radius.bottom_left + shadow.spread).max(0.0),
            };
            if shadow.blur <= 0.0 {
                return Some(Prepared::Rectangle(
                    Rectangle::new(area).background(shadow.color).radius(radius),
                ));
            }
            let shape = area.to_physical(scale_factor);
            if shape.width <= 0 || shape.height <= 0 {
                return None;
            }
            let blur = (shadow.blur * scale_factor).ceil() as i32;
            let diameter = blur.checked_mul(2)?;
            (
                KeyData {
                    width: shape.width,
                    height: shape.height,
                    blur,
                    radii: Radii::new(radius, scale_factor, shape.width, shape.height),
                    offset_x: 0,
                    offset_y: 0,
                    spread: 0,
                    inset: false,
                },
                PhysicalRect {
                    x: shape.x.saturating_sub(blur),
                    y: shape.y.saturating_sub(blur),
                    width: shape.width.checked_add(diameter)?,
                    height: shape.height.checked_add(diameter)?,
                },
            )
        };
        self.clock = self.clock.wrapping_add(1);
        let (image, nine_slice) = if let Some(entry) =
            self.entries.iter_mut().find(|entry| entry.key == key)
        {
            entry.last_used = self.clock;
            (entry.image.id(), entry.nine_slice)
        } else {
            let width = bounds.width as usize;
            let height = bounds.height as usize;
            let mut alpha = if key.inset {
                let margin = key
                    .blur
                    .checked_add(key.offset_x.saturating_abs())?
                    .checked_add(key.offset_y.saturating_abs())?
                    .checked_add(key.spread.saturating_abs())?;
                let padded_width = key.width.checked_add(margin.checked_mul(2)?)? as usize;
                let padded_height = key.height.checked_add(margin.checked_mul(2)?)? as usize;
                let mut alpha = vec![255u8; padded_width.checked_mul(padded_height)?];
                let spread = key.spread.checked_mul(2)?;
                let hole_area = PhysicalRect {
                    x: margin + key.offset_x + key.spread,
                    y: margin + key.offset_y + key.spread,
                    width: key.width.checked_sub(spread)?,
                    height: key.height.checked_sub(spread)?,
                };
                if hole_area.width > 0 && hole_area.height > 0 {
                    let hole_radii = Radii {
                        top_left: key.radii.top_left.saturating_sub(key.spread).max(0),
                        top_right: key.radii.top_right.saturating_sub(key.spread).max(0),
                        bottom_right: key.radii.bottom_right.saturating_sub(key.spread).max(0),
                        bottom_left: key.radii.bottom_left.saturating_sub(key.spread).max(0),
                    };
                    mask_rounded(&mut alpha, padded_width, hole_area, hole_radii)?;
                } else {
                    alpha.fill(0);
                }
                blur::stack(&mut alpha, padded_width, padded_height, key.blur as u32);
                for y in 0..height {
                    for x in 0..width {
                        let source = (y + margin as usize) * padded_width + x + margin as usize;
                        alpha[y * width + x] = 255 - alpha[source];
                    }
                }
                alpha.truncate(width * height);

                let area = PhysicalRect {
                    x: 0,
                    y: 0,
                    width: key.width,
                    height: key.height,
                };
                mask_rounded(&mut alpha, width, area, key.radii)?;
                alpha
            } else {
                let mut alpha = vec![255u8; width.checked_mul(height)?];
                let shape = PhysicalRect {
                    x: key.blur,
                    y: key.blur,
                    width: key.width,
                    height: key.height,
                };
                mask_rounded(&mut alpha, width, shape, key.radii)?;
                blur::stack(&mut alpha, width, height, key.blur as u32);
                alpha
            };
            let center_x = width / 2;
            let mut left = center_x;
            while left > 0
                && (0..height).all(|y| alpha[y * width + left - 1] == alpha[y * width + center_x])
            {
                left -= 1;
            }
            let mut right_start = center_x + 1;
            while right_start < width
                && (0..height)
                    .all(|y| alpha[y * width + right_start] == alpha[y * width + center_x])
            {
                right_start += 1;
            }
            let center_y = height / 2;
            let mut top = center_y;
            while top > 0
                && alpha[(top - 1) * width..top * width]
                    == alpha[center_y * width..(center_y + 1) * width]
            {
                top -= 1;
            }
            let mut bottom_start = center_y + 1;
            while bottom_start < height
                && alpha[bottom_start * width..(bottom_start + 1) * width]
                    == alpha[center_y * width..(center_y + 1) * width]
            {
                bottom_start += 1;
            }

            let removed_columns = right_start - left - 1;
            let removed_rows = bottom_start - top - 1;
            let image_width = width - removed_columns;
            let image_height = height - removed_rows;
            for destination_y in 0..image_height {
                let source_y = if destination_y <= top {
                    destination_y
                } else {
                    destination_y + removed_rows
                };
                for destination_x in 0..image_width {
                    let source_x = if destination_x <= left {
                        destination_x
                    } else {
                        destination_x + removed_columns
                    };
                    alpha[destination_y * image_width + destination_x] =
                        alpha[source_y * width + source_x];
                }
            }
            alpha.truncate(image_width * image_height);
            let nine_slice = NineSlice {
                top: top as u16,
                right: (width - right_start) as u16,
                bottom: (height - bottom_start) as u16,
                left: left as u16,
            };
            let bytes = image_width.checked_mul(image_height)?;
            let data = ImageData::new(
                ImagePixels::Owned(alpha.into_boxed_slice()),
                ImageFormat::Alpha8(blit::color::Color::WHITE),
                image_width,
                image_height,
            );
            let image = StoredImage::insert(images, data);
            let id = image.id();
            self.entries.push(Entry {
                key,
                image,
                bytes,
                nine_slice,
                last_used: self.clock,
            });
            self.bytes += bytes;
            (id, nine_slice)
        };
        Some(Prepared::Image(ImageRequest {
            image,
            area: bounds.to_logical(1.0),
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: Some(shadow.color),
            nine_slice: Some(nine_slice),
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        }))
    }

    pub fn finish_frame(&mut self) {
        while self.bytes > self.capacity {
            let Some(index) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(index, _)| index)
            else {
                break;
            };
            let entry = self.entries.swap_remove(index);
            self.bytes -= entry.bytes;
        }
    }
}

fn mask_rounded(alpha: &mut [u8], stride: usize, area: PhysicalRect, radii: Radii) -> Option<()> {
    alpha[..area.y as usize * stride].fill(0);
    for y in area.y..area.y + area.height {
        let line = RoundedLine::new(area, radii, y)?;
        let start = line.visible_start().max(0) as usize;
        let end = line.visible_end().min(stride as i32) as usize;
        let full_start = line.full_start().clamp(start as i32, end as i32) as usize;
        let full_end = line.full_end().clamp(start as i32, end as i32) as usize;
        let row = &mut alpha[y as usize * stride..][..stride];
        row[..start].fill(0);
        if full_start <= full_end {
            for (x, value) in row[start..full_start].iter_mut().enumerate() {
                *value = (*value as u16 * line.coverage((start + x) as i32) as u16 / 255) as u8;
            }
            for (x, value) in row[full_end..end].iter_mut().enumerate() {
                *value = (*value as u16 * line.coverage((full_end + x) as i32) as u16 / 255) as u8;
            }
        } else {
            for (x, value) in row[start..end].iter_mut().enumerate() {
                *value = (*value as u16 * line.coverage((start + x) as i32) as u16 / 255) as u8;
            }
        }
        row[end..].fill(0);
    }
    alpha[(area.y + area.height) as usize * stride..].fill(0);
    Some(())
}

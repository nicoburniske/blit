use blit::{
    geometry::{LogicalRect, PhysicalRect},
    paint::{BorderRadius, BoxShadow, ImageFit, ImageRequest, ImageSampling, ImageTiling, Rectangle},
    resource::{ImageData, ImageFormat, ImageId, ImagePixels},
};
use slotmap::{Key, SlotMap};

use super::rounded::{Radii, RoundedLine};
use crate::{RendererImageId, StoredImage};

#[derive(Clone, Copy, PartialEq, Eq)]
struct KeyData {
    width: i32,
    height: i32,
    blur: i32,
    radii: Radii,
}

struct Entry {
    key: KeyData,
    image: RendererImageId,
    bytes: usize,
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
    pub fn new(capacity: usize) -> Self { Self { capacity, ..Default::default() } }

    pub fn prepare(
        &mut self,
        images: &mut SlotMap<RendererImageId, StoredImage>,
        shadow: &BoxShadow,
        scale_factor: f32,
    ) -> Option<Prepared> {
        if shadow.color.alpha == 0 {
            return None;
        }
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
            return Some(Prepared::Rectangle(Rectangle::new(area).background(shadow.color).radius(radius)));
        }
        let shape = area.to_physical(scale_factor);
        if shape.width <= 0 || shape.height <= 0 {
            return None;
        }
        let radii = Radii::new(radius, scale_factor, shape.width, shape.height);
        let key = KeyData {
            width: shape.width,
            height: shape.height,
            blur: (shadow.blur * scale_factor).ceil() as i32,
            radii,
        };
        let diameter = key.blur.checked_mul(2)?;
        let bounds = PhysicalRect {
            x: shape.x.saturating_sub(key.blur),
            y: shape.y.saturating_sub(key.blur),
            width: shape.width.checked_add(diameter)?,
            height: shape.height.checked_add(diameter)?,
        };
        self.clock = self.clock.wrapping_add(1);
        let image = if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.last_used = self.clock;
            entry.image
        } else {
            let width = bounds.width as usize;
            let height = bounds.height as usize;
            let bytes = width.checked_mul(height)?;
            let mut alpha = vec![0u8; bytes];
            let shape = PhysicalRect { x: key.blur, y: key.blur, width: key.width, height: key.height };
            for y in shape.y..shape.y + shape.height {
                let line = RoundedLine::new(shape, radii, y)?;
                for x in line.visible_start().max(shape.x)..line.visible_end().min(shape.x + shape.width) {
                    alpha[y as usize * width + x as usize] = line.coverage(x);
                }
            }
            let mut scratch = vec![0u8; bytes];
            let radius = key.blur / 3;
            let remainder = key.blur % 3;
            for radius in [radius + i32::from(remainder > 0), radius + i32::from(remainder > 1), radius] {
                if radius != 0 {
                    box_blur(&alpha, &mut scratch, width, height, radius, true);
                    box_blur(&scratch, &mut alpha, width, height, radius, false);
                }
            }
            let data = ImageData::new(
                ImagePixels::Owned(alpha.into_boxed_slice()),
                ImageFormat::Alpha8(blit::color::Color::WHITE),
                width,
                height,
            );
            let image = images.insert(StoredImage::new(data));
            self.entries.push(Entry { key, image, bytes, last_used: self.clock });
            self.bytes += bytes;
            image
        };
        Some(Prepared::Image(ImageRequest {
            image: ImageId(image.data().as_ffi()),
            area: bounds.to_logical(1.0),
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: Some(shadow.color),
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        }))
    }

    pub fn finish_frame(&mut self, images: &mut SlotMap<RendererImageId, StoredImage>) {
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
            images.remove(entry.image);
        }
    }
}

fn box_blur(
    source: &[u8],
    destination: &mut [u8],
    width: usize,
    height: usize,
    radius: i32,
    horizontal: bool,
) {
    let radius = radius as usize;
    let divisor = radius * 2 + 1;
    let (lines, length, stride) = if horizontal { (height, width, 1) } else { (width, height, width) };
    for line in 0..lines {
        let base = if horizontal { line * width } else { line };
        let mut sum = 0usize;
        for index in 0..=radius.min(length - 1) {
            sum += source[base + index * stride] as usize;
        }
        for index in 0..length {
            destination[base + index * stride] = ((sum + divisor / 2) / divisor) as u8;
            if index >= radius {
                sum -= source[base + (index - radius) * stride] as usize;
            }
            if index + radius + 1 < length {
                sum += source[base + (index + radius + 1) * stride] as usize;
            }
        }
    }
}

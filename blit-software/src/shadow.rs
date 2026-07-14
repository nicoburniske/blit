use blit::{
    ImageData, ImageFormat, ImageId, ImagePixels, LogicalRect, PhysicalRect,
    widgets::{
        BorderRadius, BoxShadowRequest, ImageFit, ImageRequest, ImageSampling, ImageTiling,
        NineSlice, Rectangle,
    },
};
use slotmap::{Key, SlotMap};

use crate::{RendererImageId, StoredImage, rectangle::rounded::Radii};

const CACHE_CAPACITY: usize = 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
struct KeyData {
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
    entries: Vec<Entry>,
    bytes: usize,
    clock: u64,
}

pub enum Prepared {
    Rectangle(Rectangle),
    Image(ImageRequest),
}

impl Cache {
    pub fn prepare(
        &mut self,
        images: &mut SlotMap<RendererImageId, StoredImage>,
        request: &BoxShadowRequest,
        scale_factor: f32,
    ) -> Option<Prepared> {
        let shadow = request.shadow;
        if shadow.color.alpha == 0 {
            return None;
        }
        let area = LogicalRect {
            x: request.area.x + shadow.offset_x - shadow.spread,
            y: request.area.y + shadow.offset_y - shadow.spread,
            width: request.area.width + shadow.spread * 2.0,
            height: request.area.height + shadow.spread * 2.0,
        };
        if area.width <= 0.0 || area.height <= 0.0 {
            return None;
        }
        let radius = BorderRadius {
            top_left: (request.radius.top_left + shadow.spread).max(0.0),
            top_right: (request.radius.top_right + shadow.spread).max(0.0),
            bottom_right: (request.radius.bottom_right + shadow.spread).max(0.0),
            bottom_left: (request.radius.bottom_left + shadow.spread).max(0.0),
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
        let radii = Radii::new(radius, scale_factor, shape.width, shape.height);
        let key = KeyData {
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
        let side = |radius| diameter.checked_add(radius)?.try_into().ok();
        let slice = NineSlice {
            top: side(radii.top_left.max(radii.top_right))?,
            right: side(radii.top_right.max(radii.bottom_right))?,
            bottom: side(radii.bottom_left.max(radii.bottom_right))?,
            left: side(radii.top_left.max(radii.bottom_left))?,
        };
        self.clock = self.clock.wrapping_add(1);
        let image = if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == key) {
            entry.last_used = self.clock;
            entry.image
        } else {
            let width = usize::from(slice.left) + usize::from(slice.right) + 1;
            let height = usize::from(slice.top) + usize::from(slice.bottom) + 1;
            let bytes = width.checked_mul(height)?;
            let mut alpha = vec![0u8; bytes];
            let shape = PhysicalRect {
                x: key.blur,
                y: key.blur,
                width: width as i32 - diameter,
                height: height as i32 - diameter,
            };
            for y in shape.y..shape.y + shape.height {
                let line = super::rectangle::rounded::RoundedLine::new(shape, radii, y)?;
                for x in
                    line.visible_start().max(shape.x)..line.visible_end().min(shape.x + shape.width)
                {
                    alpha[y as usize * width + x as usize] = line.coverage(x);
                }
            }
            let mut scratch = vec![0u8; bytes];
            let radius = key.blur / 3;
            let remainder = key.blur % 3;
            for radius in [
                radius + i32::from(remainder > 0),
                radius + i32::from(remainder > 1),
                radius,
            ] {
                if radius != 0 {
                    box_blur(&alpha, &mut scratch, width, height, radius, true);
                    box_blur(&scratch, &mut alpha, width, height, radius, false);
                }
            }
            let data = ImageData::new(
                ImagePixels::Owned(alpha.into_boxed_slice()),
                ImageFormat::Alpha8(blit::Color::WHITE),
                width,
                height,
            );
            let image = images.insert(StoredImage { data, live: true });
            self.entries.push(Entry {
                key,
                image,
                bytes,
                last_used: self.clock,
            });
            self.bytes += bytes;
            image
        };
        Some(Prepared::Image(ImageRequest {
            image: ImageId(image.data().as_ffi()),
            area: bounds.to_logical(1.0),
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: Some(request.shadow.color),
            nine_slice: Some(slice),
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        }))
    }

    pub fn finish_frame(&mut self, images: &mut SlotMap<RendererImageId, StoredImage>) {
        while self.bytes > CACHE_CAPACITY {
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
    let (lines, length, stride) = if horizontal {
        (height, width, 1)
    } else {
        (width, height, width)
    };
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

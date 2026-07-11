use bullseye::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct PremultipliedRgbaColor {
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl PremultipliedRgbaColor {
    pub fn new(color: Color, coverage: u8) -> Self {
        let alpha = color.alpha as u16 * coverage as u16 / 255;
        Self {
            alpha: alpha as u8,
            red: (color.red as u16 * alpha / 255) as u8,
            green: (color.green as u16 * alpha / 255) as u8,
            blue: (color.blue as u16 * alpha / 255) as u8,
        }
    }

    pub fn with_opacity(color: Color, opacity: f32) -> Self {
        Self::new(color, (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    pub fn coverage(self, coverage: u32) -> Self {
        Self {
            alpha: (self.alpha as u32 * coverage / 255) as u8,
            red: (self.red as u32 * coverage / 255) as u8,
            green: (self.green as u32 * coverage / 255) as u8,
            blue: (self.blue as u32 * coverage / 255) as u8,
        }
    }
}

pub trait Pixel: Copy {
    fn blend(&mut self, color: PremultipliedRgbaColor);

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self;

    fn background() -> Self {
        Self::from_rgb(0, 0, 0)
    }

    fn blend_slice(pixels: &mut [Self], color: PremultipliedRgbaColor) {
        if color.alpha == 255 {
            pixels.fill(Self::from_rgb(color.red, color.green, color.blue));
        } else {
            for pixel in pixels {
                pixel.blend(color);
            }
        }
    }

    fn blend_alpha_slice(pixels: &mut [Self], color: Color, alpha: &[u8]) {
        for (pixel, alpha) in pixels.iter_mut().zip(alpha) {
            pixel.blend(PremultipliedRgbaColor::new(color, *alpha));
        }
    }
}

impl Pixel for u32 {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let inverse = 255 - color.alpha as u32;
        let red = ((*self >> 16) & 0xff) * inverse / 255 + color.red as u32;
        let green = ((*self >> 8) & 0xff) * inverse / 255 + color.green as u32;
        let blue = (*self & 0xff) * inverse / 255 + color.blue as u32;
        *self = red << 16 | green << 8 | blue;
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        (red as u32) << 16 | (green as u32) << 8 | blue as u32
    }
}

pub trait PixelBuffer {
    type Pixel: Pixel;

    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn line_mut(&mut self, line: usize) -> &mut [Self::Pixel];
}

pub struct VecBuffer<P> {
    pixels: Vec<P>,
    width: usize,
    height: usize,
}

impl<P: Pixel> VecBuffer<P> {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![P::background(); width * height],
            width,
            height,
        }
    }

    pub fn pixels(&self) -> &[P] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [P] {
        &mut self.pixels
    }
}

impl<P: Pixel> PixelBuffer for VecBuffer<P> {
    type Pixel = P;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn line_mut(&mut self, line: usize) -> &mut [P] {
        let start = line * self.width;
        &mut self.pixels[start..start + self.width]
    }
}

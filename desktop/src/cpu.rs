use std::{num::NonZeroU32, ptr::NonNull, sync::Arc};

use blit_cpu::{PixelBuffer, Renderer, Scanline, Xrgb8888};
use blit_gui::RenderInput;
use softbuffer::{Context, Surface};
use winit::{dpi::PhysicalSize, window::Window};

use crate::{GraphicsBackend, GraphicsError};

pub use blit_cpu::RendererConfig as Config;

pub struct Backend {
    active: Option<Active>,
    renderer: Renderer<DesktopBuffer, Scanline>,
}

impl Backend {
    pub fn new(config: Config) -> Self {
        Self {
            active: None,
            renderer: Renderer::new(DesktopBuffer::new(1, 1), config).strategy(Scanline::default()),
        }
    }
}

impl GraphicsBackend for Backend {
    fn resume(&mut self, window: Arc<Window>) -> Result<(), GraphicsError> {
        let context = Context::new(window.clone())?;
        let surface = Surface::new(&context, window.clone())?;
        self.active = Some(Active { window, surface });
        Ok(())
    }

    fn suspend(&mut self) {
        self.active = None;
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), GraphicsError> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        active.surface.resize(width, height)?;
        self.renderer
            .buffer_mut()
            .resize(size.width as usize, size.height as usize);
        self.renderer.invalidate_all();
        Ok(())
    }

    fn render(&mut self, input: RenderInput<'_>) -> Result<bool, GraphicsError> {
        let Some(active) = &mut self.active else {
            return Ok(false);
        };
        let mut buffer = active.surface.buffer_mut()?;
        if buffer.age() == 0 {
            self.renderer.invalidate_all();
        }
        self.renderer.buffer_mut().set(&mut buffer);
        self.renderer.render(input);
        active.window.pre_present_notify();
        buffer.present()?;
        Ok(true)
    }
}

struct Active {
    window: Arc<Window>,
    surface: Surface<Arc<Window>, Arc<Window>>,
}

struct DesktopBuffer {
    pixels: NonNull<Xrgb8888>,
    width: usize,
    height: usize,
}

impl DesktopBuffer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: NonNull::dangling(),
            width,
            height,
        }
    }

    fn resize(&mut self, width: usize, height: usize) {
        self.pixels = NonNull::dangling();
        self.width = width;
        self.height = height;
    }

    fn set(&mut self, pixels: &mut [u32]) {
        assert!(pixels.len() >= self.width * self.height);
        self.pixels = NonNull::new(pixels.as_mut_ptr().cast()).expect("softbuffer pixels");
    }
}

impl PixelBuffer for DesktopBuffer {
    type Pixel = Xrgb8888;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn line_mut(&mut self, line: usize) -> &mut [Self::Pixel] {
        assert!(line < self.height);
        // safety: set provides writable u32 pixels and Xrgb8888 is transparent over u32
        unsafe {
            std::slice::from_raw_parts_mut(self.pixels.as_ptr().add(line * self.width), self.width)
        }
    }
}

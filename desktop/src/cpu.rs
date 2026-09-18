use std::time::{Duration, Instant};
use std::{num::NonZeroU32, ptr::NonNull, sync::Arc};

use blit_cpu::{PixelBuffer, Renderer, Scanline, Xrgb8888};
use blit_gui::RenderInput;
use softbuffer::{Context, Surface};
use winit::{dpi::PhysicalSize, window::Window};

use crate::{GraphicsBackend, GraphicsError};

pub use blit_cpu::RendererConfig as Config;

pub struct Backend {
    config: Config,
    window: Option<Arc<Window>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    renderer: Option<Renderer<DesktopBuffer, Scanline>>,
}

impl Backend {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            window: None,
            surface: None,
            renderer: None,
        }
    }
}

impl GraphicsBackend for Backend {
    fn resume(&mut self, window: Arc<Window>) -> Result<(), GraphicsError> {
        let size = window.inner_size();
        let size = PhysicalSize::new(size.width.max(1), size.height.max(1));
        let context = Context::new(window.clone())?;
        let surface = Surface::new(&context, window.clone())?;
        if self.renderer.is_none() {
            self.renderer = Some(
                Renderer::new(
                    DesktopBuffer::new(size.width as usize, size.height as usize),
                    self.config,
                )
                .strategy(Scanline::default()),
            );
        }
        self.window = Some(window);
        self.surface = Some(surface);
        Ok(())
    }

    fn suspend(&mut self) {
        self.surface = None;
        self.window = None;
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), GraphicsError> {
        let (Some(surface), Some(renderer)) = (&mut self.surface, &mut self.renderer) else {
            return Ok(());
        };
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        surface.resize(width, height)?;
        renderer
            .buffer_mut()
            .resize(size.width as usize, size.height as usize);
        renderer.invalidate_all();
        Ok(())
    }

    fn render(&mut self, input: RenderInput<'_>) -> Result<Duration, GraphicsError> {
        let surface = self.surface.as_mut().expect("CPU backend is not active");
        let renderer = self.renderer.as_mut().expect("CPU backend is not active");
        let window = self.window.as_ref().expect("CPU backend is not active");
        let mut buffer = surface.buffer_mut()?;
        if buffer.age() == 0 {
            renderer.invalidate_all();
        }
        renderer.buffer_mut().set(&mut buffer);
        let started = Instant::now();
        renderer.render(input);
        let render_time = started.elapsed();
        window.pre_present_notify();
        buffer.present()?;
        Ok(render_time)
    }
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

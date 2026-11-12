use std::{io::Error as IoError, sync::Arc};

use blit::{LogicalRect, Paint};
use blit_gpu::Renderer;
use blit_gui::{
    TextSystem,
    image::{ImageData, ImageHandle},
    style::BorderRadius,
};
use wgpu::{CurrentSurfaceTexture, SurfaceConfiguration};
use winit::{dpi::PhysicalSize, window::Window};

use crate::{GraphicsBackend, GraphicsError};

#[cfg(target_os = "linux")]
const BACKENDS: wgpu::Backends = wgpu::Backends::VULKAN;
#[cfg(target_os = "macos")]
const BACKENDS: wgpu::Backends = wgpu::Backends::METAL;

pub use blit_gpu::RendererConfig as Config;

pub struct Backend {
    pending: Option<(TextSystem, Config)>,
    instance: wgpu::Instance,
    gpu: Option<Gpu>,
    active: Option<Active>,
}

impl Backend {
    pub fn new(text: TextSystem, config: Config) -> Self {
        Self {
            pending: Some((text, config)),
            instance: wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: BACKENDS,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            }),
            gpu: None,
            active: None,
        }
    }
}

impl blit::Context for Backend {
    type Scalar = f32;
}

impl blit::FrameStore for Backend {
    fn frame_data(&self) -> &blit::FrameData {
        self.gpu
            .as_ref()
            .expect("GPU renderer is not initialized")
            .renderer
            .frame_data()
    }

    fn slice<T: Copy + 'static>(&mut self, capacity: usize) -> blit::FrameSliceBuilder<'_, T> {
        self.gpu
            .as_mut()
            .expect("GPU renderer is not initialized")
            .renderer
            .slice(capacity)
    }
}

impl<T> Paint<T> for Backend
where
    Renderer: Paint<T>,
{
    fn paint(&mut self, bounds: LogicalRect, command: T) {
        self.gpu
            .as_mut()
            .expect("GPU renderer is not initialized")
            .renderer
            .paint(bounds, command);
    }
}

impl blit_gui::Context for Backend {
    fn text(&mut self) -> &mut TextSystem {
        if let Some(gpu) = &mut self.gpu {
            gpu.renderer.text()
        } else {
            &mut self.pending.as_mut().expect("GPU renderer config").0
        }
    }

    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.gpu
            .as_mut()
            .expect("GPU renderer is not initialized")
            .renderer
            .push_clip(area, radius);
    }

    fn pop_clip(&mut self) {
        self.gpu
            .as_mut()
            .expect("GPU renderer is not initialized")
            .renderer
            .pop_clip();
    }
}

impl GraphicsBackend for Backend {
    fn create_image(&mut self, data: ImageData) -> Result<ImageHandle, GraphicsError> {
        let gpu = self
            .gpu
            .as_mut()
            .ok_or_else(|| IoError::other("GPU renderer is not initialized"))?;
        Ok(gpu.renderer.create_image(data))
    }

    fn resume(&mut self, window: Arc<Window>) -> Result<(), GraphicsError> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let surface = self.instance.create_surface(window.clone())?;

        let surface_config = if let Some(gpu) = &self.gpu {
            gpu.surface_config(&surface, width, height)?
        } else {
            let adapter = blit_executor::block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            }))?;
            let mut surface_config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| IoError::other("GPU surface is not supported by the adapter"))?;
            let formats = surface.get_capabilities(&adapter).formats;
            let linear_format = surface_config.format.remove_srgb_suffix();
            if formats.contains(&linear_format) {
                surface_config.format = linear_format;
            } else {
                surface_config.format = formats
                    .into_iter()
                    .find(|format| !format.is_srgb())
                    .ok_or_else(|| IoError::other("GPU surface has no non-sRGB format"))?;
            }
            let (device, queue) = blit_executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                ..Default::default()
            }))?;
            let (text, config) = self.pending.take().expect("GPU renderer config");
            let renderer = Renderer::new(device.clone(), queue.clone(), surface_config.format, text, config);
            self.gpu = Some(Gpu {
                renderer,
                adapter,
                device,
                queue,
                format: surface_config.format,
            });
            surface_config
        };
        self.active = Some(Active {
            surface,
            config: surface_config,
            window,
        });
        Ok(())
    }

    fn suspend(&mut self) {
        self.active = None;
        if let Some(gpu) = &mut self.gpu {
            gpu.renderer.discard_frame();
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), GraphicsError> {
        let (Some(active), Some(gpu)) = (&mut self.active, &self.gpu) else {
            return Ok(());
        };
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        active.config.width = size.width;
        active.config.height = size.height;
        active.surface.configure(&gpu.device, &active.config);
        Ok(())
    }

    fn set_scale(&mut self, scale: f32) {
        self.gpu
            .as_mut()
            .expect("GPU renderer is not initialized")
            .renderer
            .set_scale(scale);
    }

    fn render(&mut self) -> Result<bool, GraphicsError> {
        let (Some(active), Some(gpu)) = (&mut self.active, &mut self.gpu) else {
            return Ok(false);
        };
        let mut retried = false;
        let (frame, suboptimal) = loop {
            match active.surface.get_current_texture() {
                CurrentSurfaceTexture::Success(frame) => break (frame, false),
                CurrentSurfaceTexture::Suboptimal(frame) => break (frame, true),
                CurrentSurfaceTexture::Timeout => {
                    gpu.renderer.discard_frame();
                    active.window.request_redraw();
                    return Ok(false);
                }
                CurrentSurfaceTexture::Occluded => {
                    gpu.renderer.discard_frame();
                    return Ok(false);
                }
                status @ (CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost) if !retried => {
                    retried = true;
                    if matches!(status, CurrentSurfaceTexture::Lost) {
                        let surface = self.instance.create_surface(active.window.clone())?;
                        let config = gpu.surface_config(&surface, active.config.width, active.config.height)?;
                        surface.configure(&gpu.device, &config);
                        active.surface = surface;
                        active.config = config;
                    } else {
                        active.surface.configure(&gpu.device, &active.config);
                    }
                }
                CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                    gpu.renderer.discard_frame();
                    active.window.request_redraw();
                    return Ok(false);
                }
                CurrentSurfaceTexture::Validation => {
                    return Err(IoError::other("GPU surface validation failed").into());
                }
            }
        };

        gpu.renderer.render(&frame.texture);
        active.window.pre_present_notify();
        gpu.queue.present(frame);
        if suboptimal {
            active.surface.configure(&gpu.device, &active.config);
        }
        Ok(true)
    }
}

struct Active {
    surface: wgpu::Surface<'static>,
    config: SurfaceConfiguration,
    window: Arc<Window>,
}

struct Gpu {
    renderer: Renderer,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
}

impl Gpu {
    fn surface_config(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
    ) -> Result<SurfaceConfiguration, IoError> {
        let mut config = surface
            .get_default_config(&self.adapter, width, height)
            .ok_or_else(|| IoError::other("GPU surface is not supported by the adapter"))?;
        if !surface.get_capabilities(&self.adapter).formats.contains(&self.format) {
            return Err(IoError::other("GPU surface format changed"));
        }
        config.format = self.format;
        Ok(config)
    }
}

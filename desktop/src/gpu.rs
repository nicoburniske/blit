use std::io::Error as IoError;
use std::sync::Arc;
use std::time::Instant;

use blit_gpu::Renderer;
use blit_gui::RenderInput;
use wgpu::{CurrentSurfaceTexture, SurfaceConfiguration};
use winit::{dpi::PhysicalSize, window::Window};

use crate::{GraphicsBackend, GraphicsError, RenderOutcome};

#[cfg(target_os = "linux")]
const BACKENDS: wgpu::Backends = wgpu::Backends::VULKAN;
#[cfg(target_os = "macos")]
const BACKENDS: wgpu::Backends = wgpu::Backends::METAL;

pub use blit_gpu::RendererConfig as Config;

pub struct Backend {
    config: Option<Config>,
    instance: wgpu::Instance,
    gpu: Option<Gpu>,
    active: Option<Active>,
}

impl Backend {
    pub fn new(config: Config) -> Self {
        Self {
            config: Some(config),
            instance: wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: BACKENDS,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            }),
            gpu: None,
            active: None,
        }
    }
}

impl GraphicsBackend for Backend {
    fn resume(&mut self, window: Arc<Window>) -> Result<(), GraphicsError> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let surface = self.instance.create_surface(window.clone())?;

        let surface_config = if let Some(gpu) = &self.gpu {
            gpu.surface_config(&surface, width, height)?
        } else {
            let adapter = blit_executor::block_on(self.instance.request_adapter(
                &wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                },
            ))?;
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
            let (device, queue) =
                blit_executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                    ..Default::default()
                }))?;
            let renderer = Renderer::new(
                device.clone(),
                queue.clone(),
                surface_config.format,
                self.config.take().expect("GPU renderer config"),
            );
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

    fn render(&mut self, input: RenderInput<'_>) -> Result<RenderOutcome, GraphicsError> {
        let (Some(active), Some(gpu)) = (&mut self.active, &mut self.gpu) else {
            return Ok(RenderOutcome::Deferred);
        };
        let mut retried = false;
        let (frame, suboptimal) = loop {
            match active.surface.get_current_texture() {
                CurrentSurfaceTexture::Success(frame) => break (frame, false),
                CurrentSurfaceTexture::Suboptimal(frame) => break (frame, true),
                CurrentSurfaceTexture::Timeout => {
                    active.window.request_redraw();
                    return Ok(RenderOutcome::Deferred);
                }
                CurrentSurfaceTexture::Occluded => {
                    return Ok(RenderOutcome::Deferred);
                }
                status @ (CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost)
                    if !retried =>
                {
                    retried = true;
                    if matches!(status, CurrentSurfaceTexture::Lost) {
                        let surface = self.instance.create_surface(active.window.clone())?;
                        let config = gpu.surface_config(
                            &surface,
                            active.config.width,
                            active.config.height,
                        )?;
                        surface.configure(&gpu.device, &config);
                        active.surface = surface;
                        active.config = config;
                    } else {
                        active.surface.configure(&gpu.device, &active.config);
                    }
                }
                CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                    active.window.request_redraw();
                    return Ok(RenderOutcome::Deferred);
                }
                CurrentSurfaceTexture::Validation => {
                    return Err(IoError::other("GPU surface validation failed").into());
                }
            }
        };

        let started = Instant::now();
        gpu.renderer.render(&frame.texture, input);
        let render_time = started.elapsed();
        active.window.pre_present_notify();
        gpu.queue.present(frame);
        if suboptimal {
            active.surface.configure(&gpu.device, &active.config);
        }
        Ok(RenderOutcome::Presented(render_time))
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
        if !surface
            .get_capabilities(&self.adapter)
            .formats
            .contains(&self.format)
        {
            return Err(IoError::other("GPU surface format changed"));
        }
        config.format = self.format;
        Ok(config)
    }
}

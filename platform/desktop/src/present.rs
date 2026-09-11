//! window presentation, either through softbuffer or through the GPU renderer

use std::sync::Arc;

use winit::{dpi::PhysicalSize, event_loop::OwnedDisplayHandle, window::Window};

use crate::{DesktopPlatform, RunError};

pub use backend::Presenter;

#[cfg(not(feature = "gpu"))]
mod backend {
    use std::num::NonZeroU32;

    use softbuffer::{Context, Surface};

    use super::*;

    pub struct Presenter {
        surface: Surface<OwnedDisplayHandle, Arc<Window>>,
    }

    impl Presenter {
        pub fn new(
            display: OwnedDisplayHandle,
            window: Arc<Window>,
            _size: PhysicalSize<u32>,
        ) -> Result<Self, RunError> {
            let context = Context::new(display)?;
            Ok(Self {
                surface: Surface::new(&context, window)?,
            })
        }

        pub fn resize(
            &mut self,
            size: PhysicalSize<u32>,
            platform: &mut DesktopPlatform,
        ) -> Result<(), RunError> {
            let (Some(width), Some(height)) =
                (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
            else {
                return Ok(());
            };
            self.surface.resize(width, height)?;
            platform
                .renderer_mut()
                .buffer_mut()
                .resize(size.width as usize, size.height as usize);
            Ok(())
        }

        /// renders and shows the frame; true when it reached the window
        pub fn present(
            &mut self,
            window: &Window,
            platform: &mut DesktopPlatform,
            render: impl FnOnce(&mut DesktopPlatform),
        ) -> Result<bool, RunError> {
            let mut buffer = self.surface.buffer_mut()?;
            if buffer.age() == 0 {
                platform.invalidate_all();
            }
            platform.renderer_mut().buffer_mut().set(&mut buffer);
            render(platform);
            window.pre_present_notify();
            buffer.present()?;
            Ok(true)
        }
    }
}

#[cfg(feature = "gpu")]
mod backend {
    use super::*;

    pub struct Presenter {
        renderer: blit_gpu::Renderer,
    }

    impl Presenter {
        pub fn new(
            _display: OwnedDisplayHandle,
            window: Arc<Window>,
            size: PhysicalSize<u32>,
        ) -> Result<Self, RunError> {
            Ok(Self {
                renderer: blit_gpu::Renderer::new(window, size.width, size.height)?,
            })
        }

        pub fn resize(
            &mut self,
            size: PhysicalSize<u32>,
            platform: &mut DesktopPlatform,
        ) -> Result<(), RunError> {
            self.renderer.resize(size.width, size.height);
            platform
                .renderer_mut()
                .buffer_mut()
                .resize(size.width as usize, size.height as usize);
            Ok(())
        }

        /// renders and shows the frame; false when the surface had no frame
        /// to give, as before the window is on screen
        pub fn present(
            &mut self,
            window: &Window,
            platform: &mut DesktopPlatform,
            render: impl FnOnce(&mut DesktopPlatform),
        ) -> Result<bool, RunError> {
            render(platform);
            window.pre_present_notify();
            let (commands, text, scale) = platform.frame();
            let presented = self.renderer.render(commands, text, scale)?;
            text.finish_frame();
            Ok(presented)
        }
    }
}

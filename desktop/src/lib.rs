#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("blit-desktop currently supports Linux and macOS only");

pub use blit_executor::{AppMut, Project, Scope, ScopeRef, TaskId};
pub use winit::event_loop::EventLoopClosed;

pub use self::{
    context::DesktopContext,
    event_loop::{Session, run},
};

mod context;
#[cfg(feature = "cpu")]
pub mod cpu;
mod event_loop;
#[cfg(feature = "gpu")]
pub mod gpu;

use std::{error::Error, fmt, sync::Arc};

use blit_gui::image::{ImageData, ImageHandle};
use winit::{dpi::PhysicalSize, event_loop::EventLoopProxy as WinitEventLoopProxy, window::Window};

pub struct Config<G> {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub graphics: G,
}

pub type Ui<'a, G, S = blit::state::Build> = blit::Ui<'a, DesktopContext<G>, S>;

pub type GraphicsError = Box<dyn Error>;

pub trait GraphicsBackend: blit_gui::Context + 'static {
    fn create_image(&mut self, data: ImageData) -> Result<ImageHandle, GraphicsError>;

    fn resume(&mut self, window: Arc<Window>) -> Result<(), GraphicsError>;

    fn suspend(&mut self);

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), GraphicsError>;

    fn set_scale(&mut self, scale: f32);

    /// returns whether a frame was presented
    ///
    /// request a redraw before returning false to retry without new input
    fn render(&mut self) -> Result<bool, GraphicsError>;
}

/// sends application input to the desktop event loop
pub struct EventLoopProxy<T: 'static> {
    inner: WinitEventLoopProxy<event_loop::Event<T>>,
}

impl<T> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> EventLoopProxy<T> {
    pub fn send_event(&self, input: T) -> Result<(), EventLoopClosed<T>> {
        self.inner
            .send_event(event_loop::Event::Input(input))
            .map_err(|error| match error.0 {
                event_loop::Event::Input(input) => EventLoopClosed(input),
                event_loop::Event::TasksReady => unreachable!(),
            })
    }
}

pub trait Application: Sized + 'static {
    type Input: Send + 'static;
    type Graphics: GraphicsBackend;

    fn new(
        input: EventLoopProxy<Self::Input>,
        scope: Scope<Self>,
        context: &mut DesktopContext<Self::Graphics>,
    ) -> Self;

    fn input(&mut self, input: Self::Input);

    fn build(&mut self, ui: Ui<'_, Self::Graphics>);
}

#[derive(Debug)]
pub enum RunError {
    EventLoop(winit::error::EventLoopError),
    Window(winit::error::OsError),
    Graphics(GraphicsError),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(error) => error.fmt(formatter),
            Self::Window(error) => error.fmt(formatter),
            Self::Graphics(error) => error.fmt(formatter),
        }
    }
}

impl Error for RunError {}

impl From<winit::error::EventLoopError> for RunError {
    fn from(error: winit::error::EventLoopError) -> Self {
        Self::EventLoop(error)
    }
}

impl From<winit::error::OsError> for RunError {
    fn from(error: winit::error::OsError) -> Self {
        Self::Window(error)
    }
}

impl From<GraphicsError> for RunError {
    fn from(error: GraphicsError) -> Self {
        Self::Graphics(error)
    }
}

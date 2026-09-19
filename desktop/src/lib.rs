#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("blit-desktop currently supports Linux and macOS only");

pub use blit_executor::{AppMut, Project, Root, Scope, ScopeRef, TaskId};
pub use event_loop::run;
pub use winit::event_loop::EventLoopClosed;

#[cfg(feature = "cpu")]
pub mod cpu;
mod event_loop;
#[cfg(feature = "gpu")]
pub mod gpu;

use std::{error::Error, fmt, sync::Arc, time::Duration};

use blit_gui::{GuiContext, RenderInput, TextConfig, Ui};
use winit::event_loop::EventLoopProxy as WinitEventLoopProxy;
use winit::{dpi::PhysicalSize, window::Window};

pub struct Config<T> {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub text_config: TextConfig,
    pub text: T,
    pub graphics: Box<dyn GraphicsBackend>,
}

pub type GraphicsError = Box<dyn Error>;

pub enum RenderOutcome {
    Presented(Duration),
    Deferred,
}

pub trait GraphicsBackend: 'static {
    fn resume(&mut self, window: Arc<Window>) -> Result<(), GraphicsError>;

    fn suspend(&mut self);

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), GraphicsError>;

    fn render(&mut self, input: RenderInput<'_>) -> Result<RenderOutcome, GraphicsError>;
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

    fn new(input: EventLoopProxy<Self::Input>, root: Root<Self>, gui: &mut GuiContext) -> Self;

    fn input(&mut self, input: Self::Input);

    fn render(&mut self, ui: Ui<'_>);
}

#[derive(Debug)]
pub enum RunError {
    Font(blit_gui::FontError),
    EventLoop(winit::error::EventLoopError),
    Window(winit::error::OsError),
    Graphics(GraphicsError),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Font(error) => error.fmt(formatter),
            Self::EventLoop(error) => error.fmt(formatter),
            Self::Window(error) => error.fmt(formatter),
            Self::Graphics(error) => error.fmt(formatter),
        }
    }
}

impl Error for RunError {}

impl From<blit_gui::FontError> for RunError {
    fn from(error: blit_gui::FontError) -> Self {
        Self::Font(error)
    }
}

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

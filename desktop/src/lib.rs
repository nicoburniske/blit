#![feature(portable_simd)]

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("blit-desktop currently supports Linux and macOS only");

pub use blit_executor::{AppMut, Project, Root, Scope, ScopeRef, TaskId};
pub use winit::event_loop::EventLoopClosed;

mod event_loop;
mod pixel;

use std::{error::Error, fmt};

use blit::{Ui, platform::Platform};
use blit_cpu::RendererConfig;
use winit::event_loop::EventLoopProxy as WinitEventLoopProxy;

pub struct Config {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub renderer: RendererConfig,
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
                event_loop::Event::TaskReady(_) => unreachable!(),
            })
    }
}

pub trait Application: Sized + 'static {
    type Input: Send + 'static;

    fn new(platform: Platform, input: EventLoopProxy<Self::Input>, root: Root<Self>) -> Self;

    fn input(&mut self, input: Self::Input);

    fn render(&mut self, ui: &mut Ui);
}

pub fn run<A: Application>(config: Config) -> Result<(), RunError> {
    event_loop::run::<A>(config)
}

#[derive(Debug)]
pub enum RunError {
    EventLoop(winit::error::EventLoopError),
    Window(winit::error::OsError),
    Surface(softbuffer::SoftBufferError),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(error) => error.fmt(formatter),
            Self::Window(error) => error.fmt(formatter),
            Self::Surface(error) => error.fmt(formatter),
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

impl From<softbuffer::SoftBufferError> for RunError {
    fn from(error: softbuffer::SoftBufferError) -> Self {
        Self::Surface(error)
    }
}

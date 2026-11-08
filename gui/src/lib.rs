pub mod atom;
pub mod color;
mod context;
pub mod display_list;
pub mod image;
pub mod shadow;
pub mod style;
pub mod text;
mod text_system;
pub mod widget;

pub use blit_text::{Family, FontData, FontError, TextLayoutEngine};

pub use self::{
    context::{BoundsClip, GuiContext, RenderInput},
    text_system::{FontStack, GlyphImageId, ResolvedTextLayout, TextConfig, TextSystem},
};

pub type Ui<'a, S = blit::state::Build> = blit::Ui<'a, GuiContext, S>;

pub trait Context: blit::Context<Scalar = f32> + AsMut<GuiContext> {}

impl<C: blit::Context<Scalar = f32> + AsMut<GuiContext>> Context for C {}

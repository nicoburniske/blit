pub mod atom;
pub mod color;
mod context;
pub mod display_list;
pub mod image;
pub mod style;
pub mod text;
mod text_system;
pub mod widget;

pub use blit_layout as layout;
pub use blit_text::{FontData, FontError, TextLayoutEngine};
pub use context::{BoundsClip, GuiContext, RenderInput};
pub use text_system::{FontFamily, ResolvedTextLayout, TextConfig, TextLayoutId, TextSystem};

pub type Ui<'a, S = blit::state::Build> = blit::Ui<'a, GuiContext, S>;

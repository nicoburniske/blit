//! low-level frame node construction

use crate::{image::ImageContent, style::Style, text::TextContent};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Content<'a> {
    Rectangle(Style<'a>),
    Text(TextContent),
    Image(ImageContent),
}

/// identifies a node only during the current render
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId {
    pub(crate) value: u32,
    #[cfg(debug_assertions)]
    pub(crate) generation: u16,
}

impl NodeId {
    pub(crate) fn index(self) -> usize {
        #[cfg(debug_assertions)]
        crate::frame::generation::assert(self.generation);
        self.value.checked_sub(1).expect("missing node") as usize
    }
}

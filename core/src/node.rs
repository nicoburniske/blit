//! low-level frame node construction

use crate::{image::ImageContent, style::Style, text::TextContent};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Content<'a> {
    Rectangle(Style<'a>),
    Text(TextContent),
    Image(ImageContent),
}

/// identifies a node only during the current render callback
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(u32);

impl NodeId {
    pub(crate) const ROOT: Self = Self(1);

    pub(crate) fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many nodes in one frame"))
    }

    pub(crate) fn index(self) -> usize {
        self.0.checked_sub(1).expect("missing node") as usize
    }
}

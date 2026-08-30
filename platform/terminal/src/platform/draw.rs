use blit::{Constraints, Leaf, LogicalRect, NodeId, Scale2, Size, Widget};
use blit_term::{
    color::Color,
    command_list::Block as DrawBlock,
    image::{ImageId, ImagePlacement},
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};

pub use blit_term::command_list::Border;

use super::TerminalPlatform;
use crate::Ui;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Block {
        new(),
        @optional {
            border: Border,
        },
        background: Color = Color::TRANSPARENT,
        opacity: f32 = 1.0,
    }
}

impl Widget<TerminalPlatform> for Block {
    type Response = NodeId;

    fn build(self, ui: &mut Ui) -> Self::Response {
        ui.add_leaf(self)
    }
}

impl Leaf<TerminalPlatform> for Block {
    fn measure(&self, _: &mut TerminalPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let mut block = DrawBlock::new(area)
            .background(self.background)
            .opacity(self.opacity);
        if let Some(border) = self.border {
            block = block.border(border);
        }
        let bounds = area.to_physical(Scale2::IDENTITY);
        let clip = platform.clip;
        platform.current.push_block(block, bounds, clip);
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextRun {
        new(text: TextRunId),
        color: Color = Color::WHITE,
        bold: bool = false,
        options: TextOptions = TextOptions::new(),
    }
}

impl Widget<TerminalPlatform> for TextRun {
    type Response = NodeId;

    fn build(self, ui: &mut Ui) -> Self::Response {
        ui.add_leaf(self)
    }
}

impl Leaf<TerminalPlatform> for TextRun {
    fn measure(&self, platform: &mut TerminalPlatform, constraints: Constraints) -> Size {
        let mut request = TextLayoutRequest::new(self.text).wrap(self.options.wrap);
        if self.options.wrap != TextWrap::None && constraints.max.width.is_finite() {
            request = request.max_width(constraints.max.width);
        }
        if let Some(max_lines) = self.options.max_lines {
            request = request.max_lines(max_lines);
        }
        constraints.constrain(platform.measure_text(&request))
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let request = TextRequest::new(self.text, area)
            .color(self.color)
            .bold(self.bold)
            .options(self.options);
        let bounds = area.to_physical(Scale2::IDENTITY);
        let clip = platform.clip;
        platform.current.push_text(request, bounds, clip);
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: Size),
    }
}

impl Widget<TerminalPlatform> for Image {
    type Response = NodeId;

    fn build(self, ui: &mut Ui) -> Self::Response {
        ui.add_leaf(self)
    }
}

impl Leaf<TerminalPlatform> for Image {
    fn measure(&self, _: &mut TerminalPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let bounds = area.to_physical(Scale2::IDENTITY);
        let clip = platform.clip;
        platform
            .current
            .push_image(ImagePlacement::new(self.image, area), bounds, clip);
    }
}

use blit::{Constraints, Leaf, LogicalRect, Size};
use blit_term::{
    color::Color,
    command_list::Block as DrawBlock,
    image::{ImageId, ImagePlacement},
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};

pub use blit_term::command_list::Border;

use crate::TerminalPlatform;

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
        let (commands, clip, scale) = platform.commands();
        commands.push_block(block, area.to_physical(scale), clip);
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        new(text: TextRunId),
        color: Color = Color::WHITE,
        bold: bool = false,
        options: TextOptions = TextOptions::new(),
    }
}

impl Leaf<TerminalPlatform> for Text {
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
        let (commands, clip, scale) = platform.commands();
        commands.push_text(request, area.to_physical(scale), clip);
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: Size),
    }
}

impl Leaf<TerminalPlatform> for Image {
    fn measure(&self, _: &mut TerminalPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let (commands, clip, scale) = platform.commands();
        commands.push_image(
            ImagePlacement::new(self.image, area),
            area.to_physical(scale),
            clip,
        );
    }
}

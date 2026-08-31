use blit::{Atom, Constraints, LogicalRect, Scale2, Size};
use blit_term::{
    color::Color,
    command_list::{Block as DrawBlock, BoxShadow as DrawShadow},
    image::{ImageId, ImagePlacement},
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};

pub use blit_term::command_list::{
    BlockTitle as Title, Border, BorderSides, BorderStyle, TitlePosition,
};

use super::TerminalPlatform;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Block {
        new(),
        @optional {
            border: Border,
            background: Color,
        },
        titles: [Option<Title>; 6] = [None; 6],
    }
}

impl Block {
    pub const fn title(mut self, title: Title) -> Self {
        self.titles[title.position.index()] = Some(title);
        self
    }
}

impl Atom<TerminalPlatform> for Block {
    fn measure(&self, _: &mut TerminalPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let mut block = DrawBlock::new(area).titles(self.titles);
        if let Some(background) = self.background {
            block = block.background(background);
        }
        if let Some(border) = self.border {
            block = block.border(border);
        }
        let bounds = area.to_physical(Scale2::IDENTITY);
        let clip = platform.clip;
        platform.current.push_block(block, bounds, clip);
    }

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
    }
}

impl Atom<TerminalPlatform> for Text {
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
            .attributes(self.attributes)
            .options(self.options);
        let bounds = area.to_physical(Scale2::IDENTITY);
        let clip = platform.clip;
        platform.current.push_text(request, bounds, clip);
    }

    fn measure_depends_on_constraints(&self) -> bool {
        self.options.wrap != TextWrap::None
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: Size),
    }
}

impl Atom<TerminalPlatform> for Image {
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

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Shadow {
        new(color: Color),
        offset_x: f32 = 1.0,
        offset_y: f32 = 1.0,
    }
}

impl Shadow {
    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}

impl Atom<TerminalPlatform> for Shadow {
    fn measure(&self, _: &mut TerminalPlatform, _: Constraints) -> Size {
        Size::ZERO
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let shifted = LogicalRect {
            x: area.x + self.offset_x,
            y: area.y + self.offset_y,
            ..area
        };
        let shadow = DrawShadow::new(area, self.color).offset(self.offset_x, self.offset_y);
        let bounds = shifted.to_physical(Scale2::IDENTITY);
        let clip = platform.clip;
        platform.current.push_shadow(shadow, bounds, clip);
    }

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::impl_atom_widgets!(TerminalPlatform => Block, Text, Image, Shadow);

use blit::{Atom, Constraints, LogicalRect, NodeId, Scale2, Size, Widget};
use blit_term::{
    color::Color,
    command_list::{Block as DrawBlock, BlockTitle, BoxShadow as DrawShadow},
    image::{ImageId, ImagePlacement},
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};

pub use blit_term::command_list::{Border, BorderSides, BorderStyle, TitlePosition};

use super::TerminalPlatform;
use crate::Cx;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Block<'a> {
        new(),
        @optional {
            border: Border,
            background: Color,
            shadow: Shadow,
        },
        titles: [Option<Title<'a>>; 6] = [None; 6],
    }
}

impl<'a> Block<'a> {
    pub const fn title(mut self, title: Title<'a>) -> Self {
        self.titles[title.position.index()] = Some(title);
        self
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Title<'a> {
        new(text: &'a str),
        @optional {
            color: Color,
        },
        attributes: TextAttributes = TextAttributes::NONE,
        position: TitlePosition = TitlePosition::TopLeft,
    }
}

impl Widget<TerminalPlatform> for Block<'_> {
    type Response = NodeId;

    fn build(self, mut cx: Cx<'_>) -> Self::Response {
        let color = self
            .border
            .map(|border| border.color)
            .unwrap_or(Color::Reset);
        let titles = self.titles.map(|title| {
            title.map(|title| {
                let text = cx.platform().text_run(title.text);
                BlockTitle::new(text)
                    .color(title.color.unwrap_or(color))
                    .attributes(title.attributes)
                    .position(title.position)
            })
        });
        if let Some(shadow) = self.shadow {
            cx.atom(shadow);
        }
        cx.atom(ResolvedBlock {
            border: self.border,
            background: self.background,
            titles,
        });
        cx.id()
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
}

#[derive(Clone, Copy)]
struct ResolvedBlock {
    border: Option<Border>,
    background: Option<Color>,
    titles: [Option<BlockTitle>; 6],
}

impl Atom<TerminalPlatform> for ResolvedBlock {
    fn measure(&self, _: &mut TerminalPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let mut block = DrawBlock::new(area);
        if let Some(background) = self.background {
            block = block.background(background);
        }
        if let Some(border) = self.border {
            block = block.border(border);
        }
        block = block.titles(self.titles);
        let bounds = area.to_physical(Scale2::IDENTITY);
        let clip = platform.clip;
        platform.current.push_block(block, bounds, clip);
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(crate) struct TextAtom {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
    }
}

impl Atom<TerminalPlatform> for TextAtom {
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
}

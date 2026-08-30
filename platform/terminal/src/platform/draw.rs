use blit::{Constraints, Leaf, LogicalRect, NodeId, Scale2, Size, Widget};
use blit_term::{
    color::Color,
    command_list::{Block as DrawBlock, BlockTitle},
    image::{ImageId, ImagePlacement},
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};

pub use blit_term::command_list::{Border, BorderSides, BorderStyle, TitlePosition};

use super::TerminalPlatform;
use crate::Ui;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Block<'a> {
        new(),
        @optional {
            border: Border,
            background: Color,
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

    fn build(self, ui: &mut Ui) -> Self::Response {
        let color = self
            .border
            .map(|border| border.color)
            .unwrap_or(Color::Reset);
        let titles = self.titles.map(|title| {
            title.map(|title| {
                let text = ui.platform().text_run(title.text);
                BlockTitle::new(text)
                    .color(title.color.unwrap_or(color))
                    .attributes(title.attributes)
                    .position(title.position)
            })
        });
        ui.add_leaf(ResolvedBlock {
            border: self.border,
            background: self.background,
            titles,
        })
    }
}

#[derive(Clone, Copy)]
struct ResolvedBlock {
    border: Option<Border>,
    background: Option<Color>,
    titles: [Option<BlockTitle>; 6],
}

impl Widget<TerminalPlatform> for ResolvedBlock {
    type Response = NodeId;

    fn build(self, ui: &mut Ui) -> Self::Response {
        ui.add_leaf(self)
    }
}

impl Leaf<TerminalPlatform> for ResolvedBlock {
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
    pub struct TextRun {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
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

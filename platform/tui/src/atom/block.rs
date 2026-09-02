use std::ops::{BitOr, BitOrAssign};

use blit::{Atom, Constraints, LogicalRect, Size};
use blit_tui_render::{
    cell::{Cell, CellStyle},
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId},
};

use crate::TuiPlatform;

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

impl Atom<TuiPlatform> for Block {
    fn measure(&self, _: &mut TuiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        let (width, height) = {
            let mut cells = platform.cells(area);
            let width = cells.columns();
            let height = cells.rows();
            if let Some(background) = self.background {
                cells.clear(Cell::default().style(CellStyle::new().background(background)));
            }
            if let Some(border) = self.border
                && width > 0
                && height > 0
            {
                let style = CellStyle::new().foreground(border.color);
                if border.sides.contains(BorderSides::TOP) {
                    for x in 0..width {
                        let edges = border_edges(border.sides, x, 0, width, height);
                        if edges != 0 {
                            cells.set_cell(
                                x,
                                0,
                                Cell::new(border_character(border.style, edges)).style(style),
                            );
                        }
                    }
                }
                if height > 1 && border.sides.contains(BorderSides::BOTTOM) {
                    for x in 0..width {
                        let edges = border_edges(border.sides, x, height - 1, width, height);
                        if edges != 0 {
                            cells.set_cell(
                                x,
                                height - 1,
                                Cell::new(border_character(border.style, edges)).style(style),
                            );
                        }
                    }
                }
                for y in 1..height.saturating_sub(1) {
                    if border.sides.contains(BorderSides::LEFT) {
                        let edges = border_edges(border.sides, 0, y, width, height);
                        if edges != 0 {
                            cells.set_cell(
                                0,
                                y,
                                Cell::new(border_character(border.style, edges)).style(style),
                            );
                        }
                    }
                    if width > 1 && border.sides.contains(BorderSides::RIGHT) {
                        let edges = border_edges(border.sides, width - 1, y, width, height);
                        if edges != 0 {
                            cells.set_cell(
                                width - 1,
                                y,
                                Cell::new(border_character(border.style, edges)).style(style),
                            );
                        }
                    }
                }
            }
            (width, height)
        };
        if height == 0 {
            return;
        }
        let left = usize::from(
            self.border
                .is_some_and(|border| border.sides.contains(BorderSides::LEFT)),
        );
        let right = width.saturating_sub(usize::from(
            self.border
                .is_some_and(|border| border.sides.contains(BorderSides::RIGHT)),
        ));
        let origin_x = area.x.round();
        let origin_y = area.y.round();
        paint_title_row(
            platform,
            [self.titles[0], self.titles[1], self.titles[2]],
            origin_x,
            origin_y,
            left,
            right,
        );
        paint_title_row(
            platform,
            [self.titles[3], self.titles[4], self.titles[5]],
            origin_x,
            origin_y + height.saturating_sub(1) as f32,
            left,
            right,
        );
    }

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Border {
        new(color: Color),
        style: BorderStyle = BorderStyle::Single,
        sides: BorderSides = BorderSides::ALL,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderStyle {
    #[default]
    Single,
    Rounded,
    Double,
    Heavy,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderSides(pub u8);

impl BorderSides {
    pub const NONE: Self = Self(0);
    pub const TOP: Self = Self(1 << 0);
    pub const RIGHT: Self = Self(1 << 1);
    pub const BOTTOM: Self = Self(1 << 2);
    pub const LEFT: Self = Self(1 << 3);
    pub const ALL: Self = Self(Self::TOP.0 | Self::RIGHT.0 | Self::BOTTOM.0 | Self::LEFT.0);

    pub const fn contains(self, sides: Self) -> bool {
        self.0 & sides.0 == sides.0
    }
}

impl BitOr for BorderSides {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for BorderSides {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Title {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        position: TitlePosition = TitlePosition::TopLeft,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitlePosition {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl TitlePosition {
    pub const fn index(self) -> usize {
        self as usize
    }
}

fn paint_title_row(
    platform: &mut TuiPlatform,
    titles: [Option<Title>; 3],
    origin_x: f32,
    y: f32,
    left: usize,
    right: usize,
) {
    let available = right.saturating_sub(left);
    if available == 0 {
        return;
    }
    let left_width = titles[0].map_or(0, |title| {
        platform
            .renderer_mut()
            .measure_text(&TextLayoutRequest::new(title.text).max_lines(1))
            .width as usize
    });
    let right_width = titles[2].map_or(0, |title| {
        platform
            .renderer_mut()
            .measure_text(&TextLayoutRequest::new(title.text).max_lines(1))
            .width as usize
    });
    let left_width = left_width.min(available.saturating_sub(right_width.min(available / 2)));
    let right_width = right_width.min(available - left_width);
    let right_start = right - right_width;
    if let Some(title) = titles[0] {
        paint_title(platform, title, origin_x, y, left, left_width);
    }
    if let Some(title) = titles[2] {
        paint_title(platform, title, origin_x, y, right_start, right_width);
    }
    if let Some(title) = titles[1] {
        let width = (platform
            .renderer_mut()
            .measure_text(&TextLayoutRequest::new(title.text).max_lines(1))
            .width as usize)
            .min(right_start.saturating_sub(left + left_width));
        let centered = left + (available - width) / 2;
        let start = centered.clamp(left + left_width, right_start - width);
        paint_title(platform, title, origin_x, y, start, width);
    }
}

fn paint_title(
    platform: &mut TuiPlatform,
    title: Title,
    origin_x: f32,
    y: f32,
    x: usize,
    width: usize,
) {
    if width == 0 {
        return;
    }
    platform.paint_text(
        TextRequest::new(
            title.text,
            LogicalRect::new(origin_x + x as f32, y, width as f32, 1.0),
        )
        .color(title.color)
        .attributes(title.attributes)
        .options(TextOptions::new().max_lines(1)),
    );
}

fn border_edges(sides: BorderSides, x: usize, y: usize, width: usize, height: usize) -> u8 {
    let mut edges = 0;
    if y == 0 && sides.contains(BorderSides::TOP) {
        edges |= u8::from(x != 0) * 8;
        edges |= u8::from(x + 1 != width) * 2;
    }
    if y + 1 == height && sides.contains(BorderSides::BOTTOM) {
        edges |= u8::from(x != 0) * 8;
        edges |= u8::from(x + 1 != width) * 2;
    }
    if x == 0 && sides.contains(BorderSides::LEFT) {
        edges |= u8::from(y != 0);
        edges |= u8::from(y + 1 != height) * 4;
    }
    if x + 1 == width && sides.contains(BorderSides::RIGHT) {
        edges |= u8::from(y != 0);
        edges |= u8::from(y + 1 != height) * 4;
    }
    edges
}

fn border_character(style: BorderStyle, edges: u8) -> char {
    const SINGLE: [char; 16] = [
        ' ', '╵', '╴', '└', '╷', '│', '┌', '├', '╶', '┘', '─', '┴', '┐', '┤', '┬', '┼',
    ];
    const DOUBLE: [char; 16] = [
        ' ', '╵', '╴', '╚', '╷', '║', '╔', '╠', '╶', '╝', '═', '╩', '╗', '╣', '╦', '╬',
    ];
    const HEAVY: [char; 16] = [
        ' ', '╹', '╺', '┗', '╻', '┃', '┏', '┣', '╸', '┛', '━', '┻', '┓', '┫', '┳', '╋',
    ];
    match style {
        BorderStyle::Rounded => match edges {
            3 => '╰',
            6 => '╭',
            9 => '╯',
            12 => '╮',
            _ => SINGLE[edges as usize],
        },
        BorderStyle::Single => SINGLE[edges as usize],
        BorderStyle::Double => DOUBLE[edges as usize],
        BorderStyle::Heavy => HEAVY[edges as usize],
    }
}

blit::impl_atom_widgets!(TuiPlatform => Block);

#[cfg(test)]
mod tests {
    use blit::{Frame, FrameInfo, Place, Size};
    use blit_std::layout::Single;
    use blit_tui_render::{RendererConfig, TuiRenderer};

    use super::*;
    use crate::{atom::Shadow, widget};

    #[test]
    fn block_and_shadow_paint_as_atoms() {
        let renderer = TuiRenderer::new(RendererConfig::new().columns(20).rows(5));
        let mut platform = TuiPlatform::new(renderer);
        let mut frame = Frame::default();
        frame.render(
            &mut platform,
            FrameInfo::new(Size::new(20.0, 5.0)),
            |ui: crate::Ui<'_>| {
                let mut root = ui.layout(Single::new());
                root.insert(widget::Text::new(
                    "xxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx",
                ));
                root.child().place(Place::fixed(20.0, 4.0)).insert(
                    widget::Block::new()
                        .background(Color::WHITE)
                        .border(Border::new(Color::GREEN).style(BorderStyle::Double))
                        .shadow(Shadow::new(Color::BLACK))
                        .title(
                            widget::Title::new("title").position(TitlePosition::TopCenter),
                        )
                        .title(
                            widget::Title::new("status").position(TitlePosition::BottomRight),
                        ),
                );
            },
        );

        assert_eq!(
            platform.renderer().plain_text(),
            "╔══════title═══════╗\n║                  ║\n║                  ║\n╚════════════status╝\nx\n"
        );
    }
}

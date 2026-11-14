use std::ops::{BitOr, BitOrAssign};

use blit::{Atom, AtomCx, Constraints, Paint, PhysicalRect, Size, Widget, state};

use crate::{
    Context, Shadow,
    cell::{Cell, CellStyle},
    color::Color,
    text::{FrameText, TextAttributes, TextLayoutRequest, TextOptions, TextRun, TextWrap},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Block<'a> {
        new(),
        #[into]
        border: Option<Border> = None,
        #[into]
        background: Option<Color> = None,
        #[into]
        shadow: Option<Shadow> = None,
        titles: [Option<Title<'a>>; 6] = [None; 6],
    }
}

impl<'a> Block<'a> {
    pub fn title(mut self, title: Title<'a>) -> Self {
        self.titles[title.position.index()] = Some(title);
        self
    }
}

impl<C> Widget<C, state::Node> for Block<'_>
where
    C: Context + Paint<FrameText>,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let color = self.border.map(|border| border.color).unwrap_or(Color::Reset);
        let titles = self.titles.map(|title| {
            title.map(|title| {
                let text = ui.context_mut().as_mut().renderer.text_run(title.text);
                TitleRun {
                    text,
                    color: title.color.unwrap_or(color),
                    attributes: title.attributes,
                }
            })
        });
        if let Some(shadow) = self.shadow {
            ui.insert(shadow);
        }
        ui.insert(BlockAtom {
            border: self.border,
            background: self.background,
            titles,
        });
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Title<'a> {
        new(text: &'a str),
        #[into]
        color: Option<Color> = None,
        attributes: TextAttributes = TextAttributes::NONE,
        position: TitlePosition = TitlePosition::TopLeft,
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

struct BlockAtom {
    border: Option<Border>,
    background: Option<Color>,
    titles: [Option<TitleRun>; 6],
}

#[derive(Clone, Copy)]
struct TitleRun {
    text: TextRun,
    color: Color,
    attributes: TextAttributes,
}

impl<C> Atom<C> for BlockAtom
where
    C: Context + Paint<FrameText>,
{
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        let context = cx.context_mut();
        let (width, height) = {
            let mut cells = context.as_mut().renderer.cells(area);
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
                            cells.set_cell(x, 0, Cell::new(border_character(border.style, edges)).style(style));
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
                            cells.set_cell(0, y, Cell::new(border_character(border.style, edges)).style(style));
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
        paint_title_row(
            context,
            [self.titles[0], self.titles[1], self.titles[2]],
            area.x,
            area.y,
            left,
            right,
        );
        paint_title_row(
            context,
            [self.titles[3], self.titles[4], self.titles[5]],
            area.x,
            area.y + height.saturating_sub(1) as i32,
            left,
            right,
        );
    }
}

fn paint_title_row(
    context: &mut (impl Context + Paint<FrameText>),
    titles: [Option<TitleRun>; 3],
    origin_x: i32,
    y: i32,
    left: usize,
    right: usize,
) {
    let available = right.saturating_sub(left);
    if available == 0 {
        return;
    }
    let mut measure = |title: TitleRun| {
        let request = TextLayoutRequest {
            text: title.text,
            wrap: TextWrap::None,
            max_width: None,
            max_lines: Some(1),
        };
        context.as_mut().renderer.measure_text(&request).width as usize
    };
    let left_width = titles[0].map_or(0, &mut measure);
    let right_width = titles[2].map_or(0, &mut measure);
    let center_width = titles[1].map_or(0, &mut measure);
    let left_width = left_width.min(available.saturating_sub(right_width.min(available / 2)));
    let right_width = right_width.min(available - left_width);
    let right_start = right - right_width;
    if let Some(title) = titles[0] {
        paint_title(context, title, origin_x, y, left, left_width);
    }
    if let Some(title) = titles[2] {
        paint_title(context, title, origin_x, y, right_start, right_width);
    }
    if let Some(title) = titles[1] {
        let width = center_width.min(right_start.saturating_sub(left + left_width));
        let centered = left + (available - width) / 2;
        let start = centered.clamp(left + left_width, right_start - width);
        paint_title(context, title, origin_x, y, start, width);
    }
}

fn paint_title(
    context: &mut impl Paint<FrameText, Scalar = i32>,
    title: TitleRun,
    origin_x: i32,
    y: i32,
    x: usize,
    width: usize,
) {
    if width == 0 {
        return;
    }
    let text = FrameText::new(title.text)
        .color(title.color)
        .attributes(title.attributes)
        .options(TextOptions::new().max_lines(1));
    context.paint(PhysicalRect::new(origin_x + x as i32, y, width as i32, 1), text);
}

fn border_edges(sides: BorderSides, x: usize, y: usize, width: usize, height: usize) -> u8 {
    let mut edges = 0;
    if y == 0 && sides.contains(BorderSides::TOP) {
        edges |= u8::from(x != 0 || !sides.contains(BorderSides::LEFT)) * 8;
        edges |= u8::from(x + 1 != width || !sides.contains(BorderSides::RIGHT)) * 2;
    }
    if y + 1 == height && sides.contains(BorderSides::BOTTOM) {
        edges |= u8::from(x != 0 || !sides.contains(BorderSides::LEFT)) * 8;
        edges |= u8::from(x + 1 != width || !sides.contains(BorderSides::RIGHT)) * 2;
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Constraints, Frame, FrameInfo, Input, Size};

    use super::*;
    use crate::{RendererConfig, Shadow, TuiContext, TuiRenderer, layout::flex};

    #[test]
    fn block_and_shadow_paint_as_atoms() {
        let renderer = TuiRenderer::new(RendererConfig::new().columns(20).rows(5));
        let mut context = TuiContext::new(renderer);
        let mut frame = Frame::default();
        context.renderer.begin_frame();
        frame.build(
            &mut context,
            FrameInfo::new(Size::new(20, 5)),
            Duration::ZERO,
            Input::None,
            |ui: crate::Ui<'_>| {
                let mut root = ui.layout(flex::column());
                root.insert(crate::Text::new(
                    "xxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx\nxxxxxxxxxxxxxxxxxxxx",
                ));
                root.child().constrain(Constraints::tight(Size::new(20, 4))).insert(
                    crate::Block::new()
                        .background(Color::WHITE)
                        .border(Border::new(Color::GREEN).style(BorderStyle::Double))
                        .shadow(Shadow::new(Color::BLACK))
                        .title(
                            crate::Title::new("title").position(TitlePosition::TopCenter),
                        )
                        .title(
                            crate::Title::new("status").position(TitlePosition::BottomRight),
                        ),
                );
            },
        );
        frame.layout(&mut context);
        frame.paint(&mut context);
        context.renderer.end_frame();

        assert_eq!(
            context.renderer.plain_text(),
            "╔══════title═══════╗\n║                  ║\n║                  ║\n╚════════════status╝\nx\n"
        );
    }
}

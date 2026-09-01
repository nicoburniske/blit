use blit::Widget;
use blit_tui_render::{color::Color, text::TextAttributes};

use crate::{
    Cx, TuiPlatform,
    atom::{self, Border, Shadow, TitlePosition},
};

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

impl Widget<TuiPlatform> for Block<'_> {
    type Response = ();

    fn build(self, mut cx: Cx<'_>) {
        let color = self
            .border
            .map(|border| border.color)
            .unwrap_or(Color::Reset);
        let titles = self.titles.map(|title| {
            title.map(|title| {
                let text = cx.platform().renderer_mut().text_run(title.text);
                atom::Title::new(text)
                    .color(title.color.unwrap_or(color))
                    .attributes(title.attributes)
                    .position(title.position)
            })
        });
        if let Some(shadow) = self.shadow {
            cx.atom(shadow);
        }
        cx.atom(atom::Block {
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
        @optional {
            color: Color,
        },
        attributes: TextAttributes = TextAttributes::NONE,
        position: TitlePosition = TitlePosition::TopLeft,
    }
}

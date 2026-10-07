use std::time::Duration;

use blit::{Frame, FrameInfo, Input, Size, WidgetId};
use blit_tui::{
    RendererConfig, TuiContext, TuiRenderer, Ui,
    layout::{Sizing, flex, grid, single},
    text::{TextOptions, TextWrap},
    widget::Text,
};

#[test]
fn three_columns_use_all_ten_cells_and_wrap_at_the_assigned_width() {
    for use_grid in [false, true] {
        let mut context = TuiContext::new(TuiRenderer::new(RendererConfig::new().columns(10).rows(2)));
        let mut frame = Frame::default();
        frame.build(
            &mut context,
            FrameInfo::new(Size::new(10, 2)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_>| {
                ui.layout(single::new().fixed(10, 2)).child().build(|ui: Ui<'_>| {
                    if use_grid {
                        let mut row = ui.layout(grid::new(3));
                        for column in 0..3 {
                            row.child()
                                .widget_id(WidgetId::new(column))
                                .insert(Text::new("abcd").options(TextOptions::new().wrap(TextWrap::Character)));
                        }
                    } else {
                        let mut row = ui.layout(flex::row());
                        for column in 0..3 {
                            row.child()
                                .widget_id(WidgetId::new(column))
                                .item(flex::item().width(Sizing::grow()))
                                .insert(Text::new("abcd").options(TextOptions::new().wrap(TextWrap::Character)));
                        }
                    }
                });
            },
        );
        frame.layout(&mut context);
        assert_eq!(
            [0, 1, 2].map(|column| frame.geometry(WidgetId::new(column)).unwrap().width),
            [3, 4, 3]
        );
        context.begin_paint();
        frame.paint(&mut context);
        context.finish_paint();
        assert_eq!(context.renderer().plain_text(), "abcabcdabc\nd      d\n");
    }
}

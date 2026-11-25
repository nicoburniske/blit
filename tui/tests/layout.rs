use std::time::Duration;

use blit::{Constraints, FrameInfo, Id, Kernel, Key, Size, input::Input};
use blit_tui::{
    RendererConfig, TuiContext, TuiRenderer, Ui,
    layout::{Sizing, flex, grid},
    text::{Text, TextOptions, TextWrap},
};

#[test]
fn three_columns_use_all_ten_cells_and_wrap_at_the_assigned_width() {
    for use_grid in [false, true] {
        let mut context = TuiContext::new(TuiRenderer::new(RendererConfig::new().columns(10).rows(2)));
        let mut kernel = Kernel::default();
        kernel.build(
            &mut context,
            FrameInfo::new(Size::new(10, 2)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_>| {
                ui.constrain(Constraints::tight(Size::new(10, 2)))
                    .layout(())
                    .child()
                    .build(|ui: Ui<'_>| {
                        if use_grid {
                            let mut row = ui.layout(grid::new(3));
                            for column in 0..3 {
                                row.child_in(Id::ROOT, Key::from(column))
                                    .insert(Text::new("abcd").options(TextOptions::new().wrap(TextWrap::Character)));
                            }
                        } else {
                            let mut row = ui.layout(flex::row());
                            for column in 0..3 {
                                row.child_in(Id::ROOT, Key::from(column))
                                    .item(flex::item().width(Sizing::grow()))
                                    .insert(Text::new("abcd").options(TextOptions::new().wrap(TextWrap::Character)));
                            }
                        }
                    });
            },
        );
        kernel.layout(&mut context);
        assert_eq!(
            [0, 1, 2].map(|column| kernel.area(Id::ROOT.child(Key::from(column))).unwrap().width),
            [3, 4, 3]
        );
        context.renderer.begin_frame();
        kernel.paint(&mut context);
        context.renderer.end_frame();
        assert_eq!(context.renderer.plain_text(), "abcabcdabc\nd      d\n");
    }
}

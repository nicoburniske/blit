use super::*;

#[test]
fn transitions() {
    for named in [false, true] {
        let (mut frame, mut context) = frame(Size::new(1.0, 5.0));
        for (extent, gap, millis, area, sibling, pending) in [
            (1.0, 0.0, 0, Rect::new(0.0, 0.0, 1.0, 1.0), Some(1.0), false),
            (3.0, 2.0, 0, Rect::new(0.0, 0.0, 1.0, 1.0), Some(3.0), true),
            (3.0, 2.0, 500, Rect::new(0.0, 1.0, 1.0, 2.0), Some(4.0), true),
            (3.0, 2.0, 1000, Rect::new(0.0, 2.0, 1.0, 3.0), None, false),
        ] {
            render_inputs(
                &mut frame,
                &mut context,
                Duration::from_millis(millis),
                [],
                |ui: Ui<'_>| {
                    let mut column = ui.layout(Column);
                    let mut child = column
                        .child()
                        .item(gap)
                        .transition(Transition::new(Duration::from_secs(1)).height().y());
                    if named {
                        child = child.widget_id(WidgetId::new("transition"));
                    }
                    child.layout(()).child().insert(Fill('X', Size::new(1.0, extent)));
                    column.child().insert(Fill('Y', Size::uniform(1.0)));
                },
            );
            if let Some(y) = sibling {
                assert_eq!(
                    context.commands,
                    [Paint('X', area), Paint('Y', Rect::new(0.0, y, 1.0, 1.0))]
                );
            } else {
                assert_eq!(context.commands, [Paint('X', area)]);
            }
            assert_eq!(frame.has_pending_redraw(), pending);
        }
    }
}

#[test]
fn animation_and_timers() {
    let (mut frame, mut context) = frame(Size::uniform(1.0));
    let animation = WidgetId::new("animation");
    let timer = WidgetId::new("timer");
    for (target, millis, expected, fired) in [(0.0, 0, 0.0, false), (1.0, 0, 0.0, false), (1.0, 500, 0.5, true)] {
        let result = render_inputs(
            &mut frame,
            &mut context,
            Duration::from_millis(millis),
            [],
            |mut ui: Ui<'_>| {
                (
                    ui.animate(animation, target, Duration::from_secs(1), Easing::Linear),
                    ui.timer(timer, Duration::from_millis(500)),
                )
            },
        );
        assert_eq!(result, (expected, fired));
    }
    assert!(frame.has_pending_redraw());
}

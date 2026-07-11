use crate::*;

struct TestPlatform;

impl PlatformImpl for TestPlatform {
    fn screen(&mut self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }
    }

    fn draw_rectangle(&mut self, _: &widgets::Rectangle, _: &[PhysicalRect]) {}

    fn draw_image(&mut self, _: &widgets::Image<'_>, _: &[PhysicalRect]) {}

    fn draw_text(&mut self, _: &TextRequest<'_>, _: &[PhysicalRect]) {}
}

#[test]
fn invalidation_is_rendered_next_frame() {
    let screen = PhysicalRect {
        x: 0,
        y: 0,
        width: 10,
        height: 10,
    };
    let changed = LogicalRect {
        x: 2.0,
        y: 3.0,
        width: 4.0,
        height: 5.0,
    };
    let mut platform = TestPlatform;
    // safety: platform outlives the runtime
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });

    let damage = runtime.render(Input::None, |ui| {
        ui.invalidate(changed);
        ui.dirty.clone()
    });
    assert_eq!(damage.regions(), &[screen]);
    assert!(runtime.has_pending_redraw());

    let damage = runtime.render(Input::None, |ui| ui.dirty.clone());
    assert_eq!(damage.regions(), &[screen]);
    assert!(runtime.has_pending_redraw());

    let damage = runtime.render(Input::None, |ui| ui.dirty.clone());
    assert_eq!(damage.regions(), &[changed.to_physical(1.0)]);
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn dirty_regions_merge_and_remain_bounded() {
    let mut dirty = DirtyRegions::default();
    dirty.add(PhysicalRect {
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    });
    dirty.add(PhysicalRect {
        x: 4,
        y: 0,
        width: 4,
        height: 4,
    });
    assert_eq!(
        dirty.regions(),
        &[PhysicalRect {
            x: 0,
            y: 0,
            width: 8,
            height: 4,
        }]
    );

    for index in 0..20 {
        dirty.add(PhysicalRect {
            x: index * 10,
            y: 10,
            width: 1,
            height: 1,
        });
    }
    assert_eq!(dirty.regions().len(), 8);
}

#[test]
fn logical_rect_rounds_outward_to_pixels() {
    assert_eq!(
        LogicalRect {
            x: 1.2,
            y: 2.8,
            width: 3.1,
            height: 4.1,
        }
        .to_physical(1.0),
        PhysicalRect {
            x: 1,
            y: 2,
            width: 4,
            height: 5,
        }
    );
}

#[test]
fn logical_rect_can_be_inset_by_axis() {
    assert_eq!(
        LogicalRect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        }
        .inset_x(10.0)
        .inset_y(5.0),
        LogicalRect {
            x: 20.0,
            y: 25.0,
            width: 80.0,
            height: 70.0,
        }
    );
}

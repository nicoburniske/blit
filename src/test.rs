use std::ptr::NonNull;

use crate::*;

unsafe fn measure(_: NonNull<()>, request: &TextRequest<'_>) -> TextMetrics {
    let character_width = request.style.size / 2.0;
    let natural_width = request.text.chars().count() as f32 * character_width;
    let mut lines = if request.options.wrap == TextWrap::None || request.area.width <= 0.0 {
        1
    } else {
        (natural_width / request.area.width).ceil().max(1.0) as u16
    };
    if let Some(max_lines) = request.options.max_lines {
        lines = lines.min(max_lines);
    }
    TextMetrics {
        width: natural_width.min(request.area.width),
        height: (request.style.size * lines as f32).min(request.area.height),
        baseline: request.style.size * 0.8,
        lines,
    }
}

unsafe fn draw(_: NonNull<()>, _: &mut PixmapMut<'_>, _: &TextRequest<'_>, _: &[Rect]) {}

fn platform() -> Platform {
    static VTABLE: PlatformVTable = PlatformVTable {
        measure_text: measure,
        draw_text: draw,
    };
    // safety: the backend has no state
    unsafe { Platform::new(NonNull::dangling(), &VTABLE) }
}

#[test]
fn invalidation_is_rendered_next_frame() {
    let screen = Rect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    let changed = Rect {
        x: 2.0,
        y: 3.0,
        width: 4.0,
        height: 5.0,
    };
    let mut runtime = Runtime::new(Pixmap::new(10, 10).unwrap(), platform());

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
    assert_eq!(damage.regions(), &[changed]);
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn dirty_regions_merge_and_remain_bounded() {
    let mut dirty = DirtyRegions::default();
    dirty.add(Rect {
        x: 0.0,
        y: 0.0,
        width: 4.0,
        height: 4.0,
    });
    dirty.add(Rect {
        x: 4.0,
        y: 0.0,
        width: 4.0,
        height: 4.0,
    });
    assert_eq!(
        dirty.regions(),
        &[Rect {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 4.0,
        }]
    );

    for index in 0..20 {
        dirty.add(Rect {
            x: index as f32 * 10.0,
            y: 10.0,
            width: 1.0,
            height: 1.0,
        });
    }
    assert_eq!(dirty.regions().len(), 8);
}

mod prepared;
mod rounded;

pub use prepared::Prepared;

use blit::{PhysicalRect, widgets::Rectangle};

use crate::{PixelBuffer, PixelSpan};

pub fn draw<B: PixelBuffer>(
    buffer: &mut B,
    rectangle: &Rectangle,
    clips: &[PhysicalRect],
    scale_factor: f32,
) {
    let Some(rectangle) = Prepared::new(rectangle, scale_factor) else {
        return;
    };
    let screen = PhysicalRect {
        x: 0,
        y: 0,
        width: buffer.width() as i32,
        height: buffer.height() as i32,
    };
    for clip in clips {
        let Some(clipped) = rectangle
            .geometry
            .intersection(*clip)
            .and_then(|area| area.intersection(screen))
        else {
            continue;
        };
        for y in clipped.y..clipped.y + clipped.height {
            let row = buffer.line_mut(y as usize);
            rectangle.draw_line(y, *clip, PixelSpan { x: 0, pixels: row });
        }
    }
}

#[cfg(test)]
mod tests {
    use blit::{Color, LogicalRect, widgets::BorderRadius};

    use super::*;
    use crate::VecBuffer;

    #[test]
    fn rounded_edges_have_partial_coverage() {
        let mut buffer = VecBuffer::<u32>::new(16, 16);
        draw(
            &mut buffer,
            &Rectangle {
                area: LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 16.0,
                    height: 16.0,
                },
                background: Color::WHITE,
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                radius: BorderRadius {
                    top_left: 8.0,
                    top_right: 8.0,
                    bottom_right: 8.0,
                    bottom_left: 8.0,
                },
                opacity: 1.0,
            },
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0);
        assert_ne!(buffer.pixels()[7], 0);
        assert_ne!(buffer.pixels()[7], 0x00ff_ffff);
        assert_eq!(buffer.pixels()[8 * 16 + 8], 0x00ff_ffff);
    }

    #[test]
    fn clipping_does_not_touch_other_pixels() {
        let mut buffer = VecBuffer::<u32>::new(8, 8);
        draw(
            &mut buffer,
            &Rectangle::new(LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 8.0,
            })
            .background(Color::WHITE),
            &[PhysicalRect {
                x: 2,
                y: 3,
                width: 2,
                height: 1,
            }],
            1.0,
        );

        assert_eq!(
            buffer.pixels().iter().filter(|pixel| **pixel != 0).count(),
            2
        );
    }

    #[test]
    fn transparent_rectangle_is_not_prepared() {
        assert!(
            Prepared::new(
                &Rectangle::new(LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 8.0,
                    height: 8.0,
                }),
                1.0,
            )
            .is_none()
        );
    }

    #[test]
    fn corner_radii_are_independent() {
        let mut buffer = VecBuffer::<u32>::new(12, 12);
        draw(
            &mut buffer,
            &Rectangle {
                area: LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 12.0,
                    height: 12.0,
                },
                background: Color::WHITE,
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                radius: BorderRadius {
                    top_left: 6.0,
                    top_right: 0.0,
                    bottom_right: 0.0,
                    bottom_left: 0.0,
                },
                opacity: 1.0,
            },
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 12,
                height: 12,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0);
        assert_eq!(buffer.pixels()[11], 0x00ff_ffff);
        assert_eq!(buffer.pixels()[11 * 12], 0x00ff_ffff);
        assert_eq!(buffer.pixels()[12 * 12 - 1], 0x00ff_ffff);
    }

    #[test]
    fn rounded_border_keeps_separate_border_and_inner_spans() {
        let mut buffer = VecBuffer::<u32>::new(16, 16);
        draw(
            &mut buffer,
            &Rectangle::new(LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 16.0,
                height: 16.0,
            })
            .background(Color::from_rgba8(0, 255, 0, 255))
            .border(2.0, Color::from_rgba8(255, 0, 0, 255))
            .uniform_radius(6.0),
            &[PhysicalRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[8], 0x00ff_0000);
        assert_eq!(buffer.pixels()[8 * 16 + 8], 0x0000_ff00);
    }
}

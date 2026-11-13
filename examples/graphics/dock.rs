use super::*;

pub struct Dock {
    pub selected: usize,
    pub phase: f32,
    pub running: bool,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            selected: 0,
            phase: 0.0,
            running: true,
        }
    }
}

impl Dock {
    pub fn build<W: Widget<DesktopContext>>(
        &mut self,
        mut ui: Ui<'_>,
        tabs: &[(&str, usize)],
        controls: impl FnOnce(usize) -> Option<W>,
    ) {
        if self.running {
            self.phase = ui.animate_loop(
                Id::new("graphics laboratory"),
                Tween::new(Duration::from_secs(24), Easing::LINEAR),
            );
        }
        let mut toolbar = ui
            .id(Id::new("graphics dock"))
            .z_index(1)
            .place(
                blit::NodeTarget::Parent,
                absolute::place()
                    .target_anchor(Anchor::Bottom)
                    .child_anchor(Anchor::Bottom)
                    .y(-sz::LG),
            )
            .layout(flex::row().gap(sz::SM).align(Align::Center))
            .motion(Motion::new(Tween::new(Duration::from_millis(220), Easing::EASE_IN_OUT)).size());
        toolbar.interact(interact::press());
        toolbar.insert(
            blit_gui::shadow::Shadow::new(Color::rgba(0, 0, 0, 88))
                .radius(BorderRadius::uniform(16.0))
                .offset(0.0, 3.0)
                .blur(12.0),
        );
        toolbar.insert(
            Rectangle::new()
                .background(LinearGradient::new(&colors::BUTTON).angle(90.0))
                .border(Border::gradient(1.0, LinearGradient::new(&colors::RIM).angle(90.0)))
                .radius(BorderRadius::uniform(16.0)),
        );
        toolbar.insert(Pad::new(sz::XS));
        toolbar
            .child()
            .item(flex::item().fixed(40.0, 40.0))
            .build(|mut ui: Ui<'_>| {
                let press = ui.interact(interact::press());
                if press.clicked {
                    self.running = !self.running;
                }
                let stops = if self.running {
                    [
                        GradientStop::new(0.0, Color::rgb(218, 76, 87)),
                        GradientStop::new(0.5, Color::rgb(183, 43, 61)),
                        GradientStop::new(1.0, Color::rgb(125, 28, 51)),
                    ]
                } else {
                    [
                        GradientStop::new(0.0, Color::rgb(37, 151, 99)),
                        GradientStop::new(0.5, Color::rgb(20, 118, 78)),
                        GradientStop::new(1.0, Color::rgb(13, 77, 64)),
                    ]
                };
                let mut button = ui.layout(());
                button.insert(
                    Rectangle::new()
                        .background(LinearGradient::new(&stops).angle(if press.active { 225.0 } else { 45.0 }))
                        .border(Border::solid(
                            1.0,
                            if press.hovered { Color::WHITE } else { stops[0].color },
                        ))
                        .radius(BorderRadius::uniform(10.0)),
                );
                button.insert(PlaybackIcon {
                    running: self.running,
                    pressed: press.active,
                });
            });
        toolbar
            .child()
            .item(flex::item().fixed(1.0, 24.0))
            .insert(Rectangle::new().background(LinearGradient::new(&colors::RIM).angle(90.0)));
        toolbar
            .child()
            .build(select(&mut self.selected, tabs, flex::row().gap(sz::XXS), draw_button));
        if let Some(controls) = controls(self.selected) {
            toolbar
                .child()
                .item(flex::item().fixed(1.0, 24.0))
                .insert(Rectangle::new().background(LinearGradient::new(&colors::RIM).angle(90.0)));
            toolbar.child().build(controls);
        }
    }
}

struct PlaybackIcon {
    running: bool,
    pressed: bool,
}

impl Atom<DesktopContext> for PlaybackIcon {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        let x = area.x + area.width * 0.5;
        let y = area.y + area.height * 0.5 + f32::from(self.pressed);
        let context = cx.context_mut();
        if self.running {
            for offset in [-7.0, 1.0] {
                Rectangle::new()
                    .background(Color::WHITE)
                    .radius(BorderRadius::uniform(1.5))
                    .paint(LogicalRect::new(x + offset, y - 8.0, 6.0, 16.0), context);
            }
        } else {
            let vertices = [
                MeshVertex::new(x - 5.0, y - 8.0, Color::WHITE),
                MeshVertex::new(x + 8.0, y, Color::WHITE),
                MeshVertex::new(x - 5.0, y + 8.0, Color::WHITE),
            ];
            Mesh {
                vertices: &vertices,
                indices: &[0, 1, 2],
            }
            .paint(area, context);
        }
    }
}

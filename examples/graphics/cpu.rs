use super::*;

#[path = "paths.rs"]
mod paths;
#[path = "ribbons.rs"]
mod ribbons;

#[derive(Default)]
pub struct View {
    dock: dock::Dock,
    ribbons: ribbons::View,
    paths: paths::View,
    knot: knot::View,
    tiles: tiles::View,
}

impl Widget<DesktopContext> for &mut View {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let mut stage = ui.layout(()).clip(BoundsClip);
        stage.insert(Rectangle::new().background(colors::TRACK));
        stage.child().build(|ui: Ui<'_>| {
            self.dock.build(
                ui,
                &[
                    (ribbons::View::TITLE, 0),
                    (paths::View::TITLE, 1),
                    (knot::View::TITLE, 2),
                    (tiles::View::TITLE, 3),
                ],
                |tab| {
                    let ribbons = &mut self.ribbons;
                    (tab == 0).then_some(move |ui: Ui<'_>| ribbons.controls(ui))
                },
            );
        });
        if stage.child().build(|ui: Ui<'_>| match self.dock.selected {
            0 => self.ribbons.build(ui, self.dock.phase),
            1 => self.paths.build(ui, self.dock.phase),
            2 => self.knot.build(ui, self.dock.phase),
            _ => self.tiles.build(ui, self.dock.phase),
        }) {
            self.dock.running = false;
        }
        stage.insert(Rectangle::new().border(Border::solid(1.0, colors::BORDER)));
    }
}

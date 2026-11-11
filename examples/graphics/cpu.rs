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
    terrain: terrain::View,
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
                    (terrain::View::TITLE, 2),
                ],
                |tab| {
                    let ribbons = &mut self.ribbons;
                    let terrain = &mut self.terrain;
                    (tab != 1).then_some(move |ui: Ui<'_>| match tab {
                        0 => ribbons.controls(ui),
                        _ => terrain.controls(ui),
                    })
                },
            );
        });
        if stage.child().build(|ui: Ui<'_>| match self.dock.selected {
            0 => self.ribbons.build(ui, self.dock.phase),
            1 => self.paths.build(ui, self.dock.phase),
            _ => self.terrain.build(ui, self.dock.phase),
        }) {
            self.dock.running = false;
        }
        stage.insert(Rectangle::new().border(Border::solid(1.0, colors::BORDER)));
    }
}

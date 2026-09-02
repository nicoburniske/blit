use blit::{
    Axis, Constraints, Interaction, Layout, LayoutCx, Platform, Point, Sense, Size, Ui, Widget,
    WidgetId,
};

blit::builder! {
    /// split behavior + geometry
    #[derive(Clone, Copy, Debug)]
    pub struct Config {
        new(),
        axis: Axis = Axis::Horizontal,
        divider_extent: f32 = 1.0,
        sense: Sense = Sense::DRAG,
    }
}

pub trait Divider {
    type Widget;

    fn config(&self) -> Config {
        Config::default()
    }

    fn into_widget(self, axis: Axis, interaction: Interaction) -> Self::Widget;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoDivider;

impl Divider for NoDivider {
    type Widget = ();

    fn into_widget(self, _: Axis, _: Interaction) -> Self::Widget {
        ()
    }
}

#[derive(Debug, Default)]
pub struct State {
    extent: Option<f32>,
    changed: bool,
}

impl State {
    pub fn extent(&self) -> Option<f32> {
        self.extent
    }

    pub fn set_extent(&mut self, extent: f32) {
        self.extent = Some(extent.max(0.0));
        self.changed = true;
    }

    pub fn reset(&mut self) {
        self.extent = None;
        self.changed = true;
    }
}

pub struct Pane<'a, L, T, D = NoDivider> {
    state: &'a mut State,
    id: WidgetId,
    initial_extent: f32,
    minimum_leading: f32,
    minimum_trailing: f32,
    leading: L,
    trailing: T,
    divider: D,
    config: Config,
}

impl<'a, L, T, D> Pane<'a, L, T, D>
where
    D: Default + Divider,
{
    pub fn new(
        state: &'a mut State,
        id: WidgetId,
        initial_extent: f32,
        leading: L,
        trailing: T,
    ) -> Self {
        let divider = D::default();
        let config = divider.config();
        Self {
            state,
            id,
            initial_extent,
            minimum_leading: 0.0,
            minimum_trailing: 0.0,
            leading,
            trailing,
            divider,
            config,
        }
    }
}

impl<L, T, D> Pane<'_, L, T, D> {
    pub fn minimum_leading(mut self, extent: f32) -> Self {
        self.minimum_leading = extent;
        self
    }

    pub fn minimum_trailing(mut self, extent: f32) -> Self {
        self.minimum_trailing = extent;
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
}

impl<R, L, T, D> Widget<R> for Pane<'_, L, T, D>
where
    R: Platform,
    L: Widget<R>,
    T: Widget<R>,
    D: Divider,
    D::Widget: Widget<R>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, R>) {
        let Self {
            state,
            id,
            initial_extent,
            minimum_leading,
            minimum_trailing,
            leading,
            trailing,
            divider,
            config,
        } = self;
        let leading_id = id.child("leading pane");
        let divider_id = id.child("divider");
        let trailing_id = id.child("trailing pane");
        let interaction = ui.interact(divider_id, config.sense);
        let measured = ui.geometry(leading_id).map(|area| match config.axis {
            Axis::Horizontal => area.width,
            Axis::Vertical => area.height,
        });
        if !state.changed
            && let Some(measured) = measured
        {
            state.extent = Some(measured);
        }
        let delta = match config.axis {
            Axis::Horizontal => interaction.drag_delta.x,
            Axis::Vertical => interaction.drag_delta.y,
        };
        if delta != 0.0 {
            let extent = if state.changed {
                state.extent
            } else {
                measured.or(state.extent)
            }
            .unwrap_or(initial_extent);
            state.extent = Some(extent + delta);
        }
        let extent = state.extent.unwrap_or(initial_extent);
        state.changed = false;

        let mut panes = ui
            .layout(SplitLayout {
                config,
                extent,
                minimum_leading,
                minimum_trailing,
            })
            .widget_id(id);
        panes
            .child()
            .item(SplitItem::Leading)
            .widget_id(leading_id)
            .insert(leading);
        panes
            .child()
            .item(SplitItem::Divider)
            .widget_id(divider_id)
            .insert(divider.into_widget(config.axis, interaction));
        panes
            .child()
            .item(SplitItem::Trailing)
            .widget_id(trailing_id)
            .insert(trailing);
    }
}

#[derive(Clone, Copy)]
struct SplitLayout {
    config: Config,
    extent: f32,
    minimum_leading: f32,
    minimum_trailing: f32,
}

#[derive(Clone, Copy)]
enum SplitItem {
    Leading,
    Divider,
    Trailing,
}

impl<R: Platform> Layout<R> for SplitLayout {
    type Item = SplitItem;

    fn layout(&self, ui: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        fn extent(size: Size, axis: Axis) -> f32 {
            match axis {
                Axis::Horizontal => size.width,
                Axis::Vertical => size.height,
            }
        }

        fn flow_size(main: f32, cross: f32, axis: Axis) -> Size {
            match axis {
                Axis::Horizontal => Size::new(main, cross),
                Axis::Vertical => Size::new(cross, main),
            }
        }

        fn flow_constraints(axis: Axis, main: (f32, f32), cross: (f32, f32)) -> Constraints {
            let (width, height) = match axis {
                Axis::Horizontal => (main, cross),
                Axis::Vertical => (cross, main),
            };
            Constraints {
                min: Size::new(width.0, height.0),
                max: Size::new(width.1, height.1),
            }
        }

        let config = self.config;
        let axis = config.axis;
        let mut leading = None;
        let mut divider = None;
        let mut trailing = None;
        for child in ui.children() {
            match ui.item(child) {
                SplitItem::Leading => leading = Some(child),
                SplitItem::Divider => divider = Some(child),
                SplitItem::Trailing => trailing = Some(child),
            }
        }
        let leading = leading.expect("split pane leading content is missing");
        let divider = divider.expect("split pane divider is missing");
        let trailing = trailing.expect("split pane trailing content is missing");

        let cross_axis = match axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        let main_min = extent(constraints.min, axis);
        let main_max = extent(constraints.max, axis);
        let cross_min = extent(constraints.min, cross_axis);
        let cross_max = extent(constraints.max, cross_axis);
        let cross_range = (cross_min, cross_max);
        let minimum_leading = ui.resolve_extent(axis, self.minimum_leading).max(0.0);
        let minimum_trailing = ui.resolve_extent(axis, self.minimum_trailing).max(0.0);
        let desired = ui.resolve_extent(axis, self.extent).max(0.0);
        let divider_extent = ui.resolve_extent(axis, config.divider_extent).max(0.0);

        let natural_trailing = if main_max.is_finite() {
            0.0
        } else {
            let size = ui.layout_child(
                trailing,
                flow_constraints(axis, (minimum_trailing, f32::INFINITY), cross_range),
            );
            extent(size, axis)
        };
        let natural_main = desired.max(minimum_leading) + divider_extent + natural_trailing;
        let main = natural_main.max(main_min).min(main_max);
        let divider_extent = divider_extent.min(main);
        let available = (main - divider_extent).max(0.0);
        let minimum_total = minimum_leading + minimum_trailing;
        let leading_extent = if minimum_total <= available {
            desired.clamp(minimum_leading, available - minimum_trailing)
        } else if minimum_total > 0.0 {
            available * minimum_leading / minimum_total
        } else {
            desired.min(available)
        };
        let trailing_extent = available - leading_extent;

        let leading_size = ui.layout_child(
            leading,
            flow_constraints(axis, (leading_extent, leading_extent), cross_range),
        );
        let divider_size = ui.layout_child(
            divider,
            flow_constraints(axis, (divider_extent, divider_extent), cross_range),
        );
        let trailing_size = ui.layout_child(
            trailing,
            flow_constraints(axis, (trailing_extent, trailing_extent), cross_range),
        );
        let cross = extent(leading_size, cross_axis)
            .max(extent(divider_size, cross_axis))
            .max(extent(trailing_size, cross_axis))
            .max(cross_min)
            .min(cross_max);
        for (child, main) in [
            (leading, leading_extent),
            (divider, divider_extent),
            (trailing, trailing_extent),
        ] {
            if extent(ui.size(child), cross_axis) != cross {
                ui.layout_child(child, flow_constraints(axis, (main, main), (cross, cross)));
            }
        }

        ui.set_position(leading, Point::ZERO);
        ui.set_position(
            divider,
            match axis {
                Axis::Horizontal => Point::new(leading_extent, 0.0),
                Axis::Vertical => Point::new(0.0, leading_extent),
            },
        );
        ui.set_position(
            trailing,
            match axis {
                Axis::Horizontal => Point::new(leading_extent + divider_extent, 0.0),
                Axis::Vertical => Point::new(0.0, leading_extent + divider_extent),
            },
        );
        flow_size(main, cross, axis)
    }
}

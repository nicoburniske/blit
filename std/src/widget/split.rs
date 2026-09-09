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

    fn into_widget(self, _: Axis, _: Interaction) -> Self::Widget {}
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
            .child(SplitItem::Leading)
            .widget_id(leading_id)
            .build(leading);
        panes
            .child(SplitItem::Divider)
            .widget_id(divider_id)
            .build(divider.into_widget(config.axis, interaction));
        panes
            .child(SplitItem::Trailing)
            .widget_id(trailing_id)
            .build(trailing);
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

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, bounds: Constraints) -> Size {
        let axis = self.config.axis;
        let cross_axis = axis.other();
        let res = cx.resolution();
        let main = axis.extent(bounds.max);
        assert!(main.is_finite(), "split needs a finite main axis budget");
        let mut leading = None;
        let mut trailing = None;
        let mut divider = None;
        for child in cx.children() {
            match cx.item(child) {
                SplitItem::Leading => leading = Some(child),
                SplitItem::Trailing => trailing = Some(child),
                SplitItem::Divider => divider = Some(child),
            }
        }
        let leading = leading.expect("missing split leading content");
        let trailing = trailing.expect("missing split trailing content");
        let divider = divider.expect("missing split divider");
        let divider_extent = res
            .extent(axis, self.config.divider_extent)
            .max(0.0)
            .min(main);
        let available = (main - divider_extent).max(0.0);
        let min_leading = res.extent(axis, self.minimum_leading).max(0.0);
        let min_trailing = res.extent(axis, self.minimum_trailing).max(0.0);
        let desired = res.extent(axis, self.extent).max(0.0);
        let leading_extent = if min_leading + min_trailing <= available {
            desired.clamp(min_leading, available - min_trailing)
        } else if min_leading + min_trailing > 0.0 {
            available * min_leading / (min_leading + min_trailing)
        } else {
            desired.min(available)
        };
        let mut cross = cross_axis.extent(bounds.min);
        for (child, extent, offset) in [
            (leading, leading_extent, 0.0),
            (
                trailing,
                available - leading_extent,
                leading_extent + divider_extent,
            ),
        ] {
            let mut child_bounds = bounds;
            axis.set_extent(&mut child_bounds.min, extent);
            axis.set_extent(&mut child_bounds.max, extent);
            let size = cx.layout_child(child, child_bounds);
            cross = cross.max(cross_axis.extent(size));
            let mut point = Size::ZERO;
            axis.set_extent(&mut point, offset);
            cx.set_child_position(child, Point::new(point.width, point.height));
        }
        let mut size = Size::ZERO;
        axis.set_extent(&mut size, divider_extent);
        cross_axis.set_extent(&mut size, cross);
        cx.layout_child(divider, Constraints::tight(size));
        let mut point = Size::ZERO;
        axis.set_extent(&mut point, leading_extent);
        cx.set_child_position(divider, Point::new(point.width, point.height));
        axis.set_extent(&mut size, main);
        bounds.constrain(size)
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}

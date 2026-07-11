use crate::{
    Color, LogicalInsets, LogicalRect, LogicalSize, Sense, SizedComponent, Text, TextOptions,
    TextStyle, Ui, WidgetId,
    widgets::{BorderRadius, Rectangle},
};

crate::component! {
    pub struct Button<'a> {
        pub label: &'a str,
        #[skip]
        pub id: Option<WidgetId>,
        pub background: Color = Color::from_rgba8(45, 55, 70, 255),
        pub clicked_background: Color = Color::from_rgba8(70, 110, 190, 255),
        pub border_color: Color,
        pub clicked_border_color: Color,
        pub border_width: f32,
        pub radius: BorderRadius,
        pub opacity: f32 = 1.0,
        pub text_color: Color = Color::WHITE,
        pub clicked_text_color: Color = Color::WHITE,
        pub text_style: TextStyle,
        pub text_options: TextOptions,
        pub padding: LogicalInsets = LogicalInsets::uniform(8.0),
    }
    features: [padding, border, radius, text_style]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    clicked: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            ..Self::default()
        }
    }

    pub fn id(mut self, source: impl std::hash::Hash) -> Self {
        self.id = Some(WidgetId::new(source));
        self
    }

    pub fn render(self, ui: &mut Ui, area: LogicalRect) -> Response {
        let local_id = self.id.unwrap_or_else(|| WidgetId::new(self.label));
        let interaction = ui.interact(ui.id(("button", local_id)), area, Sense::CLICK);
        if interaction.pressed || interaction.clicked {
            ui.invalidate(area);
        }
        let active = interaction.pressed || interaction.clicked;
        Rectangle::new(area)
            .background(if active {
                self.clicked_background
            } else {
                self.background
            })
            .border(
                self.border_width,
                if active {
                    self.clicked_border_color
                } else {
                    self.border_color
                },
            )
            .radius(self.radius)
            .opacity(self.opacity)
            .render(ui);
        Text::new(self.label)
            .in_area(area.inset(self.padding))
            .color(if active {
                self.clicked_text_color
            } else {
                self.text_color
            })
            .font(self.text_style.font)
            .text_size(self.text_style.size)
            .text_weight(self.text_style.weight)
            .options(self.text_options)
            .render(ui);
        Response {
            clicked: interaction.clicked,
        }
    }
}

impl Response {
    pub fn clicked(self) -> bool {
        self.clicked
    }
}

impl SizedComponent for Button<'_> {
    type Output = Response;

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        let height = (self.text_style.size + self.padding.top + self.padding.bottom)
            .max(self.border_width * 2.0)
            .min(available.height);
        LogicalSize {
            width: available.width,
            height,
        }
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        Button::render(self, ui, area)
    }
}

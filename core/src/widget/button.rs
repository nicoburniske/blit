use super::{Text, Widget};
use crate::{
    Appearance, Ui,
    color::Color,
    geometry::LogicalInsets,
    interact::{Sense, WidgetId},
    paint::{BorderRadius, TextOptions, TextStyle},
};

crate::widget! {
    pub struct Button<'a> {
        new(pub label: &'a str);
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

impl Button<'_> {
    pub fn id(mut self, source: impl std::hash::Hash) -> Self {
        self.id = Some(WidgetId::new(source));
        self
    }
}

impl Response {
    pub fn clicked(self) -> bool {
        self.clicked
    }
}

impl Widget for Button<'_> {
    type Output = Response;

    fn build(self, ui: &mut Ui) -> Response {
        let local_id = self.id.unwrap_or_else(|| WidgetId::new(self.label));
        let id = WidgetId::new(("button", local_id));
        let interaction = ui.interact(id, Sense::CLICK);
        let active = interaction.pressed || interaction.clicked;
        let mut button = ui
            .container()
            .row()
            .padding(self.padding)
            .id(id)
            .appearance(
                Appearance::new()
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
                    .opacity(self.opacity),
            )
            .open();
        button.add(
            Text::new(self.label)
                .color(if active {
                    self.clicked_text_color
                } else {
                    self.text_color
                })
                .font(self.text_style.font)
                .text_size(self.text_style.size)
                .text_weight(self.text_style.weight)
                .options(self.text_options),
        );
        Response {
            clicked: interaction.clicked,
        }
    }
}

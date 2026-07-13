use blit::{
    FontId, ImageData, ImageId, Input, KeyboardRequest, LogicalPoint, LogicalRect, PhysicalRect,
    Platform, PlatformImpl, TextRequest,
    widgets::{BorderRadius, ImageRequest, Rectangle},
};
use blit_software::{Font, FontFace, FontSettings, Renderer, RendererConfig, Scanline, VecBuffer};
use minifb::{InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub struct TestPlatform {
    window: Window,
    renderer: Renderer<VecBuffer<u32>, Scanline>,
    width: usize,
    height: usize,
    mouse_down: bool,
    mouse_position: Option<LogicalPoint>,
    characters: Arc<Mutex<VecDeque<char>>>,
}

impl TestPlatform {
    pub fn new(width: usize, height: usize) -> Self {
        let mut window = Window::new("Blit Todo", width, height, WindowOptions::default()).unwrap();
        window.set_target_fps(60);
        let characters = Arc::new(Mutex::new(VecDeque::new()));
        window.set_input_callback(Box::new(TextInputCharacters {
            characters: characters.clone(),
        }));
        Self {
            window,
            renderer: Renderer::new(
                VecBuffer::new(width, height),
                RendererConfig {
                    fonts: vec![FontFace {
                        id: FontId::default(),
                        weight: 400,
                        font: Font::from_bytes(
                            include_bytes!("../assets/Montserrat-Regular.ttf") as &[u8],
                            FontSettings::default(),
                        )
                        .unwrap(),
                    }],
                    glyph_cache_capacity: 1024 * 1024,
                    paragraph_cache_capacity: 1024 * 1024,
                },
            )
            .strategy(Scanline::default()),
            width,
            height,
            mouse_down: false,
            mouse_position: None,
            characters,
        }
    }

    pub fn handle(&mut self) -> Platform {
        unsafe { Platform::new(self) }
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(Key::Escape)
    }

    pub fn input(&mut self) -> Input {
        if self.window.is_key_pressed(Key::Backspace, KeyRepeat::Yes) {
            return Input::Backspace;
        }
        if self.window.is_key_pressed(Key::Delete, KeyRepeat::Yes) {
            return Input::Delete;
        }
        if self.window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
            return Input::CursorLeft;
        }
        if self.window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
            return Input::CursorRight;
        }
        if self.window.is_key_pressed(Key::Enter, KeyRepeat::No) {
            return Input::Enter;
        }
        if self.window.is_key_pressed(Key::Tab, KeyRepeat::No) {
            return Input::Tab;
        }
        if let Some(character) = self.characters.lock().unwrap().pop_front() {
            return Input::Char(character);
        }
        let down = self.window.get_mouse_down(MouseButton::Left);
        let position = self
            .window
            .get_mouse_pos(MouseMode::Discard)
            .map(|(x, y)| LogicalPoint { x, y });
        if down != self.mouse_down {
            self.mouse_down = down;
            if let Some(event_position) = position.or(self.mouse_position) {
                if position.is_some() {
                    self.mouse_position = position;
                }
                return if down {
                    Input::PointerDown {
                        position: event_position,
                    }
                } else {
                    Input::PointerUp {
                        position: event_position,
                        leave: false,
                    }
                };
            }
        }
        if position != self.mouse_position {
            self.mouse_position = position;
            return position.map_or(Input::PointerLeave, |position| Input::PointerMove {
                position,
            });
        }
        Input::None
    }

    pub fn present(&mut self) {
        self.window
            .update_with_buffer(self.renderer.buffer().pixels(), self.width, self.height)
            .unwrap();
    }
}

impl PlatformImpl for TestPlatform {
    fn begin_frame(&mut self, damage: &[PhysicalRect]) {
        self.renderer.begin_frame(damage)
    }

    fn end_frame(&mut self) {
        self.renderer.end_frame()
    }

    fn screen(&mut self) -> PhysicalRect {
        self.renderer.screen()
    }

    fn scale_factor(&mut self) -> f32 {
        self.renderer.scale_factor()
    }

    fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.renderer.push_rounded_clip(area, radius)
    }

    fn pop_rounded_clip(&mut self) {
        self.renderer.pop_rounded_clip()
    }

    fn draw_rectangle(&mut self, rectangle: &Rectangle, clip: PhysicalRect) {
        self.renderer.draw_rectangle(rectangle, clip)
    }

    fn create_image(&mut self, data: ImageData) -> ImageId {
        self.renderer.create_image(data)
    }

    fn drop_image(&mut self, image: ImageId) {
        self.renderer.drop_image(image)
    }

    fn draw_image(&mut self, image: &ImageRequest, clip: PhysicalRect) {
        self.renderer.draw_image(image, clip)
    }

    fn draw_text(&mut self, request: &TextRequest<'_>, clip: PhysicalRect) {
        self.renderer.draw_text(request, clip)
    }

    fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        self.renderer.text_offset_at_position(request, position)
    }

    fn text_cursor_rect(&mut self, request: &TextRequest<'_>, byte_offset: usize) -> LogicalRect {
        self.renderer.text_cursor_rect(request, byte_offset)
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {}
}

struct TextInputCharacters {
    characters: Arc<Mutex<VecDeque<char>>>,
}

impl InputCallback for TextInputCharacters {
    fn add_char(&mut self, character: u32) {
        if let Some(character) = char::from_u32(character) {
            self.characters.lock().unwrap().push_back(character);
        }
    }
}

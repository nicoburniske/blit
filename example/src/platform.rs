use bullseye::{
    Input, KeyboardRequest, LogicalPoint, LogicalRect, PhysicalRect, Platform, PlatformImpl,
    TextRequest,
    widgets::{Image, Rectangle},
};
use minifb::{InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use software_renderer::{Font, FontSettings, Renderer, RendererConfig, VecBuffer};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub struct TestPlatform {
    window: Window,
    renderer: Renderer<VecBuffer<u32>>,
    width: usize,
    height: usize,
    mouse_down: bool,
    characters: Arc<Mutex<VecDeque<char>>>,
}

impl TestPlatform {
    pub fn new(width: usize, height: usize) -> Self {
        let mut window =
            Window::new("Bullseye Todo", width, height, WindowOptions::default()).unwrap();
        window.set_target_fps(60);
        let characters = Arc::new(Mutex::new(VecDeque::new()));
        window.set_input_callback(Box::new(TextInputCharacters {
            characters: characters.clone(),
        }));
        Self {
            window,
            renderer: Renderer::new(
                VecBuffer::new(width, height),
                RendererConfig::new(
                    Font::from_bytes(
                        include_bytes!("../assets/Montserrat-Regular.ttf") as &[u8],
                        FontSettings::default(),
                    )
                    .unwrap(),
                ),
            ),
            width,
            height,
            mouse_down: false,
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
        let input = if down != self.mouse_down {
            self.window.get_mouse_pos(MouseMode::Clamp).map(|(x, y)| {
                let position = LogicalPoint { x, y };
                if down {
                    Input::PointerDown { position }
                } else {
                    Input::PointerUp { position }
                }
            })
        } else {
            None
        };
        self.mouse_down = down;
        input.unwrap_or_default()
    }

    pub fn present(&mut self) {
        self.window
            .update_with_buffer(self.renderer.buffer().pixels(), self.width, self.height)
            .unwrap();
    }
}

impl PlatformImpl for TestPlatform {
    fn screen(&mut self) -> PhysicalRect {
        self.renderer.screen()
    }

    fn scale_factor(&mut self) -> f32 {
        self.renderer.scale_factor()
    }

    fn draw_rectangle(&mut self, rectangle: &Rectangle, clips: &[PhysicalRect]) {
        self.renderer.draw_rectangle(rectangle, clips)
    }

    fn draw_image(&mut self, image: &Image<'_>, clips: &[PhysicalRect]) {
        self.renderer.draw_image(image, clips)
    }

    fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]) {
        self.renderer.draw_text(request, clips)
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

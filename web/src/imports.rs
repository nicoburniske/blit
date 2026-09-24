pub mod canvas {
    #[link(wasm_import_module = "canvas")]
    unsafe extern "C" {
        pub fn clear();
        pub fn fill_rect(
            left: f32,
            top: f32,
            width: f32,
            height: f32,
            radius: f32,
            color: u32,
            border: u32,
            border_width: f32,
        );
        pub fn measure_text(
            text: *const u8,
            length: usize,
            width: f32,
            size: f32,
            font: u32,
            weight: u32,
            line_height: f32,
            measured: *mut f32,
        );
        pub fn fill_text(
            text: *const u8,
            length: usize,
            left: f32,
            top: f32,
            width: f32,
            height: f32,
            size: f32,
            font: u32,
            weight: u32,
            line_height: f32,
            color: u32,
            heading: u32,
            live: u32,
        );
        pub fn action(
            label: *const u8,
            length: usize,
            href: *const u8,
            href_length: usize,
            left: f32,
            top: f32,
            width: f32,
            height: f32,
            selected: u32,
        );
        pub fn range(
            label: *const u8,
            label_length: usize,
            value_text: *const u8,
            value_text_length: usize,
            minimum: usize,
            maximum: usize,
            value: usize,
            left: f32,
            top: f32,
            width: f32,
            height: f32,
        );
        pub fn push_clip(left: f32, top: f32, width: f32, height: f32);
        pub fn pop_clip();
    }
}

pub mod browser {
    #[link(wasm_import_module = "browser")]
    unsafe extern "C" {
        pub fn set_document_height(height: f32);
        pub fn navigate(href: *const u8, length: usize);
        pub fn copy_text(text: *const u8, length: usize);
        pub fn set_cursor(pointer: u32);
        #[cfg(target_arch = "wasm32")]
        pub fn report_error(text: *const u8, length: usize);
    }
}

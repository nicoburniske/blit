#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::{sync::mpsc, time::Duration};

use blit::{Clip, LogicalRect, PhysicalRect, PhysicalSize};
use blit_gpu::{Renderer, RendererConfig};
use blit_gui::{
    BoundsClip, FontData, FontFamily, GuiContext, TextConfig, TextSystem,
    color::Color,
    display_list::{BoxShadow, ClipId, Mesh, MeshVertex, Rectangle},
    image::{
        ImageData, ImageFit, ImageFormat, ImagePixels, ImageRequest, ImageSampling, ImageTiling,
    },
    style::{Border, BorderRadius, GradientStop, LinearGradient},
    text::{FontId, TextOptions, TextRequest, TextStyle},
};

#[test]
fn renders_primitives_and_rebuilds_frames() {
    #[cfg(target_os = "linux")]
    let backends = wgpu::Backends::VULKAN;
    #[cfg(target_os = "macos")]
    let backends = wgpu::Backends::METAL;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let Ok(adapter) =
        blit_executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("skipping native GPU test because no adapter is available");
        return;
    };
    let (device, queue) =
        blit_executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .unwrap();

    const WIDTH: u32 = 128;
    const HEIGHT: u32 = 128;
    const ROW_BYTES: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT * 2;
    let extent = wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blit gpu native test target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut renderer = Renderer::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8Unorm,
        RendererConfig::default(),
    );
    let mut gui = GuiContext::new(
        TextSystem::new(
            TextConfig {
                fonts: vec![FontFamily {
                    id: FontId::default(),
                    fonts: vec![FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")))],
                }],
                text_cache_capacity: 1024,
                layout_cache_capacity: 1024,
            },
            blit_text_fontdue::Backend::new(),
        )
        .unwrap(),
    );
    gui.set_scale(2.0);
    gui.paint_rectangle(
        Rectangle::new(LogicalRect::new(0.5, 8.0, 0.5, 0.5))
            .background(Color::from_rgba8(255, 255, 255, 255)),
    );
    renderer.render(&target, gui.render_input());

    gui.finish_frame();
    gui.paint_rectangle(
        Rectangle::new(LogicalRect::new(0.0, 0.0, 7.0, 7.0))
            .background(Color::from_rgba8(255, 0, 0, 255))
            .radius(BorderRadius::uniform(3.0)),
    );
    gui.paint_rectangle(
        Rectangle::new(LogicalRect::new(2.0, 2.0, 2.0, 2.0))
            .background(Color::from_rgba8(0, 0, 255, 128)),
    );
    let image = gui.create_image(ImageData::new(
        ImagePixels::Static(&[
            255, 255, 0, 255, 0, 255, 255, 255, 255, 0, 255, 255, 255, 255, 255, 255,
        ]),
        ImageFormat::Rgba8Premultiplied,
        2,
        2,
    ));
    let nearest = ImageRequest {
        image: image.id(),
        area: LogicalRect::new(8.0, 5.0, 4.0, 4.0),
        fit: ImageFit::Fill,
        sampling: ImageSampling::Nearest,
        opacity: 1.0,
        colorize: None,
        nine_slice: None,
        horizontal_tiling: ImageTiling::Repeat,
        vertical_tiling: ImageTiling::Repeat,
    };
    let sparse_x = i32::MAX / 3 - 1;
    let mut sparse = ImageData::new(
        ImagePixels::Static(&[255, 0, 0, 255, 0, 0, 255, 255]),
        ImageFormat::Rgba8Premultiplied,
        2,
        1,
    );
    sparse.size = PhysicalSize {
        width: i32::MAX,
        height: 1,
    };
    sparse.texture_rect = PhysicalRect {
        x: sparse_x,
        y: 0,
        width: 2,
        height: 1,
    };
    let sparse = gui.create_image(sparse);
    let mut partial = ImageData::new(
        ImagePixels::Static(&[255, 0, 0, 255]),
        ImageFormat::Rgba8Premultiplied,
        1,
        1,
    );
    partial.size.width = 3;
    partial.texture_rect.x = 1;
    let partial = gui.create_image(partial);
    gui.paint_image(ImageRequest {
        area: LogicalRect::new(8.0, 0.0, 4.0, 4.0),
        sampling: ImageSampling::Bilinear,
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
        ..nearest
    });
    gui.paint_image(nearest);
    gui.paint_image(ImageRequest {
        image: sparse.id(),
        area: LogicalRect::new(-0.5, 8.0, 1.5, 0.5),
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
        ..nearest
    });
    gui.paint_image(ImageRequest {
        image: sparse.id(),
        area: LogicalRect::new(-0.5, 8.5, 1.5, 0.5),
        horizontal_tiling: ImageTiling::Round,
        vertical_tiling: ImageTiling::None,
        ..nearest
    });
    let partial_request = ImageRequest {
        image: partial.id(),
        area: LogicalRect::new(20.0, 0.0, 3.0, 1.0),
        horizontal_tiling: ImageTiling::None,
        vertical_tiling: ImageTiling::None,
        ..nearest
    };
    gui.paint_image(partial_request);
    gui.paint_image(ImageRequest {
        area: LogicalRect::new(20.0, 1.0, 3.0, 1.0),
        sampling: ImageSampling::Bilinear,
        ..partial_request
    });
    gui.paint_shadow(
        BoxShadow::new(
            LogicalRect::new(14.0, 9.0, 1.0, 1.0),
            Color::from_rgba8(255, 0, 0, 255),
        )
        .blur(1.0),
    );
    let text = gui.text_run("M", TextStyle::default());
    gui.paint_text(TextRequest {
        text,
        area: LogicalRect::new(1.0, 20.0, 12.0, 20.0),
        offset_x: 0.0,
        color: Color::from_rgba8(255, 0, 0, 255),
        options: TextOptions::default(),
    });
    gui.paint_rectangle(
        Rectangle::new(LogicalRect::new(14.0, 14.0, 50.0, 50.0))
            .background(Color::from_rgba8(0, 255, 0, 255))
            .radius(BorderRadius {
                top_left: 40.0,
                top_right: 10.0,
                ..BorderRadius::default()
            }),
    );
    let mesh_vertices = [
        MeshVertex::new(40.0, 40.0, Color::from_rgba8(255, 0, 0, 255)),
        MeshVertex::new(60.0, 40.0, Color::from_rgba8(255, 0, 0, 255)),
        MeshVertex::new(40.0, 60.0, Color::from_rgba8(255, 0, 0, 255)),
    ];
    BoundsClip.push(&mut gui, LogicalRect::new(40.0, 40.0, 10.0, 20.0));
    gui.paint_mesh(Mesh {
        vertices: &mesh_vertices,
        indices: &[0, 1, 2],
    });
    BoundsClip.pop(&mut gui);

    let stops = [
        GradientStop::new(0.0, Color::from_rgba8(255, 0, 0, 255)),
        GradientStop::new(1.0, Color::from_rgba8(0, 0, 255, 255)),
    ];
    let input = gui.render_input();
    let outer = input.display_list.push_clip(
        ClipId::default(),
        LogicalRect::new(12.5, 1.0, 3.5, 4.0),
        BorderRadius::default(),
    );
    let inner = input.display_list.push_clip(
        outer,
        LogicalRect::new(12.0, 1.5, 3.5, 3.5),
        BorderRadius::default(),
    );
    input.display_list.push_rectangle(
        Rectangle::new(LogicalRect::new(12.0, 1.0, 4.0, 4.0))
            .border(Border::gradient(1.0, LinearGradient::new(&stops))),
        PhysicalRect {
            x: 24,
            y: 2,
            width: 8,
            height: 8,
        },
        inner,
    );

    renderer.render(&target, input);

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blit gpu native test readback"),
        size: u64::from(ROW_BYTES * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blit gpu native test readback"),
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_BYTES),
                rows_per_image: None,
            },
        },
        extent,
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    receiver.recv().unwrap().unwrap();

    let bytes = readback.get_mapped_range(..).unwrap();
    let pixel = |x: usize, y: usize| {
        let start = y * ROW_BYTES as usize + x * 4;
        <[u8; 4]>::try_from(&bytes[start..start + 4]).unwrap()
    };
    assert_eq!(pixel(0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(6, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(6, 6), [127, 0, 128, 255]);
    assert_eq!(pixel(16, 0), [255, 255, 0, 255]);
    assert_eq!(pixel(18, 2), [227, 227, 60, 255]);
    assert_eq!(pixel(23, 0), [0, 255, 255, 255]);
    assert_eq!(pixel(16, 7), [255, 0, 255, 255]);
    assert_eq!(pixel(23, 7), [255, 255, 255, 255]);
    assert_eq!(pixel(16, 9), [0, 0, 0, 0]);
    assert_eq!(pixel(16, 10), [255, 255, 0, 255]);
    assert_eq!(pixel(17, 11), [255, 255, 0, 255]);
    assert_eq!(pixel(18, 10), [0, 255, 255, 255]);
    assert_eq!(pixel(16, 12), [255, 0, 255, 255]);
    assert_eq!(pixel(19, 13), [255, 255, 255, 255]);
    assert_eq!(pixel(20, 14), [255, 255, 0, 255]);
    assert_eq!(pixel(23, 17), [255, 255, 255, 255]);
    assert_eq!(pixel(0, 16), [0, 0, 255, 255]);
    assert_eq!(pixel(1, 16), [0, 0, 0, 0]);
    assert_eq!(pixel(0, 17), [0, 0, 255, 255]);
    assert_eq!(pixel(1, 17), [0, 0, 0, 0]);
    assert_eq!(pixel(40, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(42, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(44, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(40, 2), [0, 0, 0, 0]);
    assert_eq!(pixel(45, 2), [0, 0, 0, 0]);
    let partial_blend = pixel(41, 2);
    assert!(partial_blend[0] > 0 && partial_blend[0] < 255);
    assert_eq!(partial_blend[0], partial_blend[3]);
    assert_eq!(pixel(24, 5), [0, 0, 0, 0]);
    assert_eq!(pixel(31, 5), [0, 0, 0, 0]);
    let left = pixel(25, 5);
    let right = pixel(30, 5);
    assert!(left[0] > left[2] && left[3] > 0);
    assert!(right[2] > right[0] && right[3] > 0);
    let fringe = pixel(27, 19);
    assert!(fringe[0] > 0 && fringe[3] > 0 && fringe[3] < 255);
    assert!((40..80).any(|y| (2..26).any(|x| pixel(x, y)[0] > 0)));
    assert_eq!(pixel(88, 28), [0, 0, 0, 0]);
    assert_eq!(pixel(108, 29), [0, 255, 0, 255]);
    assert_eq!(pixel(90, 90), [255, 0, 0, 255]);
    assert_eq!(pixel(108, 90), [0, 255, 0, 255]);
    assert_eq!(pixel(118, 118), [0, 255, 0, 255]);
}

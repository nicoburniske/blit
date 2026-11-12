#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::{sync::mpsc, time::Duration};

use blit::{Clip, LogicalRect, Paint, Rect, Size};
use blit_gpu::{Renderer, RendererConfig};
use blit_gui::{
    BoundsClip, Context, FontData, FontStack, Image, Rectangle, Shadow, TextConfig, TextSystem,
    color::Color,
    image::{ImageData, ImageFormat, ImagePixels, ImageSampling, ImageTiling},
    mesh::{Mesh, MeshVertex},
    style::{Border, BorderRadius, ConicGradient, GradientStop, LinearGradient},
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
    let Ok(adapter) = blit_executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("skipping native GPU test because no adapter is available");
        return;
    };
    let (device, queue) = blit_executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();

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
    let text = TextSystem::new(
        TextConfig {
            fonts: vec![FontStack {
                id: FontId::default(),
                families: vec![blit_gui::Family::embedded([FontData::Static(include_bytes!(env!(
                    "BLIT_TEST_FONT"
                )))])],
            }],
            text_cache_capacity: 1024,
            layout_cache_capacity: 1024,
        },
        blit_text_parley::Backend::without_system_fonts(),
    )
    .unwrap();
    let mut renderer = Renderer::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8Unorm,
        text,
        RendererConfig::default(),
    );
    let bounds = LogicalRect::new(0.0, 0.0, 64.0, 64.0);
    renderer.push_clip(LogicalRect::new(0.0, 0.0, 1.0, 1.0), BorderRadius::default());
    Rectangle::new()
        .background(Color::rgb(255, 255, 255))
        .paint(bounds, &mut renderer);
    renderer.discard_frame();
    renderer.set_scale(2.0);
    Rectangle::new()
        .background(Color::rgb(255, 255, 255))
        .paint(LogicalRect::new(0.5, 8.0, 0.5, 0.5), &mut renderer);
    renderer.render(&target);

    Rectangle::new()
        .background(Color::rgb(255, 0, 0))
        .radius(BorderRadius::uniform(3.0))
        .paint(LogicalRect::new(0.0, 0.0, 7.0, 7.0), &mut renderer);
    Rectangle::new()
        .background(Color::rgba(0, 0, 255, 128))
        .paint(LogicalRect::new(2.0, 2.0, 2.0, 2.0), &mut renderer);
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[255, 255, 0, 255, 0, 255, 255, 255, 255, 0, 255, 255, 255, 255, 255, 255]),
        ImageFormat::Rgba8Premultiplied,
        2,
        2,
    ));
    let nearest = Image::new(&image)
        .horizontal_tiling(ImageTiling::Repeat)
        .vertical_tiling(ImageTiling::Repeat);
    let sparse_x = u32::MAX / 3 * 2 - 1;
    let mut sparse = ImageData::new(
        ImagePixels::Static(&[255, 0, 0, 255, 0, 0, 255, 255]),
        ImageFormat::Rgba8Premultiplied,
        2,
        1,
    );
    sparse.size = Size {
        width: u32::MAX,
        height: 1,
    };
    sparse.texture_rect = Rect {
        x: sparse_x,
        y: 0,
        width: 2,
        height: 1,
    };
    let sparse = renderer.create_image(sparse);
    let mut partial = ImageData::new(
        ImagePixels::Static(&[255, 0, 0, 255]),
        ImageFormat::Rgba8Premultiplied,
        1,
        1,
    );
    partial.size.width = 3;
    partial.texture_rect.x = 1;
    let partial = renderer.create_image(partial);
    renderer.paint(
        LogicalRect::new(8.0, 0.0, 4.0, 4.0),
        Image::new(&image).sampling(ImageSampling::Bilinear),
    );
    renderer.paint(LogicalRect::new(8.0, 5.0, 4.0, 4.0), nearest);
    renderer.paint(LogicalRect::new(-1.0, 8.0, 1.5, 0.5), Image::new(&sparse));
    renderer.paint(
        LogicalRect::new(-1.0, 8.5, 1.5, 0.5),
        Image::new(&sparse).horizontal_tiling(ImageTiling::Round),
    );
    let partial_request = Image::new(&partial);
    renderer.paint(LogicalRect::new(20.0, 0.0, 3.0, 1.0), partial_request.clone());
    renderer.paint(
        LogicalRect::new(20.0, 1.0, 3.0, 1.0),
        Image {
            sampling: ImageSampling::Bilinear,
            ..partial_request
        },
    );
    let shadow = Shadow::new(Color::rgb(255, 0, 0)).blur(1.0);
    renderer.paint(LogicalRect::new(14.0, 9.0, 1.0, 1.0), shadow);
    let text = renderer.text().text_run("M", TextStyle::new());
    TextRequest {
        text,
        colors: &[Color::rgb(255, 0, 0)],
        offset_x: 0.0,
        options: TextOptions::default(),
    }
    .paint(LogicalRect::new(1.0, 20.0, 12.0, 20.0), &mut renderer);
    Rectangle::new()
        .background(Color::rgb(0, 255, 0))
        .radius(BorderRadius {
            top_left: 40.0,
            top_right: 10.0,
            ..BorderRadius::default()
        })
        .paint(LogicalRect::new(14.0, 14.0, 50.0, 50.0), &mut renderer);
    let mesh_vertices = [
        MeshVertex::new(40.0, 40.0, Color::rgb(255, 0, 0)),
        MeshVertex::new(60.0, 40.0, Color::rgb(255, 0, 0)),
        MeshVertex::new(40.0, 60.0, Color::rgb(255, 0, 0)),
    ];
    BoundsClip.push(&mut renderer, LogicalRect::new(40.0, 40.0, 10.0, 20.0));
    Mesh {
        vertices: &mesh_vertices,
        indices: &[0, 1, 2],
    }
    .paint(bounds, &mut renderer);
    BoundsClip.pop(&mut renderer);

    let stops = [
        GradientStop::new(0.0, Color::rgb(255, 0, 0)),
        GradientStop::new(1.0, Color::rgb(0, 0, 255)),
    ];
    Rectangle::new()
        .background(LinearGradient::new(&stops))
        .paint(LogicalRect::new(24.0, 1.0, 8.0, 8.0), &mut renderer);
    Rectangle::new()
        .background(LinearGradient::new(&stops))
        .border(Border::solid(1.0, Color::rgba(0, 255, 0, 128)))
        .paint(LogicalRect::new(32.0, 1.0, 8.0, 8.0), &mut renderer);
    let border_stops = [
        GradientStop::new(0.0, Color::rgba(0, 255, 0, 128)),
        GradientStop::new(1.0, Color::rgba(255, 255, 255, 128)),
    ];
    Rectangle::new()
        .background(LinearGradient::new(&stops))
        .border(Border::gradient(1.0, LinearGradient::new(&border_stops).angle(90.0)))
        .radius(BorderRadius::uniform(2.0))
        .paint(LogicalRect::new(40.0, 1.0, 8.0, 8.0), &mut renderer);
    let translucent_stops = [
        GradientStop::new(0.0, Color::rgba(255, 0, 0, 64)),
        GradientStop::new(0.5, Color::rgba(0, 255, 0, 128)),
        GradientStop::new(1.0, Color::rgba(0, 0, 255, 192)),
    ];
    Rectangle::new()
        .background(LinearGradient::new(&translucent_stops).angle(-45.0))
        .opacity(0.5)
        .paint(LogicalRect::new(48.0, 1.0, 8.0, 8.0), &mut renderer);
    BoundsClip.push(&mut renderer, LogicalRect::new(58.0, 1.0, 6.0, 8.0));
    Rectangle::new()
        .background(LinearGradient::new(&stops))
        .paint(LogicalRect::new(56.0, 1.0, 8.0, 8.0), &mut renderer);
    BoundsClip.pop(&mut renderer);
    renderer.push_clip(LogicalRect::new(12.5, 1.0, 3.5, 4.0), BorderRadius::default());
    renderer.push_clip(LogicalRect::new(12.0, 1.5, 3.5, 3.5), BorderRadius::default());
    Rectangle::new()
        .border(Border::gradient(1.0, LinearGradient::new(&stops)))
        .paint(LogicalRect::new(12.0, 1.0, 4.0, 4.0), &mut renderer);
    renderer.pop_clip();
    renderer.pop_clip();
    let conic_stops = [
        GradientStop::new(0.0, Color::rgb(255, 0, 0)),
        GradientStop::new(0.25, Color::rgb(0, 255, 0)),
        GradientStop::new(0.5, Color::rgb(0, 0, 255)),
        GradientStop::new(0.75, Color::WHITE),
        GradientStop::new(1.0, Color::rgb(255, 255, 0)),
    ];
    let conic = ConicGradient::new(&conic_stops).center([0.53125, 0.53125]);
    Rectangle::new()
        .background(LinearGradient::new(&[]))
        .paint(bounds, &mut renderer);
    Rectangle::new()
        .background(conic.angle(450.0))
        .border(Border::gradient(1.0, conic.angle(180.0)))
        .paint(LogicalRect::new(48.0, 22.0, 8.0, 8.0), &mut renderer);
    let width = device.limits().max_texture_dimension_2d + 1;
    let pixels = vec![255; width as usize].into_boxed_slice();
    let oversized = renderer.create_image(ImageData::new(
        ImagePixels::Owned(pixels.clone()),
        ImageFormat::Alpha8(Color::WHITE),
        width,
        1,
    ));
    assert_eq!(
        oversized,
        renderer.create_image(ImageData::new(
            ImagePixels::Owned(pixels),
            ImageFormat::Alpha8(Color::WHITE),
            width,
            1
        ))
    );
    renderer.paint(bounds, Image::new(oversized));
    renderer.render(&target);

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
    assert_eq!(pixel(48, 10), [247, 0, 8, 255]);
    assert_eq!(pixel(63, 10), [8, 0, 247, 255]);
    assert_eq!(pixel(64, 10), [123, 128, 4, 255]);
    assert_eq!(pixel(72, 10), [119, 0, 135, 255]);
    assert_eq!(pixel(80, 2), [0, 0, 0, 0]);
    assert_eq!(pixel(88, 10), [119, 0, 135, 255]);
    let top_border = pixel(80, 6);
    let bottom_border = pixel(80, 13);
    assert_eq!(top_border[1], 128);
    assert_eq!(bottom_border[1], 128);
    assert!(bottom_border[2] > top_border[2]);
    assert_eq!(bottom_border[3], 255);
    assert_eq!(pixel(104, 10), [0, 64, 0, 64]);
    let bottom_left = pixel(96, 17);
    let top_right = pixel(111, 2);
    assert!(bottom_left[0] > bottom_left[2]);
    assert!(top_right[2] > top_right[0]);
    assert!(top_right[3] > bottom_left[3]);
    assert_eq!(pixel(114, 10), [0, 0, 0, 0]);
    assert_eq!(pixel(116, 10), [183, 0, 71, 255]);
    assert_eq!(pixel(104, 52), [255, 0, 0, 255]);
    assert_eq!(pixel(104, 56), [255, 0, 0, 255]);
    assert_eq!(pixel(111, 52), [0, 0, 255, 255]);
    assert_eq!(pixel(108, 52), [255, 255, 255, 255]);

    renderer.push_clip(LogicalRect::default(), BorderRadius::default());
    Mesh {
        vertices: &mesh_vertices,
        indices: &[0, 1, 2],
    }
    .paint(bounds, &mut renderer);
    renderer.paint(bounds, Image::new(&image));
    TextRequest {
        text,
        colors: &[Color::WHITE],
        offset_x: 0.0,
        options: TextOptions::default(),
    }
    .paint(bounds, &mut renderer);
    renderer.pop_clip();
    renderer.render(&target);
    assert_eq!(renderer.batch_count(), 0);
}

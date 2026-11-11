use blit_gui::image::ImageTiling;

use super::*;
use crate::pipeline::FIXED_SHIFT;

#[test]
fn pipeline_is_valid() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut renderer = Renderer::new(
        device.clone(),
        queue,
        wgpu::TextureFormat::Rgba8Unorm,
        TextSystem::new(
            blit_gui::TextConfig {
                fonts: Vec::new(),
                ..Default::default()
            },
            blit_text_parley::Backend::without_system_fonts(),
        )
        .unwrap(),
        RendererConfig::default(),
    );
    struct State<const BITS: u8> {
        kind: u8,
        used: bool,
    }
    impl Render for State<8> {
        type Input = u8;
        type Prepared = ();
        fn new(_: &PrepareContext<'_>) -> Self {
            Self { kind: 8, used: false }
        }
        fn prepare(&mut self, input: &u8, _: &mut PrepareContext<'_>) -> Option<()> {
            assert_eq!(self.kind, 8);
            assert!(!self.used);
            self.used = true;
            assert_eq!(*input, 0, "failed preparation");
            None
        }
        fn render(&mut self, _: &[()], _: &mut RenderContext<'_>) {}
        fn finish_frame(&mut self) {
            self.used = false;
        }
    }
    impl Render for State<16> {
        type Input = u16;
        type Prepared = ();
        fn new(_: &PrepareContext<'_>) -> Self {
            Self { kind: 16, used: false }
        }
        fn prepare(&mut self, _: &u16, _: &mut PrepareContext<'_>) -> Option<()> {
            assert_eq!(self.kind, 16);
            assert!(!self.used);
            self.used = true;
            None
        }
        fn render(&mut self, _: &[()], _: &mut RenderContext<'_>) {}
        fn finish_frame(&mut self) {
            self.used = false;
        }
    }
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    for _ in 0..2 {
        let bounds = blit::LogicalRect::new(0.0, 0.0, 8.0, 8.0);
        let mut recording = renderer.record(bounds);
        recording.push::<State<8>>(0u8);
        recording.push::<State<16>>(0u16);
        renderer.render(&target);
        assert_eq!(renderer.renderers.len(), 2);
    }
    let bounds = blit::LogicalRect::new(0.0, 0.0, 8.0, 8.0);
    renderer.record(bounds).push::<State<8>>(1u8);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(&target)));
    assert!(failure.is_err());
    renderer.record(bounds).push::<State<8>>(0u8);
    renderer.render(&target);
}

#[test]
fn wrapped_image_phase_handles_large_clipped_offsets() {
    let (fixed, phase, step, bilinear, wrap) = image_axis(3, i32::MAX, ImageTiling::Repeat, 1.0, i32::MAX as u64 - 2);

    assert_eq!(fixed, 1 << FIXED_SHIFT);
    assert_eq!(phase, 2 << FIXED_SHIFT);
    assert_eq!(step, 1.0);
    assert_eq!(bilinear, 2.0);
    assert!(wrap);
}

#[test]
fn nearest_image_axis_preserves_wide_fixed_point_values() {
    let (fixed, phase, _, _, wrap) = image_axis(u32::MAX, 2, ImageTiling::None, 1.0, 1);

    assert_eq!(fixed, (u32::MAX as u64) << (FIXED_SHIFT - 1));
    assert_eq!(phase, fixed);
    assert!(!wrap);
}

#[test]
fn bilinear_image_phase_is_unclamped_and_handles_large_offsets() {
    let (_, _, step, phase, wrap) = image_axis(2, 8, ImageTiling::None, 1.0, 0);

    assert_eq!(step, 0.25);
    assert_eq!(phase, -0.375);
    assert!(!wrap);

    let (_, _, step, phase, wrap) = image_axis(3, i32::MAX, ImageTiling::Repeat, 1.0, 20_000_003);

    assert_eq!(step, 1.0);
    assert_eq!(phase, 2.0);
    assert!(wrap);
}

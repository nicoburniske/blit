use blit::PhysicalRect;
use blit_gui::{
    color::Color,
    image::{ImageHandle, ImageId, ImageTiling},
    style::BorderRadius,
};
use bytemuck::{Pod, Zeroable};

use crate::Renderer;

pub const INITIAL_INSTANCES: u64 = 256;
pub const INITIAL_GLYPHS: u64 = 256;
pub const INITIAL_DRAWS: u64 = 256;
pub const INITIAL_MESH_DATA: u64 = 256;
pub const INITIAL_CLIPS: u64 = 64;
pub const INITIAL_STOPS: u64 = 64;
pub const FIXED_SHIFT: u32 = 16;
pub const OUTSET_SHADOW: u32 = 1;
pub const INSET_SHADOW: u32 = 2;
pub const DRAW_TEXT: u32 = 1 << 31;
pub const DRAW_MESH: u32 = 1 << 30;

impl Renderer {
    pub(super) fn push_instances(&mut self, start: u32, pipeline: Pipeline) {
        let end = self.instances.len() as u32;
        if start == end {
            return;
        }
        if matches!(pipeline, Pipeline::Content(_)) {
            for index in start..end {
                push_draw(&mut self.draws, &mut self.batches, pipeline, index);
            }
        } else if let Some(Batch::Draw {
            pipeline: previous,
            end: last,
            ..
        }) = self.batches.last_mut()
            && *previous == pipeline
        {
            *last = end;
        } else {
            self.batches.push(Batch::Draw { pipeline, start, end });
        }
    }

    pub(super) fn push_mesh_primitive(&mut self, vertices: [u32; 4]) {
        assert!(self.mesh_data.len() < DRAW_MESH as usize, "too much GPU mesh data");
        let index = self.mesh_data.len() as u32;
        self.mesh_data.push(MeshData(vertices));
        push_draw(
            &mut self.draws,
            &mut self.batches,
            Pipeline::Content(None),
            DRAW_MESH | index,
        );
    }
}

pub fn push_draw(draws: &mut Vec<u32>, batches: &mut Vec<Batch>, pipeline: Pipeline, draw: u32) {
    let start = draws.len() as u32;
    draws.push(draw);
    let end = draws.len() as u32;
    if let Some(Batch::Draw {
        pipeline: previous,
        end: last,
        ..
    }) = batches.last_mut()
        && let (Pipeline::Content(current), Pipeline::Content(texture)) = (previous, pipeline)
        && (current.is_none() || texture.is_none() || *current == texture)
    {
        if current.is_none() {
            *current = texture;
        }
        *last = end;
        return;
    }
    batches.push(Batch::Draw { pipeline, start, end });
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Frame {
    pub transform: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    pub shape: [f32; 4],
    pub draw: [f32; 4],
    pub radii: [f32; 4],
    pub inner_color: [f32; 4],
    pub border_color: [f32; 4],
    pub params: [f32; 4],
    pub data: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GlyphInstance {
    pub draw: [f32; 4],
    pub atlas: [u32; 2],
    pub color: u32,
    pub clip: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MeshData(pub [u32; 4]);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Clip {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    pub data: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GradientStop {
    pub color: [f32; 4],
    pub position: f32,
    // linear step or conic center
    pub x: f32,
    pub y: f32,
    pub end: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pipeline {
    Clear,
    Content(Option<usize>),
    Image(ImageId),
}

pub struct Effects {
    pub target: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub snapshot: wgpu::Texture,
    pub source: wgpu::TextureView,
    pub binding: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
}

pub enum Batch {
    Draw { pipeline: Pipeline, start: u32, end: u32 },
    Custom(usize),
}

pub struct StoredImage {
    pub handle: ImageHandle,
    pub bind_group: wgpu::BindGroup,
    pub mask_color: Option<Color>,
    pub texture_origin: [u32; 2],
}

pub fn grow_buffer<T>(device: &wgpu::Device, buffer: &mut wgpu::Buffer, items: &[T], label: &str) {
    let item_size = size_of::<T>() as u64;
    let required_size = items.len() as u64 * item_size;
    if required_size <= buffer.size() {
        return;
    }
    let limits = device.limits();
    let usage = buffer.usage();
    let limit = if usage.contains(wgpu::BufferUsages::STORAGE) {
        limits.max_buffer_size.min(limits.max_storage_buffer_binding_size)
    } else {
        limits.max_buffer_size
    };
    assert!(required_size <= limit, "GPU frame data exceeds device limits");
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (items.len() as u64)
            .next_power_of_two()
            .saturating_mul(item_size)
            .min(limit),
        usage,
        mapped_at_creation: false,
    });
}

pub fn data_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame: &wgpu::Buffer,
    instances: &wgpu::Buffer,
    clips: &wgpu::Buffer,
    stops: &wgpu::Buffer,
    glyphs: &wgpu::Buffer,
    mesh: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit gpu data"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: instances.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: clips.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: stops.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: glyphs.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: mesh.as_entire_binding(),
            },
        ],
    })
}

pub fn physical_rect(rect: PhysicalRect) -> [f32; 4] {
    [rect.x as f32, rect.y as f32, rect.width as f32, rect.height as f32]
}

pub fn physical_radii(radius: BorderRadius, scale: f32, width: i32, height: i32) -> [f32; 4] {
    let mut radii = [
        (radius.top_left * scale).round().max(0.0) as i32 as f32,
        (radius.top_right * scale).round().max(0.0) as i32 as f32,
        (radius.bottom_right * scale).round().max(0.0) as i32 as f32,
        (radius.bottom_left * scale).round().max(0.0) as i32 as f32,
    ];
    let factor = 1.0f32
        .min(width as f32 / (radii[0] + radii[1]).max(1.0))
        .min(width as f32 / (radii[3] + radii[2]).max(1.0))
        .min(height as f32 / (radii[0] + radii[3]).max(1.0))
        .min(height as f32 / (radii[1] + radii[2]).max(1.0));
    for radius in &mut radii {
        *radius = (*radius * factor).round();
    }
    radii
}

pub fn image_axis(
    source: u32,
    target: i32,
    tiling: ImageTiling,
    scale_factor: f32,
    offset: u64,
) -> (u64, u64, f32, f32, bool) {
    let (fixed, step, wrap) = match tiling {
        ImageTiling::None => (
            ((source as u64) << FIXED_SHIFT) / target as u64,
            source as f32 / target as f32,
            false,
        ),
        ImageTiling::Repeat => {
            let tile = (f64::from(source) * f64::from(scale_factor)).round().max(1.0) as u64;
            (
                ((source as u64) << FIXED_SHIFT) / tile,
                source as f32 / tile as f32,
                true,
            )
        }
        ImageTiling::Round => {
            let native = (f64::from(source) * f64::from(scale_factor)).max(1.0);
            let count = (f64::from(target) / native).round().max(1.0) as u64;
            (
                u64::try_from(((source as u128 * u128::from(count)) << FIXED_SHIFT) / target as u128)
                    .expect("image scale is too large"),
                source as f32 * count as f32 / target as f32,
                true,
            )
        }
    };
    let period = (source as u64) << FIXED_SHIFT;
    let fixed = if wrap { fixed % period } else { fixed };
    let phase = if wrap {
        (u128::from(offset) * u128::from(fixed) % u128::from(period)) as u64
    } else {
        offset.checked_mul(fixed).expect("image coordinate is too large")
    };
    let bilinear = (offset as f64 + 0.5) * step as f64 - 0.5;
    let bilinear = if wrap {
        bilinear.rem_euclid(source as f64)
    } else {
        bilinear
    } as f32;
    (fixed, phase, step, bilinear, wrap)
}

pub fn premultiplied(color: Color, opacity: f32) -> [f32; 4] {
    let alpha = color.alpha as f32 / 255.0 * opacity;
    [
        color.red as f32 / 255.0 * alpha,
        color.green as f32 / 255.0 * alpha,
        color.blue as f32 / 255.0 * alpha,
        alpha,
    ]
}

pub mod image_flag {
    pub const WRAP_X: u32 = 1;
    pub const WRAP_Y: u32 = 2;
    pub const BILINEAR: u32 = 4;
    pub const MASK: u32 = 8;
    pub const COLORIZE: u32 = 16;
}

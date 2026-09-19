use std::collections::HashMap;

use blit_raster::{Metrics, Rasterizer};
use blit_text::{FontFaceId, TextLayoutEngine};

const INITIAL_SIZE: u32 = 512;

pub struct GlyphAtlas {
    layout: wgpu::BindGroupLayout,
    resource: Option<Resource>,
    glyphs: HashMap<GlyphKey, Glyph>,
    rasterizer: Rasterizer,
    x: u32,
    y: u32,
    row_height: u32,
    max_size: u32,
}

#[derive(Clone, Copy)]
pub struct Glyph {
    pub metrics: Metrics,
    pub atlas: [u32; 2],
}

struct Resource {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    face: FontFaceId,
    glyph: u16,
    size: u32,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device, layout: wgpu::BindGroupLayout) -> Self {
        Self {
            layout,
            resource: None,
            glyphs: HashMap::new(),
            rasterizer: Rasterizer::default(),
            x: 0,
            y: 0,
            row_height: 0,
            max_size: device.limits().max_texture_dimension_2d,
        }
    }

    pub fn glyph(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &dyn TextLayoutEngine,
        face: FontFaceId,
        glyph: u16,
        size: f32,
    ) -> Glyph {
        let key = GlyphKey {
            face,
            glyph,
            size: size.to_bits(),
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return *glyph;
        }
        let face = text
            .font_face(face)
            .expect("text backend returned an unknown font");
        let (metrics, alpha) = self.rasterizer.rasterize(face, glyph, size);
        let atlas = if metrics.width == 0 || metrics.height == 0 {
            [0, 0]
        } else {
            let width = u32::try_from(metrics.width).expect("glyph is too wide");
            let height = u32::try_from(metrics.height).expect("glyph is too tall");
            self.insert(device, queue, width, height, &alpha)
        };
        let glyph = Glyph { metrics, atlas };
        self.glyphs.insert(key, glyph);
        glyph
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self
            .resource
            .as_ref()
            .expect("text batch requires a glyph atlas")
            .bind_group
    }

    fn insert(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        alpha: &[u8],
    ) -> [u32; 2] {
        assert!(
            width <= self.max_size && height <= self.max_size,
            "glyph exceeds the GPU texture limit"
        );
        let mut size = self
            .resource
            .as_ref()
            .map_or(INITIAL_SIZE.min(self.max_size), |resource| resource.size);
        while width > size {
            let next = size.saturating_mul(2).min(self.max_size);
            assert!(next > size, "glyph atlas exhausted the GPU texture limit");
            size = next;
        }
        if self.x.checked_add(width).is_none_or(|right| right > size) {
            self.x = 0;
            self.y = self
                .y
                .checked_add(self.row_height)
                .expect("glyph atlas position overflow");
            self.row_height = 0;
        }
        let bottom = self
            .y
            .checked_add(height)
            .expect("glyph atlas position overflow");
        while bottom > size {
            let next = size.saturating_mul(2).min(self.max_size);
            assert!(next > size, "glyph atlas exhausted the GPU texture limit");
            size = next;
        }
        if self
            .resource
            .as_ref()
            .is_none_or(|resource| resource.size != size)
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("blit gpu glyph atlas"),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            if let Some(resource) = &self.resource {
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("blit gpu glyph atlas growth"),
                });
                encoder.copy_texture_to_texture(
                    resource.texture.as_image_copy(),
                    texture.as_image_copy(),
                    wgpu::Extent3d {
                        width: resource.size,
                        height: resource.size,
                        depth_or_array_layers: 1,
                    },
                );
                queue.submit([encoder.finish()]);
            }
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit gpu glyph atlas"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            self.resource = Some(Resource {
                texture,
                bind_group,
                size,
            });
        }

        let atlas = [self.x, self.y];
        let texture = &self.resource.as_ref().unwrap().texture;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: atlas[0],
                    y: atlas[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            alpha,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.x += width;
        self.row_height = self.row_height.max(height);
        atlas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_preserves_atlas_coordinates() {
        let (device, queue) = wgpu::Device::noop(&Default::default());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let mut atlas = GlyphAtlas::new(&device, layout);
        let alpha = vec![255; 400 * 400];

        assert_eq!(atlas.insert(&device, &queue, 400, 400, &alpha), [0, 0]);
        assert_eq!(atlas.insert(&device, &queue, 400, 400, &alpha), [0, 400]);
        assert_eq!(atlas.resource.as_ref().unwrap().size, 1024);
    }
}

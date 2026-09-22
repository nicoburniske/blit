use std::collections::HashMap;

use blit_gui::ResolvedTextLayout;
use blit_raster::{Metrics, Rasterizer};
use blit_text::FontFaceId;

use crate::atlas::{AllocId, Allocation, AtlasAllocator};

const PAGE_SIZE: u32 = 1024;
const CACHE_BYTES: u64 = 8 * 1024 * 1024;
const SIZE_QUANTIZATION: f32 = 8.0;

pub struct GlyphAtlas {
    layout: wgpu::BindGroupLayout,
    pages: Vec<Option<Page>>,
    glyphs: HashMap<GlyphKey, CachedGlyph>,
    rasterizer: Rasterizer,
    victims: Vec<(u64, GlyphKey)>,
    frame: u64,
    bytes: u64,
    max_size: u32,
    phase_scale: f32,
}

#[derive(Clone, Copy)]
pub struct Glyph {
    pub metrics: Metrics,
    pub atlas: [u32; 2],
    pub page: usize,
}

struct CachedGlyph {
    glyph: Glyph,
    allocation: Option<AllocId>,
    last_used: u64,
}

struct Page {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    allocator: AtlasAllocator,
    size: [u32; 2],
    last_used: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    face: FontFaceId,
    glyph: u16,
    size_eighths: u32,
    phase: u8,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device, layout: wgpu::BindGroupLayout, phase_scale: f32) -> Self {
        Self {
            layout,
            pages: Vec::new(),
            glyphs: HashMap::new(),
            rasterizer: Rasterizer::default(),
            victims: Vec::new(),
            frame: 0,
            bytes: 0,
            max_size: device.limits().max_texture_dimension_2d,
            phase_scale,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    pub fn end_frame(&mut self) {
        while self.bytes > CACHE_BYTES {
            let Some((index, _)) = self
                .pages
                .iter()
                .enumerate()
                .filter_map(|(index, page)| {
                    let page = page.as_ref()?;
                    (page.last_used != self.frame).then_some((index, page.last_used))
                })
                .min_by_key(|(_, last_used)| *last_used)
            else {
                break;
            };
            let page = self.pages[index].take().unwrap();
            self.bytes -= u64::from(page.size[0]) * u64::from(page.size[1]);
        }
        self.glyphs.retain(|_, cached| match cached.allocation {
            Some(_) => self.pages[cached.glyph.page].is_some(),
            None => cached.last_used == self.frame,
        });
        self.drop_empty_pages();
    }

    pub fn glyph(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &ResolvedTextLayout<'_>,
        face: FontFaceId,
        glyph: u16,
        size: f32,
        phase: u8,
    ) -> Glyph {
        let size_eighths = (size * SIZE_QUANTIZATION).round().max(0.0) as u32;
        let key = GlyphKey {
            face,
            glyph,
            size_eighths,
            phase,
        };
        if let Some(cached) = self.glyphs.get_mut(&key) {
            cached.last_used = self.frame;
            let glyph = cached.glyph;
            if cached.allocation.is_some() {
                self.pages[glyph.page].as_mut().unwrap().last_used = self.frame;
            }
            return glyph;
        }

        let face = text
            .font_face(face)
            .expect("text backend returned an unknown font");
        let size = size_eighths as f32 / SIZE_QUANTIZATION;
        let (metrics, alpha) =
            self.rasterizer
                .rasterize(face, glyph, size, phase as f32 * self.phase_scale);
        if metrics.width == 0 || metrics.height == 0 {
            let glyph = Glyph {
                metrics,
                atlas: [0, 0],
                page: 0,
            };
            self.glyphs.insert(
                key,
                CachedGlyph {
                    glyph,
                    allocation: None,
                    last_used: self.frame,
                },
            );
            return glyph;
        }

        let width = u32::try_from(metrics.width).expect("glyph is too wide");
        let height = u32::try_from(metrics.height).expect("glyph is too tall");
        assert!(
            width <= self.max_size && height <= self.max_size,
            "glyph exceeds the GPU texture limit"
        );
        let requested = [width, height];
        let base_size = PAGE_SIZE.min(self.max_size);
        let page_size = [base_size.max(width), base_size.max(height)];
        let mut placement = self.allocate(requested);
        let page_bytes = u64::from(page_size[0]) * u64::from(page_size[1]);
        let reusable = self
            .pages
            .iter()
            .flatten()
            .any(|page| width <= page.size[0] && height <= page.size[1]);

        if placement.is_none() && reusable && self.bytes.saturating_add(page_bytes) > CACHE_BYTES {
            self.victims.clear();
            self.victims
                .extend(self.glyphs.iter().filter_map(|(key, cached)| {
                    let page = cached.glyph.page;
                    (cached.last_used != self.frame
                        && cached.allocation.is_some()
                        && self.pages[page]
                            .as_ref()
                            .is_some_and(|page| width <= page.size[0] && height <= page.size[1]))
                    .then_some((cached.last_used, *key))
                }));
            self.victims.sort_unstable_by_key(|victim| victim.0);

            let mut index = 0;
            while placement.is_none() && index < self.victims.len() {
                let key = self.victims[index].1;
                index += 1;
                let cached = self.glyphs.remove(&key).unwrap();
                let allocation = cached.allocation.unwrap();
                self.pages[cached.glyph.page]
                    .as_mut()
                    .unwrap()
                    .allocator
                    .deallocate(allocation);
                placement = self.allocate(requested);
            }
        }

        let (page, allocation) = placement.unwrap_or_else(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("blit gpu glyph atlas"),
                size: wgpu::Extent3d {
                    width: page_size[0],
                    height: page_size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit gpu glyph atlas"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            let mut allocator = AtlasAllocator::new(page_size);
            let allocation = allocator.allocate(requested).unwrap();
            let page = self
                .pages
                .iter()
                .position(Option::is_none)
                .unwrap_or(self.pages.len());
            let resource = Page {
                texture,
                bind_group,
                allocator,
                size: page_size,
                last_used: self.frame,
            };
            if page == self.pages.len() {
                self.pages.push(Some(resource));
            } else {
                self.pages[page] = Some(resource);
            }
            self.bytes += page_bytes;
            (page, allocation)
        });

        self.pages[page].as_mut().unwrap().last_used = self.frame;
        self.drop_empty_pages();

        let atlas = allocation.position;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.pages[page].as_ref().unwrap().texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: atlas[0],
                    y: atlas[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &alpha,
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

        let glyph = Glyph {
            metrics,
            atlas,
            page,
        };
        self.glyphs.insert(
            key,
            CachedGlyph {
                glyph,
                allocation: Some(allocation.id),
                last_used: self.frame,
            },
        );
        glyph
    }

    pub fn bind_group(&self, page: usize) -> &wgpu::BindGroup {
        &self.pages[page]
            .as_ref()
            .expect("text batch references an empty glyph atlas page")
            .bind_group
    }

    fn allocate(&mut self, requested: [u32; 2]) -> Option<(usize, Allocation)> {
        self.pages
            .iter_mut()
            .enumerate()
            .filter_map(|(index, page)| Some((index, page.as_mut()?)))
            .find_map(|(index, page)| {
                page.allocator
                    .allocate(requested)
                    .map(|allocation| (index, allocation))
            })
    }

    fn drop_empty_pages(&mut self) {
        for page in &mut self.pages {
            if page.as_ref().is_some_and(|page| page.allocator.is_empty()) {
                let empty = page.take().unwrap();
                self.bytes -= u64::from(empty.size[0]) * u64::from(empty.size[1]);
            }
        }
        while self.pages.last().is_some_and(Option::is_none) {
            self.pages.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_overflow_and_empty_glyphs_are_reclaimed() {
        let (device, _) = wgpu::Device::noop(&Default::default());
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
        let mut atlas = GlyphAtlas::new(&device, layout.clone(), 0.25);
        atlas.begin_frame();

        let size = [PAGE_SIZE * 4; 2];
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        let mut allocator = AtlasAllocator::new(size);
        let allocation = allocator.allocate([1, 1]).unwrap();
        atlas.pages.push(Some(Page {
            texture,
            bind_group,
            allocator,
            size,
            last_used: atlas.frame,
        }));
        atlas.bytes = u64::from(size[0]) * u64::from(size[1]);
        atlas.glyphs.insert(
            GlyphKey {
                face: FontFaceId::default(),
                glyph: 1,
                size_eighths: 128,
                phase: 0,
            },
            CachedGlyph {
                glyph: Glyph {
                    metrics: Metrics {
                        width: 1,
                        height: 1,
                        ..Metrics::default()
                    },
                    atlas: [0, 0],
                    page: 0,
                },
                allocation: Some(allocation.id),
                last_used: atlas.frame,
            },
        );
        atlas.glyphs.insert(
            GlyphKey {
                face: FontFaceId::default(),
                glyph: 2,
                size_eighths: 128,
                phase: 0,
            },
            CachedGlyph {
                glyph: Glyph {
                    metrics: Metrics::default(),
                    atlas: [0, 0],
                    page: 0,
                },
                allocation: None,
                last_used: atlas.frame,
            },
        );

        atlas.end_frame();
        assert_eq!(atlas.pages.len(), 1);
        assert_eq!(atlas.glyphs.len(), 2);

        atlas.begin_frame();
        atlas.end_frame();
        assert!(atlas.pages.is_empty());
        assert!(atlas.glyphs.is_empty());
        assert_eq!(atlas.bytes, 0);
    }
}

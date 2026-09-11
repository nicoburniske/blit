//! shelf packed texture atlas for glyph coverage and image pixels

use std::{collections::HashMap, hash::Hash};

/// keeps neighbouring entries from bleeding into each other when filtered
const PADDING: u32 = 1;

pub struct Atlas<K> {
    view: wgpu::TextureView,
    texture: wgpu::Texture,
    entries: HashMap<K, [f32; 4]>,
    size: u32,
    bytes_per_texel: u32,
    x: u32,
    y: u32,
    shelf: u32,
}

impl<K: Copy + Eq + Hash> Atlas<K> {
    pub fn new(device: &wgpu::Device, label: &str, size: u32, format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            entries: HashMap::new(),
            size,
            bytes_per_texel: format.block_copy_size(None).expect("atlas texel size"),
            x: 0,
            y: 0,
            shelf: 0,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn size(&self) -> f32 {
        self.size as f32
    }

    /// returns the texel rect of `key`, uploading `pixels` when it is not resident
    pub fn insert(
        &mut self,
        queue: &wgpu::Queue,
        key: K,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Option<[f32; 4]> {
        if let Some(rect) = self.entries.get(&key) {
            return Some(*rect);
        }
        if width > self.size || height > self.size {
            return None;
        }
        if self.x + width > self.size {
            self.x = 0;
            self.y += self.shelf + PADDING;
            self.shelf = 0;
        }
        if self.y + height > self.size {
            return None;
        }
        let rect = [self.x as f32, self.y as f32, width as f32, height as f32];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: self.x,
                    y: self.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * self.bytes_per_texel),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.x += width + PADDING;
        self.shelf = self.shelf.max(height);
        self.entries.insert(key, rect);
        Some(rect)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.x = 0;
        self.y = 0;
        self.shelf = 0;
    }
}

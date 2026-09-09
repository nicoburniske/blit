use std::fmt::Write as _;

use base64::Engine as _;
use blit::LogicalRect;

use crate::{
    BASE64, KittyPlacement, TuiRenderer, image::ImagePlacement, text::TextAttributes, write_color,
};

impl TuiRenderer {
    pub fn begin_frame(&mut self) {
        self.needs_palette = false;
        self.output.clear();
        self.frame_cells.clear();
        self.kitty_placements.clear();
    }
    pub fn place_image(&mut self, request: ImagePlacement, clip: LogicalRect) {
        if let Some(area) = request.area.intersection(clip) {
            let x = area.x.floor().max(0.0) as usize;
            let y = area.y.floor().max(0.0) as usize;
            let right = (area.x + area.width).ceil().min(self.columns as f32) as usize;
            let bottom = (area.y + area.height).ceil().min(self.rows as f32) as usize;
            if right > x && bottom > y {
                self.kitty_placements.push(KittyPlacement {
                    image: request.image.0 as u32,
                    x: x as u16,
                    y: y as u16,
                    width: (right - x) as u16,
                    height: (bottom - y) as u16,
                });
            }
        }
    }

    #[inline]
    pub fn end_frame(&mut self) {
        for y in 0..self.rows {
            let range = y * self.columns..(y + 1) * self.columns;
            if !self.invalidated && self.cells.row_eq(&self.frame_cells, range.clone()) {
                self.changed[range].fill(false);
                continue;
            }
            for index in range {
                let old_glyph = crate::Glyph(self.cells.glyph[index]);
                let new_glyph = crate::Glyph(self.frame_cells.glyph[index]);
                self.changed[index] = self.invalidated
                    || !Self::glyphs_equal(&self.text_runs, old_glyph, new_glyph)
                    || self.cells.foreground[index] != self.frame_cells.foreground[index]
                    || self.cells.background[index] != self.frame_cells.background[index]
                    || self.cells.attributes[index] != self.frame_cells.attributes[index];
                let old = old_glyph.run().map(|(run, _)| run);
                let new = new_glyph.run().map(|(run, _)| run);
                if old != new {
                    if let Some(run) = new {
                        self.text_runs
                            .update_index(run as usize, |run| run.screen_references += 1);
                    }
                    if let Some(run) = old {
                        self.text_runs.update_index(run as usize, |run| {
                            run.screen_references -= 1;
                        });
                    }
                }
            }
        }
        self.invalidated = false;
        std::mem::swap(&mut self.cells, &mut self.frame_cells);

        let mut style = None;
        for y in 0..self.rows {
            let mut x = 0;
            while x < self.columns {
                let index = y * self.columns + x;
                if !self.changed[index] {
                    x += 1;
                    continue;
                }
                uwrite!(self.output, "\x1b[{};{}H", y + 1, x + 1);
                while x < self.columns {
                    let index = y * self.columns + x;
                    if !self.changed[index] {
                        break;
                    }
                    let next_style = (
                        self.cells.foreground[index],
                        self.cells.background[index],
                        self.cells.attributes[index],
                    );
                    if style != Some(next_style) {
                        if let Some((foreground, background, attributes)) = style
                            && attributes == next_style.2
                        {
                            self.output.push_str("\x1b[");
                            let mut separator = false;
                            if foreground != next_style.0 {
                                write_color(
                                    &mut self.output,
                                    crate::Color::from_packed(next_style.0),
                                    true,
                                );
                                separator = true;
                            }
                            if background != next_style.1 {
                                if separator {
                                    self.output.push(';');
                                }
                                write_color(
                                    &mut self.output,
                                    crate::Color::from_packed(next_style.1),
                                    false,
                                );
                            }
                        } else {
                            let attributes = TextAttributes(next_style.2);
                            self.output.push_str("\x1b[0");
                            // attribute bits map directly to sgr codes 1 through 9
                            for index in 0..9 {
                                if attributes.0 & 1 << index != 0 {
                                    uwrite!(self.output, ";{}", index + 1);
                                }
                            }
                            self.output.push(';');
                            write_color(
                                &mut self.output,
                                crate::Color::from_packed(next_style.0),
                                true,
                            );
                            self.output.push(';');
                            write_color(
                                &mut self.output,
                                crate::Color::from_packed(next_style.1),
                                false,
                            );
                        }
                        self.output.push('m');
                        style = Some(next_style);
                    }
                    Self::push_cell_text(
                        &self.text_runs,
                        crate::Glyph(self.cells.glyph[index]),
                        &mut self.output,
                    );
                    x += 1;
                }
            }
        }
        if style.is_some() {
            self.output.push_str("\x1b[0m");
        }

        for (index, placement) in self.presented_kitty_placements.iter().enumerate() {
            if self.kitty_placements.get(index) != Some(placement) {
                uwrite!(
                    self.output,
                    "\x1b_Ga=d,d=i,i={},p={},q=2\x1b\\",
                    placement.image,
                    index + 1
                );
            }
        }
        for (index, placement) in self.kitty_placements.iter().enumerate() {
            let image = self
                .images
                .iter_mut()
                .find(|image| image.handle.id().0 == u64::from(placement.image))
                .expect("invalid terminal image");
            if !image.transmitted {
                let size = image.handle.size();
                let bytes = image.pixels.bytes();
                for (index, chunk) in bytes.chunks(3072).enumerate() {
                    let more = usize::from((index + 1) * 3072 < bytes.len());
                    if index == 0 {
                        let format = match image.format {
                            crate::image::ImageFormat::Rgb8 => 24,
                            crate::image::ImageFormat::Rgba8 => 32,
                        };
                        uwrite!(
                            self.output,
                            "\x1b_Ga=t,f={format},s={},v={},i={},m={more},q=2;",
                            size.width,
                            size.height,
                            placement.image
                        );
                    } else {
                        uwrite!(self.output, "\x1b_Gm={more},q=2;");
                    }
                    BASE64.encode_string(chunk, &mut self.output);
                    self.output.push_str("\x1b\\");
                }
                image.transmitted = true;
            }
            if self.presented_kitty_placements.get(index) != Some(placement) {
                uwrite!(
                    self.output,
                    "\x1b[{};{}H\x1b_Ga=p,i={},p={},c={},r={},C=1,z=1,q=2\x1b\\",
                    placement.y + 1,
                    placement.x + 1,
                    placement.image,
                    index + 1,
                    placement.width,
                    placement.height,
                );
            }
        }
        let mut image = 0;
        while image < self.images.len() {
            let id = self.images[image].handle.id().0 as u32;
            if !self.images[image].handle.is_uniquely_owned()
                || self
                    .kitty_placements
                    .iter()
                    .any(|placement| placement.image == id)
            {
                image += 1;
                continue;
            }
            if self.images[image].transmitted {
                uwrite!(self.output, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
            }
            self.images.swap_remove(image);
        }
        std::mem::swap(
            &mut self.presented_kitty_placements,
            &mut self.kitty_placements,
        );
        self.text_layouts.trim_to_weight();
        self.text_runs
            .trim_to_weight_if(|_, run| run.screen_references == 0);
    }

    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }
}

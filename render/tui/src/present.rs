use std::fmt::Write as _;

use base64::Engine as _;
use blit::LogicalRect;

use crate::{
    BASE64, Cell, KittyPlacement, TuiRenderer, image::ImagePlacement, text::TextAttributes,
    write_color,
};

impl TuiRenderer {
    pub fn begin_frame(&mut self) {
        self.output.clear();
        self.frame_cells.fill(Cell::default());
        self.kitty_placements.clear();
        self.next_placement = 1;
    }
    pub fn place_image(&mut self, request: ImagePlacement, clip: LogicalRect) {
        if let Some(area) = request.area.intersection(clip) {
            let x = area.x.floor().max(0.0) as usize;
            let y = area.y.floor().max(0.0) as usize;
            let right = (area.x + area.width).ceil().min(self.columns as f32) as usize;
            let bottom = (area.y + area.height).ceil().min(self.rows as f32) as usize;
            if right > x && bottom > y {
                let id = self.next_placement;
                self.next_placement = self
                    .next_placement
                    .checked_add(1)
                    .expect("too many tui image placements");
                self.kitty_placements.push(KittyPlacement {
                    id,
                    image: request.image.0 as u32,
                    x,
                    y,
                    width: right - x,
                    height: bottom - y,
                });
            }
        }
    }

    #[inline]
    pub fn end_frame(&mut self) {
        for index in 0..self.cells.len() {
            let cell = self.frame_cells[index];
            self.changed[index] = self.cells[index] != cell;
            self.set_cell(index, cell);
        }

        let mut style = None;
        for y in 0..self.rows {
            let mut x = 0;
            while x < self.columns {
                let index = y * self.columns + x;
                if !self.changed[index] {
                    x += 1;
                    continue;
                }
                write!(self.output, "\x1b[{};{}H", y + 1, x + 1).unwrap();
                while x < self.columns {
                    let index = y * self.columns + x;
                    let cell = &self.cells[index];
                    if !self.changed[index] {
                        break;
                    }
                    let next_style = (cell.foreground, cell.background, cell.attributes);
                    if style != Some(next_style) {
                        if let Some((foreground, background, attributes)) = style
                            && attributes == cell.attributes
                        {
                            self.output.push_str("\x1b[");
                            let mut separator = false;
                            if foreground != cell.foreground {
                                write_color(&mut self.output, cell.foreground, true);
                                separator = true;
                            }
                            if background != cell.background {
                                if separator {
                                    self.output.push(';');
                                }
                                write_color(&mut self.output, cell.background, false);
                            }
                        } else {
                            self.output.push_str("\x1b[0");
                            if cell.attributes.contains(TextAttributes::BOLD) {
                                self.output.push_str(";1");
                            }
                            if cell.attributes.contains(TextAttributes::DIM) {
                                self.output.push_str(";2");
                            }
                            if cell.attributes.contains(TextAttributes::ITALIC) {
                                self.output.push_str(";3");
                            }
                            if cell.attributes.contains(TextAttributes::UNDERLINE) {
                                self.output.push_str(";4");
                            }
                            if cell.attributes.contains(TextAttributes::BLINK) {
                                self.output.push_str(";5");
                            }
                            if cell.attributes.contains(TextAttributes::INVERSE) {
                                self.output.push_str(";7");
                            }
                            if cell.attributes.contains(TextAttributes::HIDDEN) {
                                self.output.push_str(";8");
                            }
                            if cell.attributes.contains(TextAttributes::STRIKETHROUGH) {
                                self.output.push_str(";9");
                            }
                            self.output.push(';');
                            write_color(&mut self.output, cell.foreground, true);
                            self.output.push(';');
                            write_color(&mut self.output, cell.background, false);
                        }
                        self.output.push('m');
                        style = Some(next_style);
                    }
                    Self::push_cell_text(&self.text_runs, cell.text, &mut self.output);
                    x += 1;
                }
            }
        }
        if style.is_some() {
            self.output.push_str("\x1b[0m");
        }

        for placement in &self.presented_kitty_placements {
            if !self.kitty_placements.contains(placement) {
                write!(
                    self.output,
                    "\x1b_Ga=d,d=i,i={},p={},q=2\x1b\\",
                    placement.image, placement.id
                )
                .unwrap();
            }
        }
        for placement in &self.kitty_placements {
            let image = self
                .images
                .iter_mut()
                .find(|image| image.handle.id().0 == u64::from(placement.image))
                .expect("invalid terminal image");
            if !image.transmitted {
                for (index, chunk) in image.rgba.chunks(3072).enumerate() {
                    let more = usize::from((index + 1) * 3072 < image.rgba.len());
                    if index == 0 {
                        write!(
                            self.output,
                            "\x1b_Ga=t,f=32,s={},v={},i={},m={more},q=2;",
                            image.width, image.height, placement.image
                        )
                        .unwrap();
                    } else {
                        write!(self.output, "\x1b_Gm={more},q=2;").unwrap();
                    }
                    BASE64.encode_string(chunk, &mut self.output);
                    self.output.push_str("\x1b\\");
                }
                image.transmitted = true;
            }
            if !self.presented_kitty_placements.contains(placement) {
                write!(
                    self.output,
                    "\x1b[{};{}H\x1b_Ga=p,i={},p={},c={},r={},C=1,z=1,q=2\x1b\\",
                    placement.y + 1,
                    placement.x + 1,
                    placement.image,
                    placement.id,
                    placement.width,
                    placement.height,
                )
                .unwrap();
            }
        }
        self.presented_kitty_placements
            .clone_from(&self.kitty_placements);
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
                write!(self.output, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\").unwrap();
            }
            self.images.swap_remove(image);
            self.presented_kitty_placements
                .retain(|placement| placement.image != id);
        }
        self.text_layouts.trim_to_weight();
        self.text_runs
            .trim_to_weight_if(|_, run| run.screen_references == 0);
    }

    pub fn invalidate(&mut self) {
        for cell in &mut self.cells {
            cell.valid = false;
        }
    }
}

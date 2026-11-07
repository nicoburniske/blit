use super::*;

pub enum ReadRequest<'a> {
    Snapshot(&'a Region),
    Preserve(&'a Region),
}

pub struct Registry<P: Pixel> {
    entries: Vec<(TypeId, Box<dyn Erased<P>>)>,
    data: Arena,
    prepared: Vec<(usize, Value)>,
    snapshot: Surface<P>,
    preserved: Vec<P>,
}

impl<P: Pixel + 'static> Registry<P> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            data: Arena::default(),
            prepared: Vec::new(),
            snapshot: Surface {
                width: 0,
                height: 0,
                pixels: Vec::new(),
                valid: Region::default(),
            },
            preserved: Vec::new(),
        }
    }

    pub fn register<E: Extension<P>>(&mut self, extension: E) {
        assert!(
            !self.entries.iter().any(|(kind, _)| *kind == TypeId::of::<E::Command>()),
            "duplicate extension"
        );
        self.entries
            .push((TypeId::of::<E::Command>(), Box::new(Adapter { extension })));
    }

    pub fn read_bounds(&self, command: Custom<'_>, output: PhysicalRect, scale: f32) -> Option<PhysicalRect> {
        self.entries
            .iter()
            .find(|(kind, _)| *kind == command.type_id())
            .unwrap_or_else(|| panic!("no cpu extension registered for {}", command.type_name()))
            .1
            .read_bounds(command, output, scale)
    }

    pub fn prepare(
        &mut self,
        command: Custom<'_>,
        scale: f32,
        bounds: PhysicalRect,
        images: &mut ImageArena<StoredImage>,
    ) -> u32 {
        let index = self
            .entries
            .iter()
            .position(|(kind, _)| *kind == command.type_id())
            .unwrap();
        let value = self.entries[index].1.prepare(
            command,
            &mut PrepareContext {
                scale,
                bounds,
                images,
                data: self.data.writer(),
            },
        );
        let id = u32::try_from(self.prepared.len()).expect("too many custom primitives");
        self.prepared.push((index, value));
        id
    }

    pub fn raster(
        &mut self,
        id: u32,
        x: i32,
        y: i32,
        pixels: &mut [P],
        coverage: u8,
        images: &ImageArena<StoredImage>,
    ) {
        let (entry, value) = self.prepared[id as usize];
        self.entries[entry].1.render(
            value,
            &self.data,
            &mut DrawContext {
                source: None,
                target: DrawTarget::Span(Span {
                    x,
                    y,
                    pixels,
                    coverage,
                    data: &self.data,
                    images,
                }),
            },
        );
    }

    pub fn read<B: PixelBuffer<Pixel = P>>(&mut self, buffer: &mut B, request: ReadRequest<'_>) {
        assert_eq!(buffer.x_offset(), 0, "source reads require a complete framebuffer");
        match request {
            ReadRequest::Snapshot(region) => {
                self.snapshot.width = buffer.width();
                self.snapshot.height = buffer.height();
                self.snapshot
                    .pixels
                    .resize(buffer.width() * buffer.height(), P::background());
                self.snapshot.valid.clone_from(region);
                for bounds in region.rects() {
                    for y in bounds.y..bounds.y + bounds.height {
                        let range = bounds.x as usize..(bounds.x + bounds.width) as usize;
                        let start = y as usize * buffer.width() + range.start;
                        self.snapshot.pixels[start..start + range.len()]
                            .copy_from_slice(buffer.read_line(y as usize, range));
                    }
                }
            }
            ReadRequest::Preserve(region) => {
                self.preserved.clear();
                for bounds in region.rects() {
                    for y in bounds.y..bounds.y + bounds.height {
                        self.preserved.extend_from_slice(
                            buffer.read_line(y as usize, bounds.x as usize..(bounds.x + bounds.width) as usize),
                        );
                    }
                }
            }
        }
    }

    pub fn restore<B: PixelBuffer<Pixel = P>>(&self, buffer: &mut B, region: &Region) {
        let mut offset = 0;
        for bounds in region.rects() {
            for y in bounds.y..bounds.y + bounds.height {
                buffer.process_line(
                    y as usize,
                    bounds.x as usize..(bounds.x + bounds.width) as usize,
                    |pixels| {
                        pixels.copy_from_slice(&self.preserved[offset..offset + pixels.len()]);
                        offset += pixels.len();
                    },
                );
            }
        }
    }

    pub fn pass<B: PixelBuffer<Pixel = P>>(
        &mut self,
        id: u32,
        buffer: &mut B,
        clips: &ClipStack,
        bounds: PhysicalRect,
        clip: u32,
        images: &ImageArena<StoredImage>,
        output: &Region,
    ) {
        let (entry, value) = self.prepared[id as usize];
        self.entries[entry].1.render(
            value,
            &self.data,
            &mut DrawContext {
                source: Some(&self.snapshot),
                target: DrawTarget::Pass {
                    target: buffer,
                    clips,
                    bounds,
                    clip,
                    data: &self.data,
                    images,
                    output,
                },
            },
        );
    }

    pub fn finish_frame(&mut self) {
        self.prepared.clear();
        self.data.clear();
        for (_, entry) in &mut self.entries {
            entry.finish_frame();
        }
    }
}

trait Erased<P> {
    fn read_bounds(&self, command: Custom<'_>, output: PhysicalRect, scale: f32) -> Option<PhysicalRect>;
    fn prepare(&mut self, command: Custom<'_>, cx: &mut PrepareContext<'_>) -> Value;
    fn render(&mut self, prepared: Value, data: &Arena, cx: &mut DrawContext<'_, P>);
    fn finish_frame(&mut self);
}

struct Adapter<E> {
    extension: E,
}

impl<E: Extension<P>, P: Pixel> Erased<P> for Adapter<E> {
    fn read_bounds(&self, command: Custom<'_>, output: PhysicalRect, scale: f32) -> Option<PhysicalRect> {
        self.extension
            .read_bounds(command.get::<E::Command>().unwrap(), output, scale)
    }

    fn prepare(&mut self, command: Custom<'_>, cx: &mut PrepareContext<'_>) -> Value {
        let prepared = self.extension.prepare(command.get::<E::Command>().unwrap(), cx);
        cx.data.push(prepared).erase()
    }

    fn render(&mut self, prepared: Value, data: &Arena, cx: &mut DrawContext<'_, P>) {
        self.extension.render(data.value(prepared).unwrap(), cx);
    }

    fn finish_frame(&mut self) {
        self.extension.finish_frame();
    }
}

use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use blit_profile::{FrameId, FrameProfiler, StageId};

const SLOTS: usize = 4;
const MAX_PASSES: u32 = 16;
const QUERIES_PER_SLOT: u32 = MAX_PASSES * 2;
const SLOT_BYTES: u64 = QUERIES_PER_SLOT as u64 * size_of::<u64>() as u64;

const MAP_PENDING: u8 = 0;
const MAP_OK: u8 = 1;
const MAP_ERR: u8 = 2;

pub struct Timestamps {
    query_set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: [Slot; SLOTS],
}

impl Timestamps {
    pub fn new(device: &wgpu::Device) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("blit gpu timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: QUERIES_PER_SLOT * SLOTS as u32,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu timestamp resolve"),
            size: SLOT_BYTES * SLOTS as u64,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = std::array::from_fn(|_| Slot {
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("blit gpu timestamp readback"),
                size: SLOT_BYTES,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            mapped: Arc::new(AtomicU8::new(MAP_PENDING)),
            pending: None,
        });
        Some(Self {
            query_set,
            resolve,
            slots,
        })
    }

    /// drains completed GPU timestamp readbacks into `profiler`
    pub fn poll(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, profiler: &mut FrameProfiler, stage: StageId) {
        if self.slots.iter().all(|slot| slot.pending.is_none()) {
            return;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let period = f64::from(queue.get_timestamp_period());
        for slot in &mut self.slots {
            let Some((frame, bytes)) = slot.pending else {
                continue;
            };
            match slot.mapped.load(Ordering::Acquire) {
                MAP_PENDING => {}
                MAP_OK => {
                    if let Ok(view) = slot.readback.slice(..bytes).get_mapped_range() {
                        let ticks: &[u64] = bytemuck::cast_slice(&view);
                        if let (Some(&first), Some(&last)) = (ticks.first(), ticks.last()) {
                            let nanos = (last.saturating_sub(first) as f64 * period) as u64;
                            profiler.complete(frame, stage, Duration::from_nanos(nanos));
                        }
                    }
                    slot.readback.unmap();
                    slot.pending = None;
                }
                _ => {
                    slot.pending = None;
                }
            }
        }
    }

    pub fn acquire(&self, frame: FrameId) -> Option<ActivePass> {
        let slot = self.slots.iter().position(|slot| slot.pending.is_none())?;
        Some(ActivePass {
            query_set: self.query_set.clone(),
            slot,
            frame,
            passes: 0,
        })
    }

    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder, timing: &ActivePass) {
        let base = timing.slot as u32 * QUERIES_PER_SLOT;
        let offset = timing.slot as u64 * SLOT_BYTES;
        let bytes = timing.bytes();
        encoder.resolve_query_set(&self.query_set, base..base + timing.passes * 2, &self.resolve, offset);
        encoder.copy_buffer_to_buffer(&self.resolve, offset, &self.slots[timing.slot].readback, 0, bytes);
    }

    pub fn submit(&mut self, timing: ActivePass) {
        let bytes = timing.bytes();
        let slot = &mut self.slots[timing.slot];
        slot.mapped.store(MAP_PENDING, Ordering::Relaxed);
        let mapped = Arc::clone(&slot.mapped);
        slot.readback
            .slice(..bytes)
            .map_async(wgpu::MapMode::Read, move |result| {
                mapped.store(if result.is_ok() { MAP_OK } else { MAP_ERR }, Ordering::Release);
            });
        slot.pending = Some((timing.frame, bytes));
    }
}

pub struct ActivePass {
    query_set: wgpu::QuerySet,
    slot: usize,
    frame: FrameId,
    passes: u32,
}

impl ActivePass {
    pub fn next(&mut self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        if self.passes >= MAX_PASSES {
            return None;
        }
        let start = self.slot as u32 * QUERIES_PER_SLOT + self.passes * 2;
        self.passes += 1;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(start),
            end_of_pass_write_index: Some(start + 1),
        })
    }

    fn bytes(&self) -> u64 {
        u64::from(self.passes) * 2 * size_of::<u64>() as u64
    }
}

struct Slot {
    readback: wgpu::Buffer,
    mapped: Arc<AtomicU8>,
    pending: Option<(FrameId, u64)>,
}

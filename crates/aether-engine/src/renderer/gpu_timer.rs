//! GPU timer for per-pass and whole-frame timing.
//!
//! Uses `wgpu::QuerySet` with timestamp queries. Results are double-buffered
//! so the previous frame's timings can be read back without stalling the GPU.
//!
//! If the adapter does not support `TIMESTAMP_QUERY`, `GpuTimer::new` returns
//! `None` and callers should fall back to CPU timing.

/// Number of timestamp queries reserved for frame start/end.
const FRAME_QUERIES: u32 = 2;

/// GPU timer state.
pub struct GpuTimer {
    query_set: wgpu::QuerySet,
    /// Resolve buffers: one per double-buffered frame.
    resolve_buffers: [wgpu::Buffer; 2],
    /// CPU-readable copy of the previous frame's resolve buffer.
    readback_buffers: [wgpu::Buffer; 2],
    /// Total number of queries (frame start/end + 2 per pass).
    capacity: u32,
    /// Number of passes tracked.
    pass_count: usize,
    /// Which double-buffer slot is being written this frame.
    frame_index: usize,
    /// Tracks whether each readback buffer has been populated at least once.
    resolved: [bool; 2],
    /// Whether timestamp queries are supported by the adapter.
    pub supported: bool,
    /// Latest per-pass timings in milliseconds. Index 0 is the whole frame;
    /// index `1 + i` is pass `i`.
    pub last_frame_ms: Vec<f64>,
    /// Pass names in the same order as `last_frame_ms[1..]`.
    pub pass_names: Vec<String>,
}

impl GpuTimer {
    /// Create a new GPU timer for the given number of passes.
    ///
    /// Returns `None` if the device does not support timestamp queries.
    pub fn new(device: &wgpu::Device, pass_names: &[String]) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }

        let pass_count = pass_names.len();
        let capacity = FRAME_QUERIES + (pass_count as u32) * 2;

        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("GpuTimer QuerySet"),
            ty: wgpu::QueryType::Timestamp,
            count: capacity,
        });

        let buffer_size = (capacity as u64) * std::mem::size_of::<u64>() as u64;
        let resolve_buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuTimer Resolve 0"),
                size: buffer_size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuTimer Resolve 1"),
                size: buffer_size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
        ];
        let readback_buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuTimer Readback 0"),
                size: buffer_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GpuTimer Readback 1"),
                size: buffer_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];

        Some(Self {
            query_set,
            resolve_buffers,
            readback_buffers,
            capacity,
            pass_count,
            frame_index: 0,
            resolved: [false; 2],
            supported: true,
            last_frame_ms: vec![0.0; 1 + pass_count],
            pass_names: pass_names.to_vec(),
        })
    }

    /// Access the underlying query set.
    pub fn query_set(&self) -> &wgpu::QuerySet {
        &self.query_set
    }

    /// Query index for the frame-start timestamp.
    pub fn frame_start_index(&self) -> u32 {
        0
    }

    /// Query index for the frame-end timestamp.
    pub fn frame_end_index(&self) -> u32 {
        1
    }

    /// Query index for the start timestamp of pass `i`.
    pub fn pass_start_index(&self, i: usize) -> u32 {
        FRAME_QUERIES + (i as u32) * 2
    }

    /// Query index for the end timestamp of pass `i`.
    pub fn pass_end_index(&self, i: usize) -> u32 {
        FRAME_QUERIES + (i as u32) * 2 + 1
    }

    /// Resolve the current frame's query results and copy them to the
    /// readback buffer for later CPU reading.
    ///
    /// Call this after all timestamps have been written and before submitting
    /// the encoder.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let resolve = &self.resolve_buffers[self.frame_index];
        encoder.resolve_query_set(&self.query_set, 0..self.capacity, resolve, 0);
        encoder.copy_buffer_to_buffer(
            resolve,
            0,
            &self.readback_buffers[self.frame_index],
            0,
            (self.capacity as u64) * std::mem::size_of::<u64>() as u64,
        );
        self.resolved[self.frame_index] = true;
    }

    /// Read back the previous frame's timings.
    ///
    /// Call once per frame after submitting the command encoder that contained
    /// `resolve()`.
    pub fn poll_results(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let read_idx = 1 - self.frame_index;
        if !self.resolved[read_idx] {
            self.frame_index = read_idx;
            return;
        }
        let buf = &self.readback_buffers[read_idx];
        let slice = buf.slice(..);

        slice.map_async(wgpu::MapMode::Read, |result| {
            if let Err(e) = result {
                tracing::error!("GpuTimer map_async failed: {:?}", e);
            }
        });
        if let Err(e) = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        }) {
            tracing::error!("GpuTimer poll failed: {:?}", e);
        }

        {
            let view = slice.get_mapped_range();
            let data: &[u64] = bytemuck::cast_slice(&view);
            let period_ns = queue.get_timestamp_period() as f64;

            self.last_frame_ms.resize(1 + self.pass_count, 0.0);
            let frame_delta = (data[1].saturating_sub(data[0])) as f64 * period_ns / 1_000_000.0;
            self.last_frame_ms[0] = frame_delta;

            for i in 0..self.pass_count {
                let start = data[(FRAME_QUERIES + i as u32 * 2) as usize];
                let end = data[(FRAME_QUERIES + i as u32 * 2 + 1) as usize];
                let delta = (end.saturating_sub(start)) as f64 * period_ns / 1_000_000.0;
                self.last_frame_ms[1 + i] = delta;
            }
        }

        buf.unmap();
        self.frame_index = 1 - self.frame_index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::headless_device_queue;

    #[test]
    fn timer_capacity_matches_pass_count() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let Some(timer) = GpuTimer::new(&device, &["A".into(), "B".into()]) else {
            // Timestamp queries not supported on this adapter — skip.
            eprintln!("SKIP: timestamp queries not supported on this adapter");
            return;
        };
        assert_eq!(timer.capacity, 2 + 2 * 2);
        assert_eq!(timer.pass_start_index(0), 2);
        assert_eq!(timer.pass_end_index(0), 3);
        assert_eq!(timer.pass_start_index(1), 4);
        assert_eq!(timer.pass_end_index(1), 5);
    }

    #[test]
    fn timer_query_indices_are_monotonic() {
        let Some((device, _queue)) = headless_device_queue() else {
            eprintln!("SKIP: no GPU adapter available");
            return;
        };
        let Some(timer) = GpuTimer::new(&device, &["P0".into(), "P1".into(), "P2".into()]) else {
            eprintln!("SKIP: timestamp queries not supported on this adapter");
            return;
        };
        let mut indices = vec![timer.frame_start_index(), timer.frame_end_index()];
        for i in 0..timer.pass_count {
            indices.push(timer.pass_start_index(i));
            indices.push(timer.pass_end_index(i));
        }
        for window in indices.windows(2) {
            assert!(window[0] < window[1]);
        }
    }
}

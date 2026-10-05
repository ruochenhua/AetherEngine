//! Research-only two-pass tiled occlusion prototype for roadmap slice T9.4.
//!
//! This example is intentionally outside the engine and launcher runtime paths.

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

const TILE_WIDTH: u32 = 48;
const TILE_HEIGHT: u32 = 27;
const TILE_COUNT: u32 = TILE_WIDTH * TILE_HEIGHT;
const CANDIDATE_COUNT: u32 = 10_000;
const WARMUP_COUNT: u32 = 30;
const SAMPLE_COUNT: u32 = 120;
const SEED: u32 = 9004;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Candidate {
    tile: u32,
    depth_milli: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Deserialize, PartialEq)]
struct Occluder {
    x_min: u32,
    y_min: u32,
    x_max: u32,
    y_max: u32,
    depth_milli: u32,
}

#[derive(Deserialize)]
struct Fixture {
    case_id: String,
    seed: u32,
    tile_width: u32,
    tile_height: u32,
    candidate_count: u32,
    warmup: u32,
    samples: u32,
    occluders: Vec<Occluder>,
}

const OCCLUDERS: [Occluder; 8] = [
    Occluder {
        x_min: 3,
        y_min: 2,
        x_max: 14,
        y_max: 8,
        depth_milli: 400,
    },
    Occluder {
        x_min: 17,
        y_min: 1,
        x_max: 28,
        y_max: 7,
        depth_milli: 360,
    },
    Occluder {
        x_min: 32,
        y_min: 4,
        x_max: 44,
        y_max: 10,
        depth_milli: 430,
    },
    Occluder {
        x_min: 6,
        y_min: 12,
        x_max: 18,
        y_max: 19,
        depth_milli: 380,
    },
    Occluder {
        x_min: 22,
        y_min: 10,
        x_max: 34,
        y_max: 17,
        depth_milli: 410,
    },
    Occluder {
        x_min: 38,
        y_min: 14,
        x_max: 46,
        y_max: 22,
        depth_milli: 350,
    },
    Occluder {
        x_min: 2,
        y_min: 21,
        x_max: 12,
        y_max: 26,
        depth_milli: 390,
    },
    Occluder {
        x_min: 18,
        y_min: 20,
        x_max: 30,
        y_max: 26,
        depth_milli: 420,
    },
];

const SHADER: &str = include_str!("t9_occlusion_streaming_prototype.wgsl");

fn candidates() -> Vec<Candidate> {
    let mut state = SEED;
    (0..CANDIDATE_COUNT)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let tile = state % TILE_COUNT;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            Candidate {
                tile,
                depth_milli: 200 + state % 801,
            }
        })
        .collect()
}

fn cpu_reference(candidates: &[Candidate]) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let mut depths = vec![1001u32; TILE_COUNT as usize];
    for y in 0..TILE_HEIGHT {
        for x in 0..TILE_WIDTH {
            let tile = (y * TILE_WIDTH + x) as usize;
            for rect in OCCLUDERS {
                if x >= rect.x_min && x < rect.x_max && y >= rect.y_min && y < rect.y_max {
                    depths[tile] = depths[tile].min(rect.depth_milli);
                }
            }
        }
    }
    let mask: Vec<u32> = candidates
        .iter()
        .map(|candidate| u32::from(candidate.depth_milli <= depths[candidate.tile as usize]))
        .collect();
    let mut totals = vec![0u32; TILE_COUNT as usize];
    let mut visible = vec![0u32; TILE_COUNT as usize];
    for (candidate, shown) in candidates.iter().zip(&mask) {
        totals[candidate.tile as usize] += 1;
        visible[candidate.tile as usize] += shown;
    }
    (mask, totals, visible)
}

fn unavailable_report(reason: &str, totals: &[u32], visible: &[u32]) -> serde_json::Value {
    json!({
        "schema_version": 1,
        "research_only": true,
        "prototype_enabled": true,
        "kill_switch": {"production_path_registered": false, "default": "off", "scope": "example-only"},
        "occlusion": {
            "status": "Unavailable",
            "unavailable_reason": reason,
            "timer_source": "wgpu timestamp query",
            "gpu_samples_ms": [],
            "warmup": WARMUP_COUNT,
            "samples": SAMPLE_COUNT,
            "candidate_count": CANDIDATE_COUNT,
            "visible_count": visible.iter().sum::<u32>(),
            "culled_count": CANDIDATE_COUNT - visible.iter().sum::<u32>(),
            "candidate_reduction": 1.0 - visible.iter().sum::<u32>() as f64 / CANDIDATE_COUNT as f64,
            "cpu_gpu_mask_mismatches": null,
            "tile_width": TILE_WIDTH,
            "tile_height": TILE_HEIGHT,
            "tile_totals": totals,
            "tile_visible": visible,
            "known_gpu_buffer_bytes": null,
            "excluded_memory": "driver-managed query set and pipeline allocations"
        }
    })
}

fn gpu_run() -> Result<serde_json::Value> {
    let candidates = candidates();
    let (cpu_mask, tile_totals, tile_visible) = cpu_reference(&candidates);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        {
            Ok(adapter) => adapter,
            Err(error) => {
                return Ok(unavailable_report(
                    &format!("no headless adapter: {error}"),
                    &tile_totals,
                    &tile_visible,
                ))
            }
        };
    let info = adapter.get_info();
    let timestamp_features =
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    let supports_timestamps = adapter.features().contains(timestamp_features);
    let requested_features = if supports_timestamps {
        timestamp_features
    } else {
        wgpu::Features::empty()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("T9.4 isolated occlusion prototype"),
        required_features: requested_features,
        ..Default::default()
    }))
    .context("request isolated research device")?;
    if !supports_timestamps {
        return Ok(unavailable_report(
            "adapter does not expose TIMESTAMP_QUERY",
            &tile_totals,
            &tile_visible,
        ));
    }

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("T9.4 tiled occlusion research shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let build_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("T9.4 build tile depth"),
        layout: None,
        module: &shader,
        entry_point: Some("build_depth"),
        compilation_options: Default::default(),
        cache: None,
    });
    let classify_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("T9.4 classify candidates"),
        layout: None,
        module: &shader,
        entry_point: Some("classify"),
        compilation_options: Default::default(),
        cache: None,
    });
    let candidate_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T9.4 candidate input"),
        size: std::mem::size_of_val(candidates.as_slice()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&candidate_buffer, 0, bytemuck::cast_slice(&candidates));
    let occluder_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T9.4 occluder rectangles"),
        size: std::mem::size_of_val(&OCCLUDERS) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&occluder_buffer, 0, bytemuck::cast_slice(&OCCLUDERS));
    let tile_depth_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T9.4 tile depth"),
        size: (TILE_COUNT * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let visibility_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T9.4 visibility result"),
        size: (CANDIDATE_COUNT * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let output_readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T9.4 visibility readback"),
        size: (CANDIDATE_COUNT * 4) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let query_count = (WARMUP_COUNT + SAMPLE_COUNT) * 2;
    let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("T9.4 prototype timestamps"),
        ty: wgpu::QueryType::Timestamp,
        count: query_count,
    });
    let query_bytes = query_count as u64 * std::mem::size_of::<u64>() as u64;
    let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T9.4 timestamp resolve"),
        size: query_bytes,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let query_readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("T9.4 timestamp readback"),
        size: query_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let build_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("T9.4 depth bindings"),
        layout: &build_pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: occluder_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: tile_depth_buffer.as_entire_binding(),
            },
        ],
    });
    let classify_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("T9.4 candidate bindings"),
        layout: &classify_pipeline.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: candidate_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: tile_depth_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: visibility_buffer.as_entire_binding(),
            },
        ],
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("T9.4 benchmark encoder"),
    });
    for run in 0..(WARMUP_COUNT + SAMPLE_COUNT) {
        let query_base = run * 2;
        encoder.write_timestamp(&query_set, query_base);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("T9.4 two-pass cull"),
                ..Default::default()
            });
            pass.set_pipeline(&build_pipeline);
            pass.set_bind_group(0, &build_group, &[]);
            pass.dispatch_workgroups(TILE_COUNT.div_ceil(64), 1, 1);
            pass.set_pipeline(&classify_pipeline);
            pass.set_bind_group(1, &classify_group, &[]);
            pass.dispatch_workgroups(CANDIDATE_COUNT.div_ceil(64), 1, 1);
        }
        encoder.write_timestamp(&query_set, query_base + 1);
    }
    encoder.resolve_query_set(&query_set, 0..query_count, &resolve_buffer, 0);
    encoder.copy_buffer_to_buffer(&resolve_buffer, 0, &query_readback, 0, query_bytes);
    encoder.copy_buffer_to_buffer(
        &visibility_buffer,
        0,
        &output_readback,
        0,
        (CANDIDATE_COUNT * 4) as u64,
    );
    queue.submit(std::iter::once(encoder.finish()));

    let query_slice = query_readback.slice(..);
    let output_slice = output_readback.slice(..);
    let (query_tx, query_rx) = std::sync::mpsc::channel();
    let (output_tx, output_rx) = std::sync::mpsc::channel();
    query_slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = query_tx.send(result);
    });
    output_slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = output_tx.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .context("wait for prototype GPU work")?;
    query_rx.recv().context("timestamp map callback")??;
    output_rx.recv().context("visibility map callback")??;
    let ticks: Vec<u64> = bytemuck::cast_slice(&query_slice.get_mapped_range()).to_vec();
    let gpu_mask: Vec<u32> = bytemuck::cast_slice(&output_slice.get_mapped_range()).to_vec();
    query_readback.unmap();
    output_readback.unmap();
    let mismatch_count = cpu_mask
        .iter()
        .zip(&gpu_mask)
        .filter(|(cpu, gpu)| cpu != gpu)
        .count();
    let gpu_visible_count = gpu_mask.iter().sum::<u32>();
    let timestamp_period_ns = queue.get_timestamp_period() as f64;
    let gpu_samples_ms: Vec<f64> = (WARMUP_COUNT..WARMUP_COUNT + SAMPLE_COUNT)
        .map(|run| {
            ticks[(run * 2 + 1) as usize].saturating_sub(ticks[(run * 2) as usize]) as f64
                * timestamp_period_ns
                / 1_000_000.0
        })
        .collect();
    let known_gpu_buffer_bytes = (std::mem::size_of_val(candidates.as_slice())
        + std::mem::size_of_val(&OCCLUDERS)
        + (TILE_COUNT * 4) as usize
        + (CANDIDATE_COUNT * 4) as usize * 2
        + query_bytes as usize * 2) as u64;
    Ok(json!({
        "schema_version": 1,
        "research_only": true,
        "prototype_enabled": true,
        "kill_switch": {"production_path_registered": false, "default": "off", "scope": "example-only"},
        "occlusion": {
            "status": "Complete",
            "adapter": info.name,
            "backend": format!("{:?}", info.backend),
            "device_type": format!("{:?}", info.device_type),
            "timer_source": "wgpu TIMESTAMP_QUERY around two compute passes",
            "warmup": WARMUP_COUNT,
            "samples": SAMPLE_COUNT,
            "gpu_samples_ms": gpu_samples_ms,
            "timestamp_period_ns": timestamp_period_ns,
            "candidate_count": CANDIDATE_COUNT,
            "visible_count": gpu_visible_count,
            "culled_count": CANDIDATE_COUNT - gpu_visible_count,
            "candidate_reduction": 1.0 - gpu_visible_count as f64 / CANDIDATE_COUNT as f64,
            "cpu_gpu_mask_mismatches": mismatch_count,
            "tile_width": TILE_WIDTH,
            "tile_height": TILE_HEIGHT,
            "tile_totals": tile_totals,
            "tile_visible": tile_visible,
            "known_gpu_buffer_bytes": known_gpu_buffer_bytes,
            "memory_scope": "known wgpu buffers only; driver-managed query set, pipeline allocations and allocator overhead excluded",
            "excluded_memory": "driver-managed query set and pipeline allocations"
        }
    }))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut output = None;
    let mut fixture_path = None;
    let mut enabled = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--output" => output = args.next().map(PathBuf::from),
            "--fixture" => fixture_path = args.next().map(PathBuf::from),
            "--prototype-enabled" => enabled = true,
            other => anyhow::bail!("unexpected argument: {other}"),
        }
    }
    let output = output.context("--output <path> is required")?;
    let fixture_path = fixture_path.context("--fixture <path> is required")?;
    let fixture_text = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("read fixture {}", fixture_path.display()))?;
    let fixture: Fixture = ron::from_str(&fixture_text).context("parse T9.4 fixture")?;
    anyhow::ensure!(
        fixture.case_id == "t9_occlusion_streaming_research",
        "unexpected T9.4 case id"
    );
    anyhow::ensure!(
        fixture.seed == SEED,
        "fixture seed differs from prototype seed"
    );
    anyhow::ensure!(
        fixture.tile_width == TILE_WIDTH && fixture.tile_height == TILE_HEIGHT,
        "fixture tile dimensions differ from shader contract"
    );
    anyhow::ensure!(
        fixture.candidate_count == CANDIDATE_COUNT,
        "fixture candidate count differs from shader contract"
    );
    anyhow::ensure!(
        fixture.warmup == WARMUP_COUNT && fixture.samples == SAMPLE_COUNT,
        "fixture sample window differs from benchmark contract"
    );
    anyhow::ensure!(
        fixture.occluders == OCCLUDERS,
        "fixture occluders differ from prototype inputs"
    );
    let result = if enabled {
        gpu_run()?
    } else {
        json!({
            "schema_version": 1,
            "research_only": true,
            "prototype_enabled": false,
            "kill_switch": {"production_path_registered": false, "default": "off", "scope": "example-only"},
            "occlusion": {"status": "Disabled", "gpu_samples_ms": [], "known_gpu_buffer_bytes": null}
        })
    };
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(
        &output,
        format!("{}\n", serde_json::to_string_pretty(&result)?),
    )
    .with_context(|| format!("write {}", output.display()))?;
    println!("prototype_report={}", output.display());
    println!("prototype_enabled={enabled}");
    Ok(())
}

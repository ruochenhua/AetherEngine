//! Typed launcher argument parser implementation.

use super::CliArgs;
use aether_engine::time::{TimeControl, TimeMode};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CliError {
    message: String,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub(crate) fn parse_args(args: &[String]) -> Result<CliArgs, CliError> {
    let mut cli = CliArgs {
        scene: None,
        screenshot: None,
        render_graph_output: None,
        exit_after_frames: None,
        no_gui_overlay: false,
        debug_mode: None,
        freeze_time: false,
        ssao_enabled: false,
        no_ibl: false,
        local_lights_enabled: true,
        ssr_enabled: false,
        transparent_enabled: false,
        particles_enabled: false,
        material_extraction_enabled: true,
        animation_enabled: false,
        debug_helpers_enabled: true,
        physics_debug_enabled: false,
        width: None,
        height: None,
        time: TimeControl::default(),
        benchmark: None,
    };
    let mut time_mode = None;
    let mut simulation_time = None;
    let mut fixed_dt = None;
    let mut max_substeps = None;
    let mut max_seek_steps = None;
    let mut benchmark_case_path = None;
    let mut benchmark_output_path = None;
    let mut benchmark_visibility_output_path = None;
    let mut benchmark_commit = None;
    let mut benchmark_source_fingerprint = None;
    let mut benchmark_source_dirty = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scene" => {
                i += 1;
                if i < args.len() {
                    cli.scene = Some(args[i].clone());
                }
            }
            "--animation" => {
                let value = next_value(args, &mut i, "--animation")?;
                cli.animation_enabled = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(error(format!("unsupported --animation value {value}"))),
                };
            }
            "--screenshot" => {
                i += 1;
                if i < args.len() {
                    cli.screenshot = Some(PathBuf::from(&args[i]));
                }
            }
            "--render-graph-output" => {
                reject_duplicate(&cli.render_graph_output, "--render-graph-output")?;
                i += 1;
                if i < args.len() {
                    cli.render_graph_output = Some(PathBuf::from(&args[i]));
                } else {
                    return Err(error("--render-graph-output requires a path"));
                }
            }
            "--exit-after-frames" => {
                i += 1;
                if i < args.len() {
                    cli.exit_after_frames = args[i].parse().ok();
                }
            }
            "--no-gui-overlay" => cli.no_gui_overlay = true,
            "--no-debug-helpers" => cli.debug_helpers_enabled = false,
            "--physics-debug" => {
                let value = next_value(args, &mut i, "--physics-debug")?;
                cli.physics_debug_enabled = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(error(format!("unsupported --physics-debug value {value}"))),
                };
            }
            "--debug-mode" => {
                i += 1;
                if i < args.len() {
                    cli.debug_mode = args[i].parse().ok();
                }
            }
            "--freeze-time" => {
                if cli.freeze_time {
                    return Err(error("duplicate --freeze-time"));
                }
                cli.freeze_time = true;
            }
            "--time-mode" => {
                reject_duplicate(&time_mode, "--time-mode")?;
                let value = next_value(args, &mut i, "--time-mode")?;
                time_mode = Some(match value {
                    "wall-clock" => TimeMode::WallClock,
                    "fixed-step" => TimeMode::FixedStep,
                    "seek" => TimeMode::Seek,
                    _ => return Err(error(format!("unsupported --time-mode {value}"))),
                });
            }
            "--simulation-time" => {
                reject_duplicate(&simulation_time, "--simulation-time")?;
                simulation_time = Some(parse_value(args, &mut i, "--simulation-time")?);
            }
            "--fixed-dt" => {
                reject_duplicate(&fixed_dt, "--fixed-dt")?;
                fixed_dt = Some(parse_value(args, &mut i, "--fixed-dt")?);
            }
            "--max-substeps" => {
                reject_duplicate(&max_substeps, "--max-substeps")?;
                max_substeps = Some(parse_value(args, &mut i, "--max-substeps")?);
            }
            "--max-seek-steps" => {
                reject_duplicate(&max_seek_steps, "--max-seek-steps")?;
                max_seek_steps = Some(parse_value(args, &mut i, "--max-seek-steps")?);
            }
            "--ssao" => cli.ssao_enabled = true,
            "--no-ibl" => cli.no_ibl = true,
            "--local-lights" => {
                let value = next_value(args, &mut i, "--local-lights")?;
                cli.local_lights_enabled = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(error(format!("unsupported --local-lights value {value}"))),
                };
            }
            "--ssr" => cli.ssr_enabled = true,
            "--transparent" => {
                let value = next_value(args, &mut i, "--transparent")?;
                cli.transparent_enabled = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(error(format!("unsupported --transparent value {value}"))),
                };
            }
            "--particles" => {
                let value = next_value(args, &mut i, "--particles")?;
                cli.particles_enabled = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(error(format!("unsupported --particles value {value}"))),
                };
            }
            "--material-extraction" => {
                let value = next_value(args, &mut i, "--material-extraction")?;
                cli.material_extraction_enabled = match value {
                    "on" => true,
                    "off" => false,
                    _ => {
                        return Err(error(format!(
                            "unsupported --material-extraction value {value}"
                        )))
                    }
                };
            }
            "--width" => {
                i += 1;
                if i < args.len() {
                    cli.width = args[i].parse().ok();
                }
            }
            "--height" => {
                i += 1;
                if i < args.len() {
                    cli.height = args[i].parse().ok();
                }
            }
            "--benchmark-case" => {
                reject_duplicate(&benchmark_case_path, "--benchmark-case")?;
                benchmark_case_path =
                    Some(PathBuf::from(next_value(args, &mut i, "--benchmark-case")?));
            }
            "--benchmark-output" => {
                reject_duplicate(&benchmark_output_path, "--benchmark-output")?;
                benchmark_output_path = Some(PathBuf::from(next_value(
                    args,
                    &mut i,
                    "--benchmark-output",
                )?));
            }
            "--benchmark-visibility-output" => {
                reject_duplicate(
                    &benchmark_visibility_output_path,
                    "--benchmark-visibility-output",
                )?;
                benchmark_visibility_output_path = Some(PathBuf::from(next_value(
                    args,
                    &mut i,
                    "--benchmark-visibility-output",
                )?));
            }
            "--benchmark-commit" => {
                reject_duplicate(&benchmark_commit, "--benchmark-commit")?;
                benchmark_commit = Some(next_value(args, &mut i, "--benchmark-commit")?.to_owned());
            }
            "--benchmark-source-fingerprint" => {
                reject_duplicate(
                    &benchmark_source_fingerprint,
                    "--benchmark-source-fingerprint",
                )?;
                benchmark_source_fingerprint =
                    Some(next_value(args, &mut i, "--benchmark-source-fingerprint")?.to_owned());
            }
            "--benchmark-source-dirty" => {
                reject_duplicate(&benchmark_source_dirty, "--benchmark-source-dirty")?;
                benchmark_source_dirty = Some(
                    match next_value(args, &mut i, "--benchmark-source-dirty")? {
                        "true" => true,
                        "false" => false,
                        value => {
                            return Err(error(format!(
                                "invalid value for --benchmark-source-dirty: {value}"
                            )));
                        }
                    },
                );
            }
            _ => {}
        }
        i += 1;
    }

    if cli.freeze_time && (time_mode.is_some() || simulation_time.is_some()) {
        return Err(error(
            "--freeze-time conflicts with --time-mode and --simulation-time",
        ));
    }
    let mode = time_mode.unwrap_or_else(|| {
        if cli.freeze_time || simulation_time.is_some() {
            TimeMode::Seek
        } else {
            TimeMode::WallClock
        }
    });
    cli.time = TimeControl::new(
        mode,
        simulation_time.unwrap_or(0.0),
        fixed_dt.unwrap_or(1.0 / 60.0),
        max_substeps.unwrap_or(4),
        max_seek_steps.unwrap_or(4096),
    )
    .map_err(|cause| error(cause.to_string()))?;
    let benchmark_flag_count = [
        benchmark_case_path.is_some(),
        benchmark_output_path.is_some(),
        benchmark_commit.is_some(),
        benchmark_source_fingerprint.is_some(),
        benchmark_source_dirty.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if benchmark_visibility_output_path.is_some() && benchmark_flag_count == 0 {
        return Err(error(
            "--benchmark-visibility-output requires a benchmark capture",
        ));
    }
    if benchmark_flag_count != 0 {
        if benchmark_flag_count != 5 {
            return Err(error(
                "benchmark capture requires case, output, commit, source fingerprint, and dirty-state flags",
            ));
        }
        let (Some(case_path), Some(output_path), Some(commit), Some(fingerprint), Some(dirty)) = (
            benchmark_case_path,
            benchmark_output_path,
            benchmark_commit,
            benchmark_source_fingerprint,
            benchmark_source_dirty,
        ) else {
            return Err(error("incomplete benchmark capture arguments"));
        };
        let bytes = std::fs::read(&case_path).map_err(|cause| {
            error(format!(
                "could not read benchmark case {}: {cause}",
                case_path.display()
            ))
        })?;
        let case: aether_engine::performance::BenchmarkCase = serde_json::from_slice(&bytes)
            .map_err(|cause| {
                error(format!(
                    "invalid benchmark case {}: {cause}",
                    case_path.display()
                ))
            })?;
        if case
            .feature_flags
            .get("visibility")
            .copied()
            .unwrap_or(false)
            && benchmark_visibility_output_path.is_none()
        {
            return Err(error(
                "visibility-enabled benchmark requires --benchmark-visibility-output",
            ));
        }
        let expected_frames = case.warmup.checked_add(case.samples);
        if case.warmup != aether_engine::performance::BENCHMARK_WARMUP_FRAMES
            || case.samples != aether_engine::performance::BENCHMARK_SAMPLE_COUNT
            || cli.exit_after_frames != expected_frames
            || cli.screenshot.is_none()
            || cli.scene.is_none()
            || cli.width.unwrap_or(1280) != case.render.width
            || cli.height.unwrap_or(720) != case.render.height
            || !cli.no_gui_overlay
            || case.render.format != "rgba8"
            || case.time.mode != time_mode_name(cli.time.mode)
            || f64::from(cli.time.simulation_time) != case.time.simulation_time
            || (f64::from(cli.time.fixed_dt) - case.time.fixed_dt).abs() > 0.000001
        {
            return Err(error(
                "benchmark capture requires 30/120 frames, a bounded screenshot run, matching scene dimensions, and no GUI overlay",
            ));
        }
        if !is_lower_hex(&commit, 40) || !is_lower_hex(&fingerprint, 64) {
            return Err(error(
                "benchmark commit and source fingerprint must be lowercase hexadecimal hashes",
            ));
        }
        if PathBuf::from(cli.scene.as_deref().unwrap_or_default())
            .file_stem()
            .and_then(|value| value.to_str())
            != Some(case.scene.as_str())
        {
            return Err(error(
                "benchmark scene path stem must match BenchmarkCase.scene",
            ));
        }
        cli.benchmark = Some(super::benchmark::BenchmarkCapture::new(
            case,
            output_path,
            benchmark_visibility_output_path,
            commit,
            fingerprint,
            dirty,
        ));
    }
    Ok(cli)
}

fn time_mode_name(mode: TimeMode) -> &'static str {
    match mode {
        TimeMode::WallClock => "wall_clock",
        TimeMode::FixedStep => "fixed_step",
        TimeMode::Seek => "seek",
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn next_value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str, CliError> {
    *index += 1;
    let value = args
        .get(*index)
        .ok_or_else(|| error(format!("missing value for {flag}")))?;
    if value.starts_with("--") {
        return Err(error(format!("missing value for {flag}")));
    }
    Ok(value)
}

fn parse_value<T: std::str::FromStr>(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<T, CliError> {
    let value = next_value(args, index, flag)?;
    value
        .parse()
        .map_err(|_| error(format!("invalid value for {flag}: {value}")))
}

fn reject_duplicate<T>(value: &Option<T>, flag: &str) -> Result<(), CliError> {
    if value.is_some() {
        return Err(error(format!("duplicate {flag}")));
    }
    Ok(())
}

fn error(message: impl Into<String>) -> CliError {
    CliError {
        message: message.into(),
    }
}

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
        exit_after_frames: None,
        no_gui_overlay: false,
        debug_mode: None,
        freeze_time: false,
        ssao_enabled: false,
        no_ibl: false,
        ssr_enabled: false,
        width: None,
        height: None,
        time: TimeControl::default(),
    };
    let mut time_mode = None;
    let mut simulation_time = None;
    let mut fixed_dt = None;
    let mut max_substeps = None;
    let mut max_seek_steps = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scene" => {
                i += 1;
                if i < args.len() {
                    cli.scene = Some(args[i].clone());
                }
            }
            "--screenshot" => {
                i += 1;
                if i < args.len() {
                    cli.screenshot = Some(PathBuf::from(&args[i]));
                }
            }
            "--exit-after-frames" => {
                i += 1;
                if i < args.len() {
                    cli.exit_after_frames = args[i].parse().ok();
                }
            }
            "--no-gui-overlay" => cli.no_gui_overlay = true,
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
            "--ssr" => cli.ssr_enabled = true,
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
    Ok(cli)
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

//! Launcher CLI argument parsing.

use std::path::PathBuf;

pub(crate) struct CliArgs {
    pub(crate) scene: Option<String>,
    pub(crate) screenshot: Option<PathBuf>,
    pub(crate) exit_after_frames: Option<u32>,
    pub(crate) no_gui_overlay: bool,
    pub(crate) debug_mode: Option<i32>,
}

pub(crate) fn parse_args(args: &[String]) -> CliArgs {
    let mut cli = CliArgs {
        scene: None,
        screenshot: None,
        exit_after_frames: None,
        no_gui_overlay: false,
        debug_mode: None,
    };
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
            "--no-gui-overlay" => {
                cli.no_gui_overlay = true;
            }
            "--debug-mode" => {
                i += 1;
                if i < args.len() {
                    cli.debug_mode = args[i].parse().ok();
                }
            }
            _ => {}
        }
        i += 1;
    }
    cli
}

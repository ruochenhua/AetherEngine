//! Launcher CLI argument parsing.

use std::path::PathBuf;

pub(crate) struct CliArgs {
    pub(crate) scene: Option<String>,
    pub(crate) screenshot: Option<PathBuf>,
    pub(crate) exit_after_frames: Option<u32>,
    pub(crate) no_gui_overlay: bool,
    pub(crate) debug_mode: Option<i32>,
    pub(crate) freeze_time: bool,
    pub(crate) ssr_enabled: bool,
    /// Physical pixel width for the window / screenshots.
    pub(crate) width: Option<u32>,
    /// Physical pixel height for the window / screenshots.
    pub(crate) height: Option<u32>,
}

pub(crate) fn parse_args(args: &[String]) -> CliArgs {
    let mut cli = CliArgs {
        scene: None,
        screenshot: None,
        exit_after_frames: None,
        no_gui_overlay: false,
        debug_mode: None,
        freeze_time: false,
        ssr_enabled: false,
        width: None,
        height: None,
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
            "--freeze-time" => {
                cli.freeze_time = true;
            }
            "--ssr" => {
                cli.ssr_enabled = true;
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
            _ => {}
        }
        i += 1;
    }
    cli
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_width_and_height() {
        let cli = parse_args(&args(&[
            "aether-launcher",
            "--width",
            "1600",
            "--height",
            "900",
        ]));
        assert_eq!(cli.width, Some(1600));
        assert_eq!(cli.height, Some(900));
    }

    #[test]
    fn width_and_height_default_to_none() {
        let cli = parse_args(&args(&["aether-launcher"]));
        assert_eq!(cli.width, None);
        assert_eq!(cli.height, None);
    }

    #[test]
    fn invalid_width_is_ignored() {
        let cli = parse_args(&args(&["aether-launcher", "--width", "not-a-number"]));
        assert_eq!(cli.width, None);
    }
}

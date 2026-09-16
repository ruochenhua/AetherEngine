use super::super::App;
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

#[test]
fn parses_ssao_flag() {
    let cli = parse_args(&args(&["aether-launcher", "--ssao"]));
    assert!(cli.ssao_enabled);
}

#[test]
fn app_honors_ssr_cli_flag() {
    let cli = CliArgs {
        scene: None,
        screenshot: None,
        exit_after_frames: None,
        no_gui_overlay: false,
        debug_mode: None,
        freeze_time: false,
        ssao_enabled: false,
        ssr_enabled: true,
        width: None,
        height: None,
    };
    let app = App::new(cli);
    assert!(app.ssr_enabled);
}

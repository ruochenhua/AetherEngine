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
    ]))
    .unwrap();
    assert_eq!(cli.width, Some(1600));
    assert_eq!(cli.height, Some(900));
}

#[test]
fn width_and_height_default_to_none() {
    let cli = parse_args(&args(&["aether-launcher"])).unwrap();
    assert_eq!(cli.width, None);
    assert_eq!(cli.height, None);
}

#[test]
fn invalid_width_is_ignored() {
    let cli = parse_args(&args(&["aether-launcher", "--width", "not-a-number"])).unwrap();
    assert_eq!(cli.width, None);
}

#[test]
fn parses_ssao_flag() {
    let cli = parse_args(&args(&["aether-launcher", "--ssao"])).unwrap();
    assert!(cli.ssao_enabled);
}

#[test]
fn parses_no_ibl_flag() {
    let cli = parse_args(&args(&["aether-launcher", "--no-ibl"])).unwrap();
    assert!(cli.no_ibl);
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
        no_ibl: false,
        ssr_enabled: true,
        width: None,
        height: None,
        time: aether_engine::time::TimeControl::default(),
    };
    let app = App::new(cli);
    assert!(app.ssr_enabled);
}

#[test]
fn parses_time_modes_with_explicit_priority_and_implicit_seek() {
    let explicit = parse_args(&args(&[
        "aether-launcher",
        "--time-mode",
        "fixed-step",
        "--simulation-time",
        "1.0",
    ]))
    .unwrap();
    assert_eq!(explicit.time.mode, aether_engine::time::TimeMode::FixedStep);

    let implicit = parse_args(&args(&["aether-launcher", "--simulation-time", "0.5"])).unwrap();
    assert_eq!(implicit.time.mode, aether_engine::time::TimeMode::Seek);
    assert_eq!(implicit.time.simulation_time, 0.5);

    let frozen = parse_args(&args(&["aether-launcher", "--freeze-time"])).unwrap();
    assert_eq!(frozen.time.mode, aether_engine::time::TimeMode::Seek);
    assert_eq!(frozen.time.simulation_time, 0.0);
}

#[test]
fn rejects_missing_duplicate_conflicting_and_unsupported_time_values() {
    for invalid in [
        vec!["aether-launcher", "--time-mode"],
        vec!["aether-launcher", "--fixed-dt", "--ssao"],
        vec!["aether-launcher", "--time-mode", "turbo"],
        vec![
            "aether-launcher",
            "--fixed-dt",
            "0.01",
            "--fixed-dt",
            "0.02",
        ],
        vec!["aether-launcher", "--freeze-time", "--simulation-time", "0"],
        vec!["aether-launcher", "--freeze-time", "--time-mode", "seek"],
    ] {
        assert!(parse_args(&args(&invalid)).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn rejects_invalid_numeric_time_values_and_seek_limit() {
    for (flag, value) in [
        ("--fixed-dt", "0"),
        ("--fixed-dt", "0.100001"),
        ("--fixed-dt", "NaN"),
        ("--fixed-dt", "inf"),
        ("--simulation-time", "-1"),
        ("--simulation-time", "NaN"),
        ("--max-substeps", "0"),
        ("--max-substeps", "9"),
        ("--max-seek-steps", "0"),
        ("--max-seek-steps", "4097"),
    ] {
        assert!(
            parse_args(&args(&["aether-launcher", flag, value])).is_err(),
            "accepted {flag} {value}"
        );
    }

    assert!(parse_args(&args(&[
        "aether-launcher",
        "--simulation-time",
        "1.0",
        "--fixed-dt",
        "0.1",
        "--max-seek-steps",
        "9",
    ]))
    .is_err());
}

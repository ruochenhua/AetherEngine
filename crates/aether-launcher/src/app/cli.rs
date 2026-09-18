//! Launcher CLI argument parsing.

use aether_engine::time::TimeControl;
use std::path::PathBuf;

mod parse;
#[cfg(test)]
mod tests;

pub(crate) use parse::parse_args;

pub(crate) struct CliArgs {
    pub(crate) scene: Option<String>,
    pub(crate) screenshot: Option<PathBuf>,
    pub(crate) exit_after_frames: Option<u32>,
    pub(crate) no_gui_overlay: bool,
    pub(crate) debug_mode: Option<i32>,
    pub(crate) freeze_time: bool,
    pub(crate) ssao_enabled: bool,
    pub(crate) ssr_enabled: bool,
    /// Physical pixel width for the window / screenshots.
    pub(crate) width: Option<u32>,
    /// Physical pixel height for the window / screenshots.
    pub(crate) height: Option<u32>,
    pub(crate) time: TimeControl,
}

pub(crate) fn parse_env_or_exit() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    match parse_args(&args) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(2);
        }
    }
}

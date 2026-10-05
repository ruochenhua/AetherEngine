//! Headless process fixture; never creates a window or GPU device.
use std::{env, fs, process, thread, time::Duration};
use std::os::windows::process::CommandExt;
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("--descendant") {
        fs::write(&args[2], process::id().to_string()).unwrap();
        loop { thread::sleep(Duration::from_millis(20)); }
    }
    let mode = std::path::Path::new(&args[2]).file_stem().unwrap().to_str().unwrap();
    let root = std::path::PathBuf::from(env::var("FAKE_ROOT").unwrap());
    fs::write(root.join(format!("{mode}.started")), "started").unwrap();
    fs::write(root.join(format!("{mode}.argv")), args.join("\n")).unwrap();
    println!("stdout-{mode}"); eprintln!("stderr-{mode}");
    if ["hang", "timeout", "orphan", "panic"].contains(&mode) {
        process::Command::new(env::current_exe().unwrap()).args(["--descendant", root.join(format!("{mode}.descendant")).to_str().unwrap()])
            .creation_flags(0x08000000).spawn().unwrap();
        let marker = root.join(format!("{mode}.descendant"));
        for _ in 0..100 { if marker.exists() { break; } thread::sleep(Duration::from_millis(10)); }
    }
    if mode == "delayed" { thread::sleep(Duration::from_millis(600)); }
    if mode == "early" { process::exit(19); }
    if mode != "hang" { fs::write(env::var("AETHER_READY_PATH").unwrap(), b"R").unwrap(); }
    if mode == "panic" { process::exit(17); }
    if mode == "signal" { process::exit(134); }
    if ["hang", "timeout"].contains(&mode) { loop { thread::sleep(Duration::from_millis(20)); } }
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        if let Ok(source) = env::var("FAKE_CAPTURE") { fs::copy(source, &args[i + 1]).unwrap(); }
    }
    if let (Ok(source), Ok(target)) = (env::var("FAKE_DEVICE"), env::var("AETHER_DEVICE_METADATA_PATH")) {
        fs::copy(source, target).unwrap();
    }
    if mode == "delayed" { thread::sleep(Duration::from_millis(150)); }
}

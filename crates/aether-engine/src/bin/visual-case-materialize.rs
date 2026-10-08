//! Validates a VisualCase v2 manifest against a project tree and expands variants.
use aether_engine::visual_case::VisualManifest;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("visual-case-materialize: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = None;
    let mut manifest_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next().map(PathBuf::from),
            "--manifest" => manifest_path = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                println!("Usage: visual-case-materialize --root PROJECT --manifest FILE");
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    let root = root.ok_or("missing --root")?;
    let manifest_path = manifest_path.ok_or("missing --manifest")?;
    let input = std::fs::read_to_string(manifest_path)?;
    let manifest = VisualManifest::from_json_in(&input, &root)?;
    let cases = manifest.materialize()?;
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}

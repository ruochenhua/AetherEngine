//! CLI adapter for the production VisualCase schema; the runner owns no copy.
use aether_engine::visual_case::VisualManifest;
use std::{env, fs, path::Path};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    anyhow::ensure!(
        args.len() == 3 || (args.len() == 4 && args[3] == "--lexical"),
        "usage: visual_case_manifest <manifest> <project-root> [--lexical]"
    );
    let input = fs::read_to_string(&args[1])?;
    let manifest = if args.len() == 4 {
        VisualManifest::from_json(&input)?
    } else {
        VisualManifest::from_json_in(&input, Path::new(&args[2]))?
    };
    println!("{}", serde_json::to_string(&manifest.materialize()?)?);
    Ok(())
}

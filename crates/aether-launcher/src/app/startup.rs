//! Runtime root discovery for source checkouts and portable packages.

use std::path::{Path, PathBuf};

fn runtime_root_for_executable(executable: &Path) -> Option<PathBuf> {
    let mut directory = executable.parent().map(Path::to_path_buf);
    while let Some(current) = directory {
        if current.join("assets").is_dir() && current.join("scenes").is_dir() {
            return Some(current);
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    None
}

pub(super) fn set_working_dir_to_project_root() {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(root) = runtime_root_for_executable(&executable) {
            if let Err(error) = std::env::set_current_dir(&root) {
                eprintln!(
                    "Warning: failed to set working directory to {:?}: {}",
                    root, error
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::runtime_root_for_executable;
    use std::fs;
    use std::path::PathBuf;

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "aether-launcher-root-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn resolves_portable_package_without_cargo_manifest() {
        let root = test_root("portable");
        let package = root.join("release");
        fs::create_dir_all(package.join("assets")).unwrap();
        fs::create_dir_all(package.join("scenes")).unwrap();

        assert_eq!(
            runtime_root_for_executable(&package.join("aether-launcher.exe")),
            Some(package)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_workspace_for_development_build() {
        let root = test_root("workspace");
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::create_dir_all(root.join("target/release")).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();

        assert_eq!(
            runtime_root_for_executable(&root.join("target/release/aether-launcher.exe")),
            Some(root.clone())
        );
        fs::remove_dir_all(root).unwrap();
    }
}

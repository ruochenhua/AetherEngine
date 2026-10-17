use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether-dependency-snapshot-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("assets")).unwrap();
        fs::write(path.join("assets/target.bin"), b"first").unwrap();
        fs::write(path.join("assets/material.ron"), b"(roughness:0.5)").unwrap();
        Self(path)
    }
    fn inventory(&self) -> inventory::Inventory {
        inventory::scan(&self.0, AnalysisBudget::default(), true).unwrap()
    }
    fn finish(&self, first: inventory::Inventory) -> DependencyReport {
        finish(
            &self.0,
            "assets/target.bin",
            AnalysisBudget::default(),
            first,
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn target_replacement_between_scan_and_publication_is_stale() {
    let f = Fixture::new();
    let first = f.inventory();
    fs::write(f.0.join("replacement"), b"other").unwrap();
    fs::rename(f.0.join("replacement"), f.0.join("assets/target.bin")).unwrap();
    let report = f.finish(first);
    assert!(report.stale);
    assert!(!report.is_complete());
}
#[test]
fn same_size_document_change_between_scan_and_publication_is_stale() {
    let f = Fixture::new();
    let first = f.inventory();
    fs::write(f.0.join("assets/material.ron"), b"(roughness:0.6)").unwrap();
    let report = f.finish(first);
    assert!(report.stale);
    assert!(!report.is_complete());
}
#[test]
fn file_addition_between_scan_and_publication_is_stale() {
    let f = Fixture::new();
    let first = f.inventory();
    fs::write(
        f.0.join("assets/new.ron"),
        b"(future_texture:Some(\"x.png\"))",
    )
    .unwrap();
    let report = f.finish(first);
    assert!(report.stale);
    assert!(!report.is_complete());
}
#[test]
fn directory_addition_between_scan_and_publication_is_stale() {
    let f = Fixture::new();
    let first = f.inventory();
    fs::create_dir(f.0.join("assets/empty")).unwrap();
    let report = f.finish(first);
    assert!(report.stale);
    assert!(!report.is_complete());
}

//! Read-only inventory of path identities retained by hot-reload watchers.

use super::{asset_hot_reload::AssetHotReloadWatcher, material_reload::MaterialAssetWatcher};
use aether_engine::asset::{AssetKind, CanonicalPath};
use std::path::Path;

/// Which watcher field currently retains a path key.
#[allow(dead_code)] // Staged for the upcoming project-wide mutation gate inventory.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum WatcherResourceOwner {
    /// Prefab and texture file watcher fields.
    AssetHotReload,
    /// Linked material file watcher fields.
    MaterialAssetReload,
}

/// Whether a path is an observed timestamp baseline or awaiting reload completion.
#[allow(dead_code)] // Staged for the upcoming project-wide mutation gate inventory.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum WatcherResourceState {
    /// The watcher stores a last-seen modification timestamp.
    ObservedBaseline,
    /// The watcher has queued a reload and retains the path until result handling.
    ReloadPending,
}

/// A path key owned by one watcher field.
#[allow(dead_code)] // Staged for the upcoming project-wide mutation gate inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WatcherResourceUsage {
    /// Owning watcher implementation.
    pub owner: WatcherResourceOwner,
    /// Typed asset category associated with this watcher field.
    pub kind: AssetKind,
    /// Exact key spelling retained by the watcher.
    pub authored_path: String,
    /// Lexical project-relative identity, or `None` when the raw key is invalid.
    pub project_relative: Option<CanonicalPath>,
    /// State represented by this watcher field.
    pub state: WatcherResourceState,
}

/// Observe path keys retained by both launcher reload watchers.
///
/// The result is sorted by owner, asset kind, normalized path (then raw spelling
/// when unknown), and state. Equal paths in separate fields remain separate rows.
/// This reads no files, polls no queues, and does not claim worker requests,
/// results already polled into locals, dependent scene consumers, or editor queues.
#[allow(dead_code)] // Staged for the upcoming project-wide mutation gate inventory.
pub(crate) fn capture_watcher_resource_usage(
    asset_watcher: &AssetHotReloadWatcher,
    material_watcher: &MaterialAssetWatcher,
    project_root: &Path,
) -> Vec<WatcherResourceUsage> {
    let mut rows = Vec::new();
    for (kind, baselines, pending) in [
        (
            AssetKind::Prefab,
            &asset_watcher.prefab_modified,
            &asset_watcher.pending_prefabs,
        ),
        (
            AssetKind::CpuTexture,
            &asset_watcher.texture_modified,
            &asset_watcher.pending_textures,
        ),
    ] {
        rows.extend(baselines.keys().map(|path| {
            row(
                WatcherResourceOwner::AssetHotReload,
                kind,
                path,
                WatcherResourceState::ObservedBaseline,
                project_root,
            )
        }));
        rows.extend(pending.iter().map(|path| {
            row(
                WatcherResourceOwner::AssetHotReload,
                kind,
                path,
                WatcherResourceState::ReloadPending,
                project_root,
            )
        }));
    }
    rows.extend(material_watcher.modified.keys().map(|path| {
        row(
            WatcherResourceOwner::MaterialAssetReload,
            AssetKind::Material,
            path,
            WatcherResourceState::ObservedBaseline,
            project_root,
        )
    }));
    rows.extend(material_watcher.pending.iter().map(|path| {
        row(
            WatcherResourceOwner::MaterialAssetReload,
            AssetKind::Material,
            path,
            WatcherResourceState::ReloadPending,
            project_root,
        )
    }));
    rows.sort_by(|left, right| {
        left.owner
            .cmp(&right.owner)
            .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
            .then_with(|| sort_path(left).cmp(sort_path(right)))
            .then_with(|| left.state.cmp(&right.state))
            .then_with(|| left.authored_path.cmp(&right.authored_path))
    });
    rows
}

fn row(
    owner: WatcherResourceOwner,
    kind: AssetKind,
    path: &str,
    state: WatcherResourceState,
    root: &Path,
) -> WatcherResourceUsage {
    WatcherResourceUsage {
        owner,
        kind,
        authored_path: path.to_owned(),
        project_relative: checked_path(root, path),
        state,
    }
}

fn sort_path(row: &WatcherResourceUsage) -> &str {
    row.project_relative
        .as_ref()
        .map_or(row.authored_path.as_str(), CanonicalPath::as_str)
}

fn checked_path(root: &Path, raw: &str) -> Option<CanonicalPath> {
    CanonicalPath::new(root, Path::new(raw)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::time::SystemTime;

    #[test]
    fn watcher_usage_is_sorted_typed_and_observational() {
        let root = PathBuf::from("/project");
        let mut asset = AssetHotReloadWatcher::default();
        asset
            .prefab_modified
            .insert("./assets/shared.prefab".into(), SystemTime::UNIX_EPOCH);
        asset
            .pending_prefabs
            .insert("./assets/shared.prefab".into());
        asset
            .texture_modified
            .insert("assets/shared.prefab".into(), SystemTime::UNIX_EPOCH);
        asset.pending_textures.insert("assets/other.png".into());
        asset
            .prefab_modified
            .insert("../outside.prefab".into(), SystemTime::UNIX_EPOCH);

        let mut material = MaterialAssetWatcher::default();
        material
            .modified
            .insert("assets/shared.prefab".into(), SystemTime::UNIX_EPOCH);
        material.pending.insert("assets/shared.prefab".into());

        let first = capture_watcher_resource_usage(&asset, &material, &root);
        let second = capture_watcher_resource_usage(&asset, &material, &root);
        assert_eq!(first, second, "repeated observation must be stable");
        assert_eq!(
            first.len(),
            7,
            "each retained watcher field needs its own row"
        );
        assert_eq!(
            first
                .iter()
                .map(|row| (
                    row.owner,
                    row.kind,
                    row.project_relative.as_ref().map(CanonicalPath::as_str),
                    row.state
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    WatcherResourceOwner::AssetHotReload,
                    AssetKind::CpuTexture,
                    Some("assets/other.png"),
                    WatcherResourceState::ReloadPending
                ),
                (
                    WatcherResourceOwner::AssetHotReload,
                    AssetKind::CpuTexture,
                    Some("assets/shared.prefab"),
                    WatcherResourceState::ObservedBaseline
                ),
                (
                    WatcherResourceOwner::AssetHotReload,
                    AssetKind::Prefab,
                    None,
                    WatcherResourceState::ObservedBaseline
                ),
                (
                    WatcherResourceOwner::AssetHotReload,
                    AssetKind::Prefab,
                    Some("assets/shared.prefab"),
                    WatcherResourceState::ObservedBaseline
                ),
                (
                    WatcherResourceOwner::AssetHotReload,
                    AssetKind::Prefab,
                    Some("assets/shared.prefab"),
                    WatcherResourceState::ReloadPending
                ),
                (
                    WatcherResourceOwner::MaterialAssetReload,
                    AssetKind::Material,
                    Some("assets/shared.prefab"),
                    WatcherResourceState::ObservedBaseline
                ),
                (
                    WatcherResourceOwner::MaterialAssetReload,
                    AssetKind::Material,
                    Some("assets/shared.prefab"),
                    WatcherResourceState::ReloadPending
                ),
            ]
        );
        assert_eq!(first[2].authored_path, "../outside.prefab");
        assert_eq!(asset.prefab_modified.len(), 2);
        assert_eq!(
            asset.pending_prefabs,
            HashSet::from(["./assets/shared.prefab".into()])
        );
        assert_eq!(asset.texture_modified.len(), 1);
        assert_eq!(
            asset.pending_textures,
            HashSet::from(["assets/other.png".into()])
        );
        assert_eq!(material.modified.len(), 1);
        assert_eq!(
            material.pending,
            HashSet::from(["assets/shared.prefab".into()])
        );
    }
}

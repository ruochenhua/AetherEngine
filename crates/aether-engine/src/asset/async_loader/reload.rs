//! File-change detection and recovery for [`super::AsyncAssetLoader`].

use super::{Asset, AssetStore, AsyncRecord, LoadStateView};
use std::any::Any;

#[cfg(test)]
mod tests;

pub(super) fn reload_changed<T: Asset>(
    store: &mut AssetStore,
    value: &mut (dyn Any + Send + Sync),
) {
    let Some(record) = value.downcast_mut::<AsyncRecord<T>>() else {
        return;
    };
    if record.error.is_some() {
        return;
    }
    let Some(handle) = record.handle else {
        return;
    };
    let has_last_good = match store.state(handle) {
        Ok(LoadStateView::Ready { .. }) => true,
        Ok(LoadStateView::Failed { has_last_good, .. }) => has_last_good,
        Ok(LoadStateView::Loading { .. }) | Err(_) => return,
    };
    let modified = std::fs::metadata(store.project_root().join(&record.path))
        .and_then(|metadata| metadata.modified())
        .ok();
    if modified.is_some() && modified != record.last_modified {
        let queued = if has_last_good {
            store.reload(handle).map(|ticket| {
                record.ticket = store.load_ticket(&ticket).ok();
            })
        } else {
            store.request::<T>(&record.path).map(|(handle, ticket)| {
                record.handle = Some(handle);
                record.ticket = Some(ticket);
            })
        };
        match queued {
            Ok(()) => record.last_modified = modified,
            Err(error) => tracing::warn!("asset auto-reload failed: {error}"),
        }
    }
}

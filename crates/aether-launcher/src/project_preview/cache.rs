use super::Thumbnail;
use crate::project_index::ResourceKind;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) enum PreviewState {
    Idle,
    Loading,
    Ready(Arc<Thumbnail>),
    Unavailable(String),
}
#[derive(Clone)]
pub(super) struct Request {
    pub path: String,
    pub kind: ResourceKind,
    pub token: u64,
}
pub(super) struct Cache {
    pub state: PreviewState,
    selection: Option<(String, ResourceKind)>,
    token: u64,
    capacity: usize,
    entries: VecDeque<(String, ResourceKind, PreviewState)>,
}
impl Cache {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: PreviewState::Idle,
            selection: None,
            token: 0,
            capacity: capacity.min(8),
            entries: VecDeque::new(),
        }
    }
    pub fn select(&mut self, selection: Option<(&str, ResourceKind)>) -> Option<Request> {
        let selection = selection.map(|(path, kind)| (path.to_owned(), kind));
        if selection == self.selection {
            return None;
        }
        self.token = self.token.wrapping_add(1);
        self.selection = selection;
        let Some((path, kind)) = self.selection.as_ref() else {
            self.state = PreviewState::Idle;
            return None;
        };
        if let Some(index) = self
            .entries
            .iter()
            .position(|(cached_path, cached_kind, _)| cached_path == path && cached_kind == kind)
        {
            if let Some(entry) = self.entries.remove(index) {
                self.state = entry.2.clone();
                self.entries.push_back(entry);
            }
            return None;
        }
        self.state = PreviewState::Loading;
        Some(Request {
            path: path.clone(),
            kind: *kind,
            token: self.token,
        })
    }
    pub fn publish(&mut self, request: &Request, result: Result<Thumbnail, String>) -> bool {
        if request.token != self.token
            || self.selection.as_ref() != Some(&(request.path.clone(), request.kind))
        {
            return false;
        }
        self.state = match result {
            Ok(thumbnail) => PreviewState::Ready(Arc::new(thumbnail)),
            Err(reason) => PreviewState::Unavailable(reason),
        };
        if self.capacity > 0 {
            while self.entries.len() >= self.capacity {
                self.entries.pop_front();
            }
            self.entries
                .push_back((request.path.clone(), request.kind, self.state.clone()));
        }
        true
    }
    pub fn invalidate(&mut self) {
        self.token = self.token.wrapping_add(1);
        self.selection = None;
        self.state = PreviewState::Idle;
        self.entries.clear();
    }
    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn thumbnail() -> Thumbnail {
        Thumbnail {
            size: [1, 1],
            rgba: vec![0, 0, 0, 255],
            dimensions: "Image: 1 × 1 px".into(),
        }
    }
    #[test]
    fn deduplicates_selection_and_reuses_bounded_cache() {
        let mut cache = Cache::new(2);
        let first = cache
            .select(Some(("a.png", ResourceKind::Image)))
            .expect("first request");
        assert!(matches!(cache.state, PreviewState::Loading));
        assert!(cache.select(Some(("a.png", ResourceKind::Image))).is_none());
        assert!(cache.publish(&first, Ok(thumbnail())));
        for path in ["b.png", "c.png"] {
            let request = cache.select(Some((path, ResourceKind::Image))).unwrap();
            assert!(cache.publish(&request, Ok(thumbnail())));
        }
        assert_eq!(cache.len(), 2);
        assert!(cache.select(Some(("b.png", ResourceKind::Image))).is_none());
        assert!(matches!(cache.state, PreviewState::Ready(_)));
        assert!(cache.select(Some(("a.png", ResourceKind::Image))).is_some());
    }
    #[test]
    fn selection_change_and_refresh_reject_stale_results() {
        let mut cache = Cache::new(2);
        let old = cache.select(Some(("a.png", ResourceKind::Image))).unwrap();
        let current = cache.select(Some(("b.obj", ResourceKind::Obj))).unwrap();
        assert!(!cache.publish(&old, Ok(thumbnail())));
        assert!(matches!(cache.state, PreviewState::Loading));
        cache.invalidate();
        let fresh = cache.select(Some(("b.obj", ResourceKind::Obj))).unwrap();
        assert_ne!(current.token, fresh.token);
        assert!(!cache.publish(&current, Err("old".into())));
        assert!(cache.publish(&fresh, Err("Unavailable reason".into())));
        assert!(
            matches!(cache.state,PreviewState::Unavailable(ref reason) if reason == "Unavailable reason")
        );
        cache.select(None);
        assert!(matches!(cache.state, PreviewState::Idle));
        assert!(!cache.publish(&fresh, Ok(thumbnail())));
    }
}

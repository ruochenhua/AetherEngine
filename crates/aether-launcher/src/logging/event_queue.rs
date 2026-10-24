use super::CapturedEvent;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{mpsc::TryRecvError, Arc, Mutex},
};

#[derive(Clone)]
pub(crate) struct EventQueue {
    events: Arc<Mutex<VecDeque<CapturedEvent>>>,
    capacity: usize,
}

impl EventQueue {
    pub(crate) fn bounded(capacity: usize) -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    pub(crate) fn try_send(&self, event: CapturedEvent) -> bool {
        let Ok(mut events) = self.events.try_lock() else {
            return false;
        };
        if events.len() >= self.capacity {
            return false;
        }
        events.push_back(event);
        true
    }

    pub(crate) fn try_recv(&self) -> Result<CapturedEvent, TryRecvError> {
        self.events
            .try_lock()
            .ok()
            .and_then(|mut events| events.pop_front())
            .ok_or(TryRecvError::Empty)
    }

    pub(crate) fn try_iter(&self) -> impl Iterator<Item = CapturedEvent> + '_ {
        std::iter::from_fn(|| self.try_recv().ok())
    }

    pub(crate) fn resource_path_hints(&self) -> Result<Vec<PathBuf>, ()> {
        let events = self.events.try_lock().map_err(|_| ())?;
        let mut resources = Vec::new();
        for event in events.iter() {
            for field in ["asset", "path"] {
                if let Some(path) = event.fields.get(field).and_then(|value| value.as_str()) {
                    resources.push(PathBuf::from(path));
                }
            }
        }
        Ok(resources)
    }
}

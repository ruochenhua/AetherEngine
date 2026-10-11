use super::CapturedEvent;
use std::collections::VecDeque;

pub(crate) struct LogStore {
    pub events: VecDeque<CapturedEvent>,
    capacity: usize,
}
impl LogStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }
    pub fn push(&mut self, event: CapturedEvent) {
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
    pub fn clear(&mut self) {
        self.events.clear();
    }
    pub fn visible<'a>(&'a self, filter: &'a LogFilter) -> impl Iterator<Item = &'a CapturedEvent> {
        self.events.iter().filter(move |event| {
            super::level_index(&event.level) <= filter.level
                && event.target.contains(&filter.category)
                && (event
                    .message
                    .to_lowercase()
                    .contains(&filter.text.to_lowercase())
                    || event.fields.iter().any(|(key, value)| {
                        format!("{key} {value}")
                            .to_lowercase()
                            .contains(&filter.text.to_lowercase())
                    }))
        })
    }
}
pub(crate) struct LogFilter {
    pub level: usize,
    pub category: String,
    pub text: String,
}
#[derive(Default)]
pub(crate) struct LogView {
    pub filter: LogFilter,
    pub paused: bool,
}
impl LogView {
    pub fn auto_scroll(&self) -> bool {
        !self.paused
    }
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            level: 2,
            category: String::new(),
            text: String::new(),
        }
    }
}

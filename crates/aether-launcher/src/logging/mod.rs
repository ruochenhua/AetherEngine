//! Structured runtime diagnostics, independent of editor transaction replay.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use tracing_subscriber::{layer::Context, Layer};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CapturedEvent {
    pub timestamp_ms: u64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: BTreeMap<String, serde_json::Value>,
    pub resource: Option<std::path::PathBuf>,
}
#[derive(Clone)]
pub(crate) struct CaptureLayer {
    sender: EventQueue,
    pub health: std::sync::Arc<Health>,
    file: Option<file::FileSink>,
}
pub(crate) fn capture(capacity: usize) -> (CaptureLayer, EventQueue) {
    let sender = EventQueue::bounded(capacity);
    (
        CaptureLayer {
            sender: sender.clone(),
            health: Arc::default(),
            file: None,
        },
        sender.clone(),
    )
}
impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut visitor = Fields::default();
        event.record(&mut visitor);
        let captured = CapturedEvent {
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64),
            level: event.metadata().level().to_string(),
            target: bounded(event.metadata().target()),
            message: visitor.message,
            fields: visitor.values,
            resource: None,
        };
        if let Some(file) = self.file.as_ref() {
            file.send(captured.clone());
        }
        if !self.sender.try_send(captured) {
            let index = level_index(event.metadata().level().as_str());
            self.health.dropped[index].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
#[cfg(test)]
mod tests;

// Bound event payloads as well as queue slots: 32 fields, 2 KiB per string.
const MAX_FIELDS: usize = 32;
const MAX_TEXT: usize = 2048;
fn bounded(value: &str) -> String {
    let mut end = value.len().min(MAX_TEXT);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
#[derive(Default)]
struct Fields {
    message: String,
    values: BTreeMap<String, serde_json::Value>,
}
impl Fields {
    fn insert(&mut self, field: &tracing::field::Field, value: serde_json::Value) {
        if field.name() == "message" {
            self.message = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
        } else if self.values.len() < MAX_FIELDS {
            self.values.insert(bounded(field.name()), value);
        }
    }
}
impl tracing::field::Visit for Fields {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.insert(field, bounded(value).into());
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.insert(field, value.into());
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.insert(field, value.into());
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.insert(field, value.into());
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.insert(field, value.into());
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        struct Text(String);
        impl Write for Text {
            fn write_str(&mut self, value: &str) -> std::fmt::Result {
                let available = MAX_TEXT.saturating_sub(self.0.len());
                let mut end = value.len().min(available);
                while !value.is_char_boundary(end) {
                    end -= 1;
                }
                self.0.push_str(&value[..end]);
                Ok(())
            }
        }
        let mut text = Text(String::new());
        let _ = write!(text, "{value:?}");
        self.insert(field, text.0.into());
    }
}

type Fallback = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Default)]
pub(crate) struct Health {
    #[cfg(test)]
    file_hooks: Option<std::sync::Arc<file::tests::Hooks>>,
    dropped: [std::sync::atomic::AtomicU64; 5],
    pub fallback_count: std::sync::atomic::AtomicU64,
    stopping: std::sync::atomic::AtomicBool,
    active_admissions: std::sync::atomic::AtomicUsize,
    error: std::sync::Mutex<Option<String>>,
    fallback: std::sync::Mutex<Option<Fallback>>,
}

impl Health {
    pub fn dropped(&self) -> [u64; 5] {
        std::array::from_fn(|i| self.dropped[i].load(std::sync::atomic::Ordering::Relaxed))
    }
    pub fn degraded(&self) -> bool {
        self.dropped().iter().any(|n| *n > 0) || self.error().is_some()
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|error| error.clone())
    }
    fn fail(&self, message: String) {
        if let Ok(mut error) = self.error.lock() {
            if error.is_none() {
                *error = Some(message.clone());
            }
        }
        if self
            .fallback_count
            .compare_exchange(
                0,
                1,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_ok()
        {
            let summary = format!("Runtime logging degraded: {message}");
            let fallback = self
                .fallback
                .lock()
                .ok()
                .and_then(|emitter| emitter.clone());
            if let Some(emit) = fallback {
                emit(&summary);
            } else {
                eprintln!("{summary}");
            }
        }
    }
}

fn level_index(level: &str) -> usize {
    match level {
        "ERROR" => 0,
        "WARN" => 1,
        "INFO" => 2,
        "DEBUG" => 3,
        _ => 4,
    }
}

mod store;
pub(crate) use store::{LogStore, LogView};

mod event_queue;
pub(crate) use event_queue::EventQueue;

mod file;

mod panel;
pub(crate) use panel::{LogPanel, LogPanelResourceSnapshot};

impl CaptureLayer {
    pub fn with_file(mut self, file: file::FileSink) -> Self {
        self.file = Some(file);
        self
    }
}

/// Finite production limits: 512 UI records queued, 2,000 retained in the view,
/// 2,048 queued for disk, and five 1 MiB runtime files (rotated at session start).
pub(crate) const UI_QUEUE_CAPACITY: usize = 512;
const UI_HISTORY_CAPACITY: usize = 2000;
const FILE_QUEUE_CAPACITY: usize = 2048;

pub(crate) fn session(root: std::path::PathBuf) -> (CaptureLayer, LogPanel, file::FileWriter) {
    let (layer, receiver) = capture(UI_QUEUE_CAPACITY);
    let (sink, file_receiver) = file::file_channel(FILE_QUEUE_CAPACITY);
    let writer = file::FileWriter::start(
        file_receiver,
        sink.health.clone(),
        file::FileConfig {
            directory: root.join(".aether/logs"),
            max_bytes: 1024 * 1024,
            retained_files: 5,
        },
    );
    let panel = LogPanel::new(
        Some(receiver),
        layer.health.clone(),
        sink.health.clone(),
        root,
    );
    (layer.with_file(sink), panel, writer)
}

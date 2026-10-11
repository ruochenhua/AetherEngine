//! Dedicated bounded runtime file writer. Never invokes tracing.
use super::{CapturedEvent, Health};
use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::PathBuf,
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone)]
pub(crate) struct FileSink {
    sender: SyncSender<CapturedEvent>,
    pub health: Arc<Health>,
}
impl FileSink {
    pub fn send(&self, event: CapturedEvent) {
        let index = super::level_index(&event.level);
        if self.health.stopping.load(Ordering::Acquire) || self.sender.try_send(event).is_err() {
            self.health.dropped[index].fetch_add(1, Ordering::Relaxed);
        }
    }
}
pub(crate) fn file_channel(capacity: usize) -> (FileSink, Receiver<CapturedEvent>) {
    let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
    (
        FileSink {
            sender,
            health: Default::default(),
        },
        receiver,
    )
}
pub(crate) struct FileConfig {
    pub directory: PathBuf,
    pub max_bytes: u64,
    pub retained_files: usize,
}
pub(crate) struct FileWriter {
    health: Arc<Health>,
    thread: Option<JoinHandle<()>>,
}
impl FileWriter {
    pub fn start(
        receiver: Receiver<CapturedEvent>,
        health: Arc<Health>,
        config: FileConfig,
    ) -> Self {
        Self::start_with_fallback(receiver, health, config, |summary| eprintln!("{summary}"))
    }
    pub fn start_with_fallback(
        receiver: Receiver<CapturedEvent>,
        health: Arc<Health>,
        config: FileConfig,
        fallback: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        if let Ok(mut emitter) = health.fallback.lock() {
            *emitter = Some(Arc::new(fallback));
        }
        let worker_health = health.clone();
        let thread = match std::thread::Builder::new()
            .name("runtime-log-writer".into())
            .spawn(move || {
                write_events(receiver, &worker_health, config);
            }) {
            Ok(thread) => Some(thread),
            Err(error) => {
                health.fail(error.to_string());
                None
            }
        };
        Self { health, thread }
    }
    pub fn shutdown(&mut self) {
        self.health.stopping.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                self.health.fail("runtime log writer panicked".into());
            }
        }
    }
}
impl Drop for FileWriter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Output {
    config: FileConfig,
    file: BufWriter<File>,
    bytes: u64,
}
impl Output {
    fn open(config: FileConfig) -> io::Result<Self> {
        fs::create_dir_all(&config.directory)?;
        rotate(&config)?;
        let file = BufWriter::new(File::create(config.directory.join("runtime.0.jsonl"))?);
        Ok(Self {
            config,
            file,
            bytes: 0,
        })
    }
    fn write(&mut self, event: &CapturedEvent) -> io::Result<()> {
        let mut record = serde_json::to_vec(event)?;
        record.push(b'\n');
        let length = record.len() as u64;
        if length > self.config.max_bytes {
            return Err(io::Error::other(
                "runtime log record exceeds file size limit",
            ));
        }
        if self.bytes + length > self.config.max_bytes {
            self.file.flush()?;
            rotate(&self.config)?;
            self.file =
                BufWriter::new(File::create(self.config.directory.join("runtime.0.jsonl"))?);
            self.bytes = 0;
        }
        self.file.write_all(&record)?;
        self.bytes += length;
        Ok(())
    }
}
fn rotate(config: &FileConfig) -> io::Result<()> {
    let count = config.retained_files.max(1);
    let oldest = config
        .directory
        .join(format!("runtime.{}.jsonl", count - 1));
    if oldest.exists() {
        fs::remove_file(oldest)?;
    }
    for index in (1..count).rev() {
        let source = config
            .directory
            .join(format!("runtime.{}.jsonl", index - 1));
        if source.exists() {
            fs::rename(
                source,
                config.directory.join(format!("runtime.{index}.jsonl")),
            )?;
        }
    }
    Ok(())
}
fn write_events(receiver: Receiver<CapturedEvent>, health: &Health, config: FileConfig) {
    let mut output = match Output::open(config) {
        Ok(output) => Some(output),
        Err(error) => {
            health.fail(error.to_string());
            None
        }
    };
    loop {
        let result = if health.stopping.load(Ordering::Acquire) {
            receiver
                .try_recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } else {
            receiver.recv_timeout(Duration::from_millis(50))
        };
        match result {
            Ok(event) => {
                let result = output.as_mut().map_or_else(
                    || Err(io::Error::other("runtime log output unavailable")),
                    |output| output.write(&event),
                );
                if let Err(error) = result {
                    health.dropped[super::level_index(&event.level)]
                        .fetch_add(1, Ordering::Relaxed);
                    health.fail(error.to_string());
                    output = None;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(writer) = output.as_mut() {
                    if let Err(error) = writer.file.flush() {
                        health.fail(error.to_string());
                        output = None;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if let Some(writer) = output.as_mut() {
        if let Err(error) = writer.file.flush() {
            health.fail(error.to_string());
        }
    }
}

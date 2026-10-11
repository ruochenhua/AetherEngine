use super::*;
use crate::logging::tests::{directory, event, file_config};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize},
    Mutex,
};

#[derive(Default)]
pub(crate) struct Hooks {
    pub before_receive: Option<Box<dyn Fn() + Send + Sync>>,
    pub before_send: Option<Box<dyn Fn() + Send + Sync>>,
    pub before_empty_shutdown: Option<Box<dyn Fn() + Send + Sync>>,
    pub after_write: Option<Box<dyn Fn(usize) + Send + Sync>>,
    pub output_file: Mutex<Option<File>>,
}
fn hooked_channel(hooks: Hooks) -> (FileSink, Receiver<CapturedEvent>) {
    let (mut sink, receiver) = file_channel(16);
    Arc::get_mut(&mut sink.health).unwrap().file_hooks = Some(Arc::new(hooks));
    (sink, receiver)
}

#[test]
fn ew1_shutdown_waits_for_inflight_admission_before_final_flush() {
    let directory = directory("shutdown-admission");
    let (producer_checked_tx, producer_checked_rx) = mpsc::channel();
    let (resume_producer_tx, resume_producer_rx) = mpsc::channel();
    let resume_producer_rx = Mutex::new(resume_producer_rx);
    let (empty_tx, empty_rx) = mpsc::channel();
    let (resume_worker_tx, resume_worker_rx) = mpsc::channel();
    let resume_worker_rx = Mutex::new(resume_worker_rx);
    let empty_once = AtomicBool::new(false);
    let receive_once = AtomicBool::new(false);
    let (receive_tx, receive_rx) = mpsc::channel();
    let (resume_receive_tx, resume_receive_rx) = mpsc::channel();
    let resume_receive_rx = Mutex::new(resume_receive_rx);
    let (sink, receiver) = hooked_channel(Hooks {
        before_receive: Some(Box::new(move || {
            if !receive_once.swap(true, Ordering::Relaxed) {
                receive_tx.send(()).unwrap();
                resume_receive_rx.lock().unwrap().recv().unwrap();
            }
        })),
        before_send: Some(Box::new(move || {
            producer_checked_tx.send(()).unwrap();
            resume_producer_rx.lock().unwrap().recv().unwrap();
        })),
        before_empty_shutdown: Some(Box::new(move || {
            if !empty_once.swap(true, Ordering::Relaxed) {
                empty_tx.send(()).unwrap();
                resume_worker_rx.lock().unwrap().recv().unwrap();
            }
        })),
        ..Default::default()
    });
    let mut writer = FileWriter::start(
        receiver,
        sink.health.clone(),
        file_config(directory.clone(), 4096, 3),
    );
    receive_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let producer = sink.clone();
    let producer = std::thread::spawn(move || producer.send(event("WARN", "test", "inflight")));
    producer_checked_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let shutdown = std::thread::spawn(move || writer.shutdown());
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !sink.health.stopping.load(Ordering::Acquire) {
        assert!(
            std::time::Instant::now() < deadline,
            "shutdown must close admission"
        );
        std::thread::yield_now();
    }
    resume_receive_tx.send(()).unwrap();
    empty_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    resume_producer_tx.send(()).unwrap();
    producer.join().unwrap();
    resume_worker_tx.send(()).unwrap();
    shutdown.join().unwrap();
    let records = fs::read_to_string(directory.join("runtime.0.jsonl")).unwrap();
    assert!(
        records.contains("inflight"),
        "successfully admitted record must survive shutdown"
    );
    assert_eq!(sink.health.dropped(), [0; 5]);
    fs::remove_dir_all(directory).unwrap();
}

fn flush_failure(mode: &str) {
    let directory = directory(mode);
    let read_only_path = directory.join("read-only");
    fs::write(&read_only_path, "").unwrap();
    let buffered_records = Arc::new(AtomicUsize::new(0));
    let observed = buffered_records.clone();
    let (sink, receiver) = hooked_channel(Hooks {
        after_write: Some(Box::new(move |bytes| {
            assert!(
                bytes > 0,
                "admitted record must be in BufWriter before failure"
            );
            observed.fetch_add(1, Ordering::Relaxed);
        })),
        output_file: Mutex::new(Some(File::open(&read_only_path).unwrap())),
        ..Default::default()
    });
    for level in ["ERROR", "WARN", "WARN", "INFO", "DEBUG", "TRACE"] {
        sink.send(event(level, "test", "buffered"));
    }
    let first_length = serde_json::to_vec(&event("ERROR", "test", "buffered"))
        .unwrap()
        .len() as u64
        + 1;
    let max_bytes = if mode == "rotation-flush" {
        first_length * 3
    } else {
        4096
    };
    let (failed_tx, failed_rx) = mpsc::channel();
    let mut writer = FileWriter::start_with_fallback(
        receiver,
        sink.health.clone(),
        file_config(directory.clone(), max_bytes, 3),
        move |_| {
            failed_tx.send(()).unwrap();
        },
    );
    if mode == "idle-flush" {
        failed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    writer.shutdown();
    assert_eq!(
        buffered_records.load(Ordering::Relaxed),
        if mode == "rotation-flush" { 3 } else { 6 },
        "failure must occur after successfully buffered writes"
    );
    assert_eq!(
        sink.health.dropped(),
        [1, 2, 1, 1, 1],
        "all unconfirmed records must be counted by level on {mode} failure"
    );
    assert_eq!(sink.health.fallback_count.load(Ordering::Relaxed), 1);
    assert!(sink.health.error().is_some());
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn ew1_shutdown_flush_failure_counts_all_buffered_records_by_level() {
    flush_failure("shutdown-flush");
}
#[test]
fn ew1_idle_flush_failure_counts_all_buffered_records_by_level() {
    flush_failure("idle-flush");
}
#[test]
fn ew1_rotation_flush_failure_counts_all_buffered_records_by_level() {
    flush_failure("rotation-flush");
}

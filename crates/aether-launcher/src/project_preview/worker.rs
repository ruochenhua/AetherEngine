use super::{
    cache::{Cache, PreviewState, Request},
    load, Limits, ResourceKind, Thumbnail,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

pub(crate) struct PreviewWorker {
    cache: Cache,
    sender: SyncSender<Request>,
    receiver: Receiver<(Request, Result<Thumbnail, String>)>,
    pending: Option<Request>,
    busy: bool,
}
impl PreviewWorker {
    pub fn new(root: PathBuf) -> Self {
        let (sender, requests) = mpsc::sync_channel::<Request>(1);
        let (results, receiver) = mpsc::sync_channel(1);
        // One worker per Project panel, one in-flight job and one latest pending selection.
        // No filesystem work, decode or CPU rasterization runs in the egui frame.
        std::thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                let outcome = load(&root, &request.path, request.kind, Limits::default());
                if results.send((request, outcome)).is_err() {
                    break;
                }
            }
        });
        Self {
            cache: Cache::new(8),
            sender,
            receiver,
            pending: None,
            busy: false,
        }
    }
    pub fn select(&mut self, selection: Option<(&str, ResourceKind)>) {
        if let Some(request) = self.cache.select(selection) {
            self.pending = Some(request);
        }
        if !matches!(self.cache.state, PreviewState::Loading) {
            self.pending = None;
        }
        self.dispatch();
    }
    pub fn poll(&mut self) {
        if let Ok((request, result)) = self.receiver.try_recv() {
            self.busy = false;
            self.cache.publish(&request, result);
        }
        self.dispatch();
    }
    pub fn invalidate(&mut self) {
        self.cache.invalidate();
        self.pending = None;
    }
    pub fn state(&self) -> &PreviewState {
        &self.cache.state
    }
    fn dispatch(&mut self) {
        if self.busy {
            return;
        }
        let Some(request) = self.pending.take() else {
            return;
        };
        match self.sender.try_send(request) {
            Ok(()) => self.busy = true,
            Err(TrySendError::Full(request)) => self.pending = Some(request),
            Err(TrySendError::Disconnected(request)) => {
                self.cache
                    .publish(&request, Err("Preview worker unavailable".into()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_worker_publishes_selected_preview_via_nonblocking_poll() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("aether-worker-{}", std::process::id()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        image::RgbaImage::new(80, 40)
            .save(root.join("assets/test.png"))
            .unwrap();
        let mut worker = PreviewWorker::new(root.clone());
        worker.select(Some(("assets/test.png", ResourceKind::Image)));
        assert!(matches!(worker.state(), PreviewState::Loading));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            worker.poll();
            if let PreviewState::Ready(thumbnail) = worker.state() {
                assert_eq!(thumbnail.dimensions, "Image: 80 × 40 px");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not publish"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        worker.invalidate();
        assert!(matches!(worker.state(), PreviewState::Idle));
        worker.select(None);
        std::fs::remove_dir_all(root).unwrap();
    }
}

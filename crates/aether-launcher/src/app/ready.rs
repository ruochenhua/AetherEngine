//! Optional supervisor handshake. AETHER_READY_FD is an inherited writable pipe.
//! Duplicate it through /dev/fd so malformed values cannot create an unsafe fd.
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write};

pub(super) struct ReadySignal<W: Write = File>(Option<W>);

impl ReadySignal<File> {
    pub(super) fn from_env() -> Self {
        Self::from_value(std::env::var_os("AETHER_READY_FD"))
    }

    fn from_value(value: Option<OsString>) -> Self {
        let fd = value
            .and_then(|value| value.into_string().ok())
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|fd| *fd > 2);
        Self(fd.and_then(|fd| {
            File::options()
                .write(true)
                .open(format!("/dev/fd/{fd}"))
                .ok()
        }))
    }
}

impl<W: Write> ReadySignal<W> {
    pub(super) fn presented(&mut self) -> io::Result<()> {
        if let Some(mut writer) = self.0.take() {
            writer.write_all(b"R")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn ready_signal_is_one_byte_exactly_once() {
        let capture = Capture(Arc::default());
        let mut ready = ReadySignal(Some(capture.clone()));
        ready.presented().unwrap();
        ready.presented().unwrap();
        assert_eq!(*capture.0.lock().unwrap(), b"R");
    }

    #[cfg(unix)]
    #[test]
    fn inherited_pipe_receives_ready_and_closes_duplicate() {
        use std::os::fd::AsRawFd;
        use std::process::{Command, Stdio};
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let writer = child.stdin.take().unwrap();
        let mut ready = ReadySignal::from_value(Some(writer.as_raw_fd().to_string().into()));
        assert!(ready.0.is_some());
        ready.presented().unwrap();
        ready.presented().unwrap();
        drop(writer);
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"R");
    }

    #[test]
    fn absent_or_invalid_ready_fd_is_disabled() {
        for value in [
            None,
            Some("not-a-fd".into()),
            Some("-1".into()),
            Some("1".into()),
        ] {
            let mut ready = ReadySignal::from_value(value);
            assert!(ready.0.is_none());
            ready.presented().unwrap();
        }
    }

    #[test]
    fn broken_pipe_is_attempted_once() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut ready = ReadySignal(Some(Broken));
        assert!(ready.presented().is_err());
        assert!(ready.presented().is_ok());
    }
}

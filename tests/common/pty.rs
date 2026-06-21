use std::{
    fmt, fs,
    io::{self, Read, Write},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, ExitStatus, PtySize, native_pty_system};

const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

pub enum PtyRunResult {
    Skipped(PtyUnavailable),
    Completed { status: ExitStatus, output: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyUnavailable {
    detail: String,
}

impl PtyUnavailable {
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for PtyUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "PTY support unavailable: {}", self.detail)
    }
}

impl std::error::Error for PtyUnavailable {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtySpawnError {
    Unavailable(PtyUnavailable),
    Infrastructure(String),
}

impl fmt::Display for PtySpawnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(formatter, "{reason}"),
            Self::Infrastructure(detail) => {
                write!(formatter, "PTY test infrastructure error: {detail}")
            }
        }
    }
}

impl std::error::Error for PtySpawnError {}

pub struct PtyTui {
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output_rx: mpsc::Receiver<PtyOutputEvent>,
    output: Vec<u8>,
    output_read_error: Option<PtyReadError>,
}

impl PtyTui {
    #[allow(dead_code)]
    pub fn spawn(args: &[&str]) -> Result<Self, PtySpawnError> {
        Self::spawn_with_env(args, &[])
    }

    pub fn spawn_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> Result<Self, PtySpawnError> {
        let binary = compiled_binary_path()?;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| {
                PtySpawnError::Unavailable(PtyUnavailable::new(format!(
                    "failed to open a pseudo-terminal for TUI integration tests: {error}"
                )))
            })?;

        let reader = pair.master.try_clone_reader().map_err(|error| {
            PtySpawnError::Infrastructure(format!(
                "failed to clone PTY reader after opening PTY: {error}"
            ))
        })?;
        let writer = pair.master.take_writer().map_err(|error| {
            PtySpawnError::Infrastructure(format!(
                "failed to open PTY writer after opening PTY: {error}"
            ))
        })?;

        let (output_tx, output_rx) = mpsc::channel();
        thread::spawn(move || read_pty_output(reader, output_tx));

        let mut command = CommandBuilder::new(&binary);
        command.args(args);
        command.env("TERM", "xterm-256color");
        command.env("AIRGRADIENT_CLI_FETCH_TIMEOUT_MS", "1000");
        for (name, value) in env_vars {
            command.env(name, value);
        }

        let child = pair.slave.spawn_command(command).map_err(|error| {
            PtySpawnError::Infrastructure(format!(
                "failed to spawn {} in PTY with args [{}]: {error}",
                binary.display(),
                args.join(" ")
            ))
        })?;
        drop(pair.slave);

        Ok(Self {
            child,
            writer,
            output_rx,
            output: Vec::new(),
            output_read_error: None,
        })
    }

    #[allow(dead_code)]
    pub fn spawn_or_skip(args: &[&str], context: &str) -> Result<Self, PtyUnavailable> {
        Self::spawn_or_skip_with_env(args, &[], context)
    }

    pub fn spawn_or_skip_with_env(
        args: &[&str],
        env_vars: &[(&str, &str)],
        context: &str,
    ) -> Result<Self, PtyUnavailable> {
        match Self::spawn_with_env(args, env_vars) {
            Ok(tui) => Ok(tui),
            Err(PtySpawnError::Unavailable(reason)) => Err(reason),
            Err(PtySpawnError::Infrastructure(reason)) => {
                panic!("PTY test infrastructure failed while {context}: {reason}")
            }
        }
    }

    #[allow(dead_code)]
    pub fn press_q(&mut self) {
        self.write_all(b"q");
    }

    #[allow(dead_code)]
    pub fn press_escape(&mut self) {
        self.write_all(b"\x1b");
    }

    #[allow(dead_code)]
    pub fn press_refresh(&mut self) {
        self.write_all(b"r");
    }

    pub fn write_all(&mut self, bytes: &[u8]) {
        self.writer
            .write_all(bytes)
            .expect("input should be written to PTY");
        self.writer.flush().expect("input should be flushed to PTY");
    }

    pub fn wait_for_exit(&mut self, timeout: Duration) -> PtyRunResult {
        let started = Instant::now();

        loop {
            self.drain_output();

            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.drain_output_for(OUTPUT_DRAIN_TIMEOUT);
                    self.panic_on_output_read_error(Some(&status));
                    return PtyRunResult::Completed {
                        status,
                        output: self.output.clone(),
                    };
                }
                Ok(None) if self.output_read_error.is_some() => {
                    let _ = self.child.kill();
                    let child_status = self.child.wait();
                    self.drain_output_for(OUTPUT_DRAIN_TIMEOUT);
                    self.panic_on_output_read_error_with_child_status(format_cleanup_child_status(
                        child_status.as_ref(),
                    ));
                }
                Ok(None) if started.elapsed() < timeout => {
                    thread::sleep(Duration::from_millis(25));
                }
                Ok(None) => {
                    let _ = self.child.kill();
                    let child_status = self.child.wait();
                    self.drain_output_for(OUTPUT_DRAIN_TIMEOUT);
                    let read_error = self.output_read_error.as_ref().map(|error| {
                        format!(
                            "\nPTY output reader failed before timeout cleanup completed: {error}"
                        )
                    });
                    let child_status = format_cleanup_child_status(child_status.as_ref());
                    panic!(
                        "TUI process did not exit within {:?}; child status after cleanup: {}; output:\n{}{}",
                        timeout,
                        child_status,
                        String::from_utf8_lossy(&self.output),
                        read_error.as_deref().unwrap_or("")
                    );
                }
                Err(error) => panic!(
                    "failed to poll TUI child status: {error}; output read error: {}; output:\n{}",
                    self.output_read_error
                        .as_ref()
                        .map_or_else(|| "none".to_string(), ToString::to_string),
                    String::from_utf8_lossy(&self.output)
                ),
            }
        }
    }

    fn drain_output(&mut self) {
        while let Ok(event) = self.output_rx.try_recv() {
            self.record_output_event(event);
        }
    }

    fn drain_output_for(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            match self.output_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(event) => self.record_output_event(event),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        self.drain_output();
    }

    fn record_output_event(&mut self, event: PtyOutputEvent) {
        match event {
            PtyOutputEvent::Chunk(chunk) => self.output.extend_from_slice(&chunk),
            PtyOutputEvent::ReadError(error) => {
                self.output_read_error.get_or_insert(error);
            }
        }
    }

    fn panic_on_output_read_error(&self, child_status: Option<&ExitStatus>) {
        if self.output_read_error.is_some() {
            self.panic_on_output_read_error_with_child_status(format_child_status(child_status));
        }
    }

    fn panic_on_output_read_error_with_child_status(&self, child_status: String) -> ! {
        let read_error = self
            .output_read_error
            .as_ref()
            .expect("read error should be present before panicking");
        panic!(
            "PTY output reader failed unexpectedly: {read_error}; child status: {}; output:\n{}",
            child_status,
            String::from_utf8_lossy(&self.output)
        );
    }
}

impl Drop for PtyTui {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub fn report_conditional_skip(prefix: &str, reason: &dyn fmt::Display, coverage: &str) {
    eprintln!("{prefix}: {reason}. {coverage}");
}

fn compiled_binary_path() -> Result<PathBuf, PtySpawnError> {
    let Some(binary) = std::env::var_os("CARGO_BIN_EXE_airgradient-cli") else {
        return Err(PtySpawnError::Infrastructure(
            "CARGO_BIN_EXE_airgradient-cli is not set; run this helper from Cargo integration tests so Cargo provides the compiled binary path".to_string(),
        ));
    };

    if binary.is_empty() {
        return Err(PtySpawnError::Infrastructure(
            "CARGO_BIN_EXE_airgradient-cli is set to an empty path".to_string(),
        ));
    }

    let binary = PathBuf::from(binary);
    let metadata = fs::metadata(&binary).map_err(|error| {
        PtySpawnError::Infrastructure(format!(
            "CARGO_BIN_EXE_airgradient-cli points to {}, but it could not be inspected: {error}",
            binary.display()
        ))
    })?;

    if !metadata.is_file() {
        return Err(PtySpawnError::Infrastructure(format!(
            "CARGO_BIN_EXE_airgradient-cli points to {}, but that path is not a file",
            binary.display()
        )));
    }

    Ok(binary)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PtyReadError {
    kind: io::ErrorKind,
    raw_os_error: Option<i32>,
    message: String,
}

impl From<io::Error> for PtyReadError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
            message: error.to_string(),
        }
    }
}

impl fmt::Display for PtyReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.raw_os_error {
            Some(raw_os_error) => write!(
                formatter,
                "{} (kind: {:?}, raw OS error: {})",
                self.message, self.kind, raw_os_error
            ),
            None => write!(formatter, "{} (kind: {:?})", self.message, self.kind),
        }
    }
}

enum PtyOutputEvent {
    Chunk(Vec<u8>),
    ReadError(PtyReadError),
}

fn read_pty_output(mut reader: Box<dyn Read + Send>, output_tx: mpsc::Sender<PtyOutputEvent>) {
    let mut buffer = [0; 4096];

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => {
                if output_tx
                    .send(PtyOutputEvent::Chunk(buffer[..bytes_read].to_vec()))
                    .is_err()
                {
                    break;
                }
            }
            Err(error) if is_closed_pty_error(&error) => break,
            Err(error) => {
                let _ = output_tx.send(PtyOutputEvent::ReadError(error.into()));
                break;
            }
        }
    }
}

fn format_child_status(child_status: Option<&ExitStatus>) -> String {
    child_status.map_or_else(|| "unavailable".to_string(), ToString::to_string)
}

fn format_cleanup_child_status(child_status: Result<&ExitStatus, &io::Error>) -> String {
    match child_status {
        Ok(status) => status.to_string(),
        Err(error) => format!("unavailable after cleanup failed: {error}"),
    }
}

fn is_closed_pty_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
    ) || is_closed_pty_raw_os_error(error.raw_os_error())
}

fn is_closed_pty_raw_os_error(raw_os_error: Option<i32>) -> bool {
    // Reading from a PTY master after the slave closes can report EIO on
    // supported Unix targets. Keep this target-scoped: on Windows, raw OS
    // error 5 means ERROR_ACCESS_DENIED and must not be classified as an
    // expected PTY close.
    raw_os_error.is_some() && raw_os_error == closed_pty_eio_raw_os_error()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const PTY_CLOSED_EIO_RAW_OS_ERROR: i32 = 5;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos"
))]
const PTY_CLOSED_EIO_RAW_OS_ERROR: i32 = 5;

#[cfg(any(
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const PTY_CLOSED_EIO_RAW_OS_ERROR: i32 = 5;

#[cfg(any(target_os = "illumos", target_os = "solaris"))]
const PTY_CLOSED_EIO_RAW_OS_ERROR: i32 = 5;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly",
    target_os = "illumos",
    target_os = "solaris"
))]
fn closed_pty_eio_raw_os_error() -> Option<i32> {
    Some(PTY_CLOSED_EIO_RAW_OS_ERROR)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "watchos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly",
    target_os = "illumos",
    target_os = "solaris"
)))]
fn closed_pty_eio_raw_os_error() -> Option<i32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_pty_error_classification_accepts_expected_terminal_close_errors() {
        for kind in [
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::ConnectionReset,
        ] {
            let error = io::Error::new(kind, "terminal closed");

            assert!(
                is_closed_pty_error(&error),
                "{kind:?} should be treated as an expected closed-PTY read"
            );
        }
    }

    #[test]
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
        target_os = "illumos",
        target_os = "solaris"
    ))]
    fn closed_pty_eio_mapping_is_available_for_supported_targets() {
        assert_eq!(
            closed_pty_eio_raw_os_error(),
            Some(PTY_CLOSED_EIO_RAW_OS_ERROR)
        );
        assert_eq!(
            io::Error::from_raw_os_error(PTY_CLOSED_EIO_RAW_OS_ERROR).raw_os_error(),
            Some(PTY_CLOSED_EIO_RAW_OS_ERROR)
        );
    }

    #[test]
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
        target_os = "illumos",
        target_os = "solaris"
    ))]
    fn closed_pty_error_classification_accepts_supported_unix_eio() {
        let error = io::Error::from_raw_os_error(PTY_CLOSED_EIO_RAW_OS_ERROR);

        assert!(
            is_closed_pty_error(&error),
            "supported Unix EIO should be treated as an expected closed-PTY read"
        );
    }

    #[test]
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly",
        target_os = "illumos",
        target_os = "solaris"
    )))]
    fn closed_pty_error_classification_rejects_raw_error_5_without_supported_mapping() {
        const WINDOWS_ERROR_ACCESS_DENIED_RAW_OS_ERROR: i32 = 5;

        let error = io::Error::from_raw_os_error(WINDOWS_ERROR_ACCESS_DENIED_RAW_OS_ERROR);

        assert!(
            !is_closed_pty_error(&error),
            "raw OS error 5 should not be suppressed without a supported PTY EIO mapping"
        );
    }

    #[test]
    fn closed_pty_error_classification_rejects_unexpected_read_errors() {
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::InvalidData,
            io::ErrorKind::TimedOut,
        ] {
            let error = io::Error::new(kind, "unexpected reader failure");

            assert!(
                !is_closed_pty_error(&error),
                "{kind:?} should be retained as an unexpected PTY read error"
            );
        }
    }

    #[test]
    fn spawn_error_display_distinguishes_unavailable_from_infrastructure() {
        let unavailable = PtySpawnError::Unavailable(PtyUnavailable::new("openpty failed"));
        let infrastructure = PtySpawnError::Infrastructure("missing binary path".to_string());

        assert_eq!(
            unavailable.to_string(),
            "PTY support unavailable: openpty failed"
        );
        assert_eq!(
            infrastructure.to_string(),
            "PTY test infrastructure error: missing binary path"
        );
    }

    #[test]
    fn spawn_error_variants_support_branching_without_string_matching() {
        let unavailable = PtySpawnError::Unavailable(PtyUnavailable::new("openpty failed"));
        let infrastructure = PtySpawnError::Infrastructure("missing binary path".to_string());

        assert!(matches!(unavailable, PtySpawnError::Unavailable(_)));
        assert!(matches!(infrastructure, PtySpawnError::Infrastructure(_)));
    }

    #[test]
    fn skipped_run_result_only_carries_unavailable_pty_support() {
        let result = PtyRunResult::Skipped(PtyUnavailable::new("openpty failed"));

        match result {
            PtyRunResult::Skipped(reason) => {
                assert_eq!(
                    reason.to_string(),
                    "PTY support unavailable: openpty failed"
                );
            }
            PtyRunResult::Completed { .. } => {
                panic!("test constructed a skipped PTY result");
            }
        }
    }
}

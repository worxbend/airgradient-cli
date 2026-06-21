use std::{
    io::{self, Read, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use portable_pty::{native_pty_system, CommandBuilder, ExitStatus, PtySize};
use tempfile::tempdir;

const NON_TTY_ERROR: &str = "TUI requires an interactive terminal";
const PTY_SKIP_PREFIX: &str = "conditional PTY smoke coverage skipped";
const EXIT_TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn tui_exits_when_q_is_pressed_in_pty() {
    let result = run_tui_in_pty(b"q");
    assert_pty_run_exited_cleanly(result);
}

#[test]
fn tui_exits_when_escape_is_pressed_in_pty() {
    let result = run_tui_in_pty(b"\x1b");
    assert_pty_run_exited_cleanly(result);
}

fn assert_pty_run_exited_cleanly(result: PtyRunResult) {
    match result {
        PtyRunResult::Skipped(reason) => {
            eprintln!(
                "{PTY_SKIP_PREFIX}: {reason}. Runtime harness tests cover TUI shutdown behavior without a platform PTY."
            );
        }
        PtyRunResult::Completed { status, output } => {
            assert!(
                status.success(),
                "TUI exited unsuccessfully: {status}; output:\n{}",
                String::from_utf8_lossy(&output)
            );
            assert!(
                !String::from_utf8_lossy(&output).contains(NON_TTY_ERROR),
                "TUI reported non-interactive terminal inside a PTY; output:\n{}",
                String::from_utf8_lossy(&output)
            );
        }
    }
}

fn run_tui_in_pty(input: &[u8]) -> PtyRunResult {
    let binary = match std::env::var("CARGO_BIN_EXE_airgradient-cli") {
        Ok(binary) => binary,
        Err(error) => panic!("compiled binary path should be available: {error}"),
    };

    let tempdir = tempdir().expect("temp config dir should be created");
    let config_path = tempdir.path().join("config.json");
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(error) => return PtyRunResult::Skipped(format!("failed to open PTY: {error}")),
    };

    let reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => return PtyRunResult::Skipped(format!("failed to clone PTY reader: {error}")),
    };
    let mut writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => return PtyRunResult::Skipped(format!("failed to open PTY writer: {error}")),
    };

    let (output_tx, output_rx) = mpsc::channel();
    thread::spawn(move || read_pty_output(reader, output_tx));

    let mut command = CommandBuilder::new(binary);
    command.args(["--config", path_str(&config_path), "--tui"]);

    let mut child = match pair.slave.spawn_command(command) {
        Ok(child) => child,
        Err(error) => {
            return PtyRunResult::Skipped(format!("failed to spawn command in PTY: {error}"));
        }
    };
    drop(pair.slave);

    thread::sleep(Duration::from_millis(100));
    writer
        .write_all(input)
        .expect("exit key should be written to PTY");
    writer.flush().expect("exit key should be flushed to PTY");

    let mut output = Vec::new();
    let started = Instant::now();
    let status = loop {
        drain_output(&output_rx, &mut output);

        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < EXIT_TIMEOUT => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                drain_output_for(&output_rx, &mut output, Duration::from_millis(100));
                panic!(
                    "TUI did not exit within {:?}; output:\n{}",
                    EXIT_TIMEOUT,
                    String::from_utf8_lossy(&output)
                );
            }
            Err(error) => panic!("failed to poll TUI child status: {error}"),
        }
    };

    drop(writer);
    drain_output_for(&output_rx, &mut output, Duration::from_millis(100));

    PtyRunResult::Completed { status, output }
}

fn read_pty_output(mut reader: Box<dyn Read + Send>, output_tx: mpsc::Sender<Vec<u8>>) {
    let mut buffer = [0; 4096];

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => {
                if output_tx.send(buffer[..bytes_read].to_vec()).is_err() {
                    break;
                }
            }
            Err(error) if is_closed_pty_error(&error) => break,
            Err(_) => break,
        }
    }
}

fn drain_output(output_rx: &mpsc::Receiver<Vec<u8>>, output: &mut Vec<u8>) {
    while let Ok(chunk) = output_rx.try_recv() {
        output.extend_from_slice(&chunk);
    }
}

fn drain_output_for(output_rx: &mpsc::Receiver<Vec<u8>>, output: &mut Vec<u8>, timeout: Duration) {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        match output_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(chunk) => output.extend_from_slice(&chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    drain_output(output_rx, output);
}

fn is_closed_pty_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
    ) || error.raw_os_error() == Some(5)
}

fn path_str(path: &std::path::Path) -> &str {
    path.to_str().expect("test path should be valid UTF-8")
}

enum PtyRunResult {
    Skipped(String),
    Completed { status: ExitStatus, output: Vec<u8> },
}

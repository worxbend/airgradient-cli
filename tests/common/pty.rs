use std::{
    io::{self, Read, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, ExitStatus, PtySize, native_pty_system};

const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

pub enum PtyRunResult {
    Skipped(String),
    Completed { status: ExitStatus, output: Vec<u8> },
}

pub struct PtyTui {
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output_rx: mpsc::Receiver<Vec<u8>>,
    output: Vec<u8>,
}

impl PtyTui {
    pub fn spawn(args: &[&str]) -> Result<Self, String> {
        let binary = std::env::var("CARGO_BIN_EXE_airgradient-cli")
            .map_err(|error| format!("compiled binary path unavailable: {error}"))?;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("failed to open PTY: {error}"))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("failed to clone PTY reader: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("failed to open PTY writer: {error}"))?;

        let (output_tx, output_rx) = mpsc::channel();
        thread::spawn(move || read_pty_output(reader, output_tx));

        let mut command = CommandBuilder::new(binary);
        command.args(args);
        command.env("TERM", "xterm-256color");
        command.env("AIRGRADIENT_CLI_FETCH_TIMEOUT_MS", "1000");

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("failed to spawn command in PTY: {error}"))?;
        drop(pair.slave);

        Ok(Self {
            child,
            writer,
            output_rx,
            output: Vec::new(),
        })
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
                    return PtyRunResult::Completed {
                        status,
                        output: self.output.clone(),
                    };
                }
                Ok(None) if started.elapsed() < timeout => {
                    thread::sleep(Duration::from_millis(25));
                }
                Ok(None) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    self.drain_output_for(OUTPUT_DRAIN_TIMEOUT);
                    panic!(
                        "TUI process did not exit within {:?}; output:\n{}",
                        timeout,
                        String::from_utf8_lossy(&self.output)
                    );
                }
                Err(error) => panic!("failed to poll TUI child status: {error}"),
            }
        }
    }

    fn drain_output(&mut self) {
        while let Ok(chunk) = self.output_rx.try_recv() {
            self.output.extend_from_slice(&chunk);
        }
    }

    fn drain_output_for(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            match self.output_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(chunk) => self.output.extend_from_slice(&chunk),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        self.drain_output();
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

pub fn report_conditional_skip(prefix: &str, reason: &str, coverage: &str) {
    eprintln!("{prefix}: {reason}. {coverage}");
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

fn is_closed_pty_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
    ) || error.raw_os_error() == Some(5)
}

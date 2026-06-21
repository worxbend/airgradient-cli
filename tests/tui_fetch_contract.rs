mod common;

use std::{
    error::Error,
    fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use common::pty::{PtyRunResult, PtySpawnError, PtyTui, report_conditional_skip};
use tempfile::TempDir;

const NON_TTY_ERROR: &str = "TUI requires an interactive terminal";
const PTY_SKIP_PREFIX: &str = "conditional PTY fetch contract coverage skipped";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(4);
const EXIT_TIMEOUT: Duration = Duration::from_secs(3);
const REFRESH_OVERRIDE_OBSERVATION: Duration = Duration::from_millis(5500);

#[test]
fn tui_startup_success_requests_current_measures_endpoint() -> Result<(), Box<dyn Error>> {
    let server = TestServer::start(ServerResponse::Success)?;
    let server_uri = server.uri();
    let config = TestConfig::missing();

    let result = run_tui_until(
        &[
            "--config",
            config.path_str(),
            "--tui",
            "--url",
            &server_uri,
            "--refresh",
            "3600",
        ],
        |tui| {
            assert!(
                server.wait_for_current_count(1, STARTUP_TIMEOUT),
                "TUI did not request /measures/current; observed paths: {:?}",
                server.paths()
            );
            tui.press_q();
        },
    );

    let _ = assert_completed_cleanly(result);
    Ok(())
}

#[test]
fn tui_startup_failure_still_requests_current_measures_endpoint() -> Result<(), Box<dyn Error>> {
    let server = TestServer::start(ServerResponse::Failure)?;
    let server_uri = server.uri();
    let config = TestConfig::missing();

    let result = run_tui_until(
        &[
            "--config",
            config.path_str(),
            "--tui",
            "--url",
            &server_uri,
            "--refresh",
            "3600",
        ],
        |tui| {
            assert!(
                server.wait_for_current_count(1, STARTUP_TIMEOUT),
                "TUI did not request /measures/current for failed startup fetch; observed paths: {:?}",
                server.paths()
            );
            tui.press_q();
        },
    );

    let _ = assert_completed_cleanly(result);
    Ok(())
}

#[test]
fn tui_manual_refresh_requests_current_measures_endpoint_again() -> Result<(), Box<dyn Error>> {
    let server = TestServer::start(ServerResponse::Success)?;
    let server_uri = server.uri();
    let config = TestConfig::missing();

    let result = run_tui_until(
        &[
            "--config",
            config.path_str(),
            "--tui",
            "--url",
            &server_uri,
            "--refresh",
            "3600",
        ],
        |tui| {
            assert!(
                server.wait_for_current_count(1, STARTUP_TIMEOUT),
                "TUI did not perform the initial fetch; observed paths: {:?}",
                server.paths()
            );

            tui.press_refresh();

            assert!(
                server.wait_for_current_count(2, STARTUP_TIMEOUT),
                "manual refresh did not request /measures/current again; observed paths: {:?}",
                server.paths()
            );
            tui.press_q();
        },
    );

    let _ = assert_completed_cleanly(result);
    Ok(())
}

#[test]
fn tui_cli_url_and_refresh_overrides_take_precedence_over_config() -> Result<(), Box<dyn Error>> {
    let configured_server = TestServer::start(ServerResponse::Success)?;
    let override_server = TestServer::start(ServerResponse::Success)?;
    let configured_uri = configured_server.uri();
    let override_uri = override_server.uri();
    let config = TestConfig::with_server_url_and_refresh(&configured_uri, 5)?;

    let result = run_tui_until(
        &[
            "--config",
            config.path_str(),
            "--tui",
            "--url",
            &override_uri,
            "--refresh",
            "3600",
        ],
        |tui| {
            assert!(
                override_server.wait_for_current_count(1, STARTUP_TIMEOUT),
                "TUI did not request the CLI --url server; observed override paths: {:?}",
                override_server.paths()
            );
            assert_eq!(
                configured_server.current_count(),
                0,
                "TUI requested the config-file server despite a CLI --url override"
            );

            thread::sleep(REFRESH_OVERRIDE_OBSERVATION);

            assert_eq!(
                override_server.current_count(),
                1,
                "config refresh interval was used despite a CLI --refresh override"
            );
            assert_eq!(
                configured_server.current_count(),
                0,
                "config-file server was requested while the CLI --url override was active"
            );
            tui.press_q();
        },
    );

    let output = assert_completed_cleanly(result);
    if !output.is_empty() {
        assert!(
            String::from_utf8_lossy(&output).contains("1h"),
            "TUI did not render the CLI --refresh override; output:\n{}",
            String::from_utf8_lossy(&output)
        );
    }
    Ok(())
}

fn assert_completed_cleanly(result: PtyRunResult) -> Vec<u8> {
    match result {
        PtyRunResult::Skipped(reason) => {
            report_conditional_skip(
                PTY_SKIP_PREFIX,
                &reason,
                "Runtime harness tests cover TUI fetch lifecycle behavior without a platform PTY.",
            );
            Vec::new()
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
            output
        }
    }
}

fn run_tui_until(args: &[&str], exercise: impl FnOnce(&mut PtyTui)) -> PtyRunResult {
    let mut tui = match PtyTui::spawn(args) {
        Ok(tui) => tui,
        Err(PtySpawnError::Unavailable(reason)) => {
            return PtyRunResult::Skipped(PtySpawnError::Unavailable(reason));
        }
        Err(PtySpawnError::Infrastructure(reason)) => {
            panic!(
                "PTY test infrastructure failed while starting TUI fetch contract test: {reason}"
            )
        }
    };

    exercise(&mut tui);
    tui.wait_for_exit(EXIT_TIMEOUT)
}

struct TestConfig {
    _dir: TempDir,
    path: PathBuf,
}

impl TestConfig {
    fn missing() -> Self {
        let dir = tempfile::tempdir().expect("temp config dir should be created");
        let path = dir.path().join("missing-config.json");
        Self { _dir: dir, path }
    }

    fn with_server_url_and_refresh(
        server_url: &str,
        refresh_interval_secs: u64,
    ) -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.json");
        fs::write(
            &path,
            serde_json::json!({
                "server_url": server_url,
                "refresh_interval_secs": refresh_interval_secs,
                "notifications_enabled": true,
                "start_minimized": false
            })
            .to_string(),
        )?;
        Ok(Self { _dir: dir, path })
    }

    fn path_str(&self) -> &str {
        path_str(&self.path)
    }
}

#[derive(Debug, Clone, Copy)]
enum ServerResponse {
    Success,
    Failure,
}

struct TestServer {
    addr: SocketAddr,
    current_count: Arc<AtomicUsize>,
    paths: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    fn start(response: ServerResponse) -> Result<Self, Box<dyn Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let current_count = Arc::new(AtomicUsize::new(0));
        let paths = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        Ok(Self {
            addr,
            current_count: Arc::clone(&current_count),
            paths: Arc::clone(&paths),
            shutdown: Arc::clone(&shutdown),
            thread: Some(thread::spawn(move || {
                serve_http(listener, response, current_count, paths, shutdown);
            })),
        })
    }

    fn uri(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn current_count(&self) -> usize {
        self.current_count.load(Ordering::SeqCst)
    }

    fn paths(&self) -> Vec<String> {
        self.paths.lock().expect("paths lock not poisoned").clone()
    }

    fn wait_for_current_count(&self, expected: usize, timeout: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if self.current_count() >= expected {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }

        self.current_count() >= expected
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_http(
    listener: TcpListener,
    response: ServerResponse,
    current_count: Arc<AtomicUsize>,
    paths: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                handle_connection(&mut stream, response, &current_count, &paths);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(
    stream: &mut TcpStream,
    response: ServerResponse,
    current_count: &AtomicUsize,
    paths: &Mutex<Vec<String>>,
) {
    let mut request = [0; 2048];
    let Ok(read) = stream.read(&mut request) else {
        return;
    };

    let request = String::from_utf8_lossy(&request[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("")
        .to_owned();

    paths
        .lock()
        .expect("paths lock not poisoned")
        .push(path.clone());
    if path == "/measures/current" {
        current_count.fetch_add(1, Ordering::SeqCst);
    }

    let (status, body) = match response {
        ServerResponse::Success => (
            "200 OK",
            r#"{"rco2":612,"pm02":7.4,"atmpCompensated":21.8}"#,
        ),
        ServerResponse::Failure => ("503 Service Unavailable", r#"{"error":"offline"}"#),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("test path should be valid UTF-8")
}

mod common;

use std::{path::Path, time::Duration};

use common::pty::{PtyRunResult, PtyTui, report_conditional_skip};
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
            report_conditional_skip(
                PTY_SKIP_PREFIX,
                &reason,
                "Runtime harness tests cover TUI shutdown behavior without a platform PTY.",
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
    let tempdir = tempdir().expect("temp config dir should be created");
    let config_path = tempdir.path().join("config.json");
    let args = ["--config", path_str(&config_path), "--tui"];
    let mut tui = match PtyTui::spawn(&args) {
        Ok(tui) => tui,
        Err(reason) => return PtyRunResult::Skipped(reason),
    };

    std::thread::sleep(Duration::from_millis(100));
    match input {
        b"q" => tui.press_q(),
        b"\x1b" => tui.press_escape(),
        bytes => tui.write_all(bytes),
    };

    tui.wait_for_exit(EXIT_TIMEOUT)
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("test path should be valid UTF-8")
}

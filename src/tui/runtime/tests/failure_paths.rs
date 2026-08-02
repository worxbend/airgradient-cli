//! What the loop does when the terminal or a fetch fails: cancellation of
//! in-flight work, error context that survives unwinding, and cleanup.

use std::sync::atomic::Ordering;

use super::*;

#[tokio::test]
async fn quit_while_fetch_is_pending_requests_cancellation() {
    let mut terminal = HarnessTerminal::with_quit();
    let mut fetcher = HarnessFetcher::pending_then([None]);
    let mut app = app(Some(configured_url()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls.len(), 1);
    assert_eq!(fetcher.canceled_fetches, 1);
    assert_eq!(terminal.calls.last(), Some(&RuntimeCall::Cleanup));
    assert_eq!(app.current_snapshot, None);
    assert!(!app.is_fetching);
}

#[tokio::test]
async fn quit_while_spawned_fetch_is_pending_observes_cancellation() {
    let mut terminal = HarnessTerminal::with_quit();
    let mut fetcher = SpawnedPendingFetcher::default();
    let mut app = app(Some(configured_url()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls.len(), 1);
    assert!(fetcher.active_handle.is_none());
    assert!(fetcher.cancellation_observed);
    assert!(!app.is_fetching);
    assert!(terminal.cleanup_called);
}

#[tokio::test(flavor = "current_thread")]
async fn current_thread_runtime_progresses_fetch_while_terminal_poll_blocks() {
    let fetch_completed = Arc::new(AtomicBool::new(false));
    let mut terminal = BlockingPollTerminal::new(Duration::from_millis(25));
    let mut fetcher = YieldingFetcher::new(fetch_completed.clone());
    let mut app = app(Some(configured_url()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert!(terminal.cleanup_called);
    assert_eq!(fetcher.calls.len(), 1);
    assert!(
        fetch_completed.load(Ordering::SeqCst),
        "background fetch task should progress while terminal polling is blocking"
    );
}

#[tokio::test]
async fn draw_failure_cancels_pending_fetch() {
    let mut terminal = HarnessTerminal::with_quit().fail_draw("draw failed");
    let mut fetcher = HarnessFetcher::pending_then([None]);
    let mut app = app(Some(configured_url()));

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("draw failure should be returned");

    assert_eq!(terminal_error_message(&error), "draw failed");
    assert_eq!(fetcher.calls.len(), 1);
    assert_eq!(fetcher.canceled_fetches, 1);
    assert_eq!(app.current_snapshot, None);
    assert!(!app.is_fetching);
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn draw_failure_retains_panicked_fetch_context_from_cancellation() {
    let mut terminal = HarnessTerminal::with_quit().fail_draw("draw failed");
    let mut fetcher = HarnessFetcher::pending_then([None]).fail_cancel("fetch task exploded");
    let mut app = app(Some(configured_url()));

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("draw failure should retain fetch cancellation context");

    assert_eq!(terminal_error_message(&error), "draw failed");
    assert_eq!(
        secondary_error_message(&error).as_deref(),
        Some("background fetch task failed: fetch task exploded")
    );
    assert_eq!(fetcher.canceled_fetches, 1);
    assert!(!app.is_fetching);
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn poll_failure_cancels_pending_fetch() {
    let mut terminal = HarnessTerminal::with_events([]).fail_poll("poll failed");
    let mut fetcher = HarnessFetcher::pending_then([None]);
    let mut app = app(Some(configured_url()));

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("poll failure should be returned");

    assert_eq!(terminal_error_message(&error), "poll failed");
    assert_eq!(fetcher.calls.len(), 1);
    assert_eq!(fetcher.canceled_fetches, 1);
    assert_eq!(app.current_snapshot, None);
    assert!(!app.is_fetching);
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn poll_failure_retains_fetch_cancellation_failure_context() {
    let mut terminal = HarnessTerminal::with_events([]).fail_poll("poll failed");
    let mut fetcher = HarnessFetcher::pending_then([None]).fail_cancel("fetch cancellation failed");
    let mut app = app(Some(configured_url()));

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("poll failure should retain fetch cancellation context");

    assert_eq!(terminal_error_message(&error), "poll failed");
    assert_eq!(
        secondary_error_message(&error).as_deref(),
        Some("background fetch task failed: fetch cancellation failed")
    );
    assert_eq!(fetcher.canceled_fetches, 1);
    assert!(!app.is_fetching);
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn read_failure_cancels_pending_fetch() {
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]).fail_read("read failed");
    let mut fetcher = HarnessFetcher::pending_then([None]);
    let mut app = app(Some(configured_url()));

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("read failure should be returned");

    assert_eq!(terminal_error_message(&error), "read failed");
    assert_eq!(fetcher.calls.len(), 1);
    assert_eq!(fetcher.canceled_fetches, 1);
    assert_eq!(app.current_snapshot, None);
    assert!(!app.is_fetching);
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn read_failure_retains_fetch_cancellation_failure_context() {
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]).fail_read("read failed");
    let mut fetcher = HarnessFetcher::pending_then([None]).fail_cancel("fetch cancellation failed");
    let mut app = app(Some(configured_url()));

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("read failure should retain fetch cancellation context");

    assert_eq!(terminal_error_message(&error), "read failed");
    assert_eq!(
        secondary_error_message(&error).as_deref(),
        Some("background fetch task failed: fetch cancellation failed")
    );
    assert_eq!(fetcher.canceled_fetches, 1);
    assert!(!app.is_fetching);
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn stale_completion_after_cancellation_does_not_mutate_app() {
    let url = configured_url();
    let mut app = app(Some(url.clone()));
    let mut fetcher =
        HarnessFetcher::pending_then([None]).with_stale_after_cancel([Ok(successful_payload())]);
    let mut scheduler = FetchScheduler::default();

    scheduler.request_refresh(&mut app, &mut fetcher);
    scheduler
        .cancel_pending_fetch(&mut app, &mut fetcher)
        .await
        .expect("cancellation should be observed");

    assert!(
        !scheduler
            .apply_ready_results(&mut app, &mut fetcher)
            .await
            .expect("stale completion drain should not fail")
    );
    assert_eq!(fetcher.calls, [url]);
    assert_eq!(fetcher.canceled_fetches, 1);
    assert_eq!(app.current_snapshot, None);
    assert_eq!(app.current_error, None);
    assert!(!app.is_fetching);
}

#[tokio::test]
async fn panicked_fetch_task_is_returned_as_runtime_error_when_observed() {
    let handle = tokio::spawn(async {
        panic!("fetch task exploded");
    });

    let error = observe_fetch_handle(handle)
        .await
        .expect_err("panicked fetch task should be surfaced");

    assert!(terminal_error_message(&error).contains("fetch task exploded"));
}

#[tokio::test]
async fn cleanup_runs_after_draw_failure() {
    let mut terminal = HarnessTerminal::with_quit().fail_draw("draw failed");
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("draw failure should be returned");

    assert_eq!(terminal_error_message(&error), "draw failed");
    assert_eq!(
        terminal.calls,
        [RuntimeCall::Enter, RuntimeCall::Draw, RuntimeCall::Cleanup]
    );
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn cleanup_failure_context_is_retained_after_draw_failure() {
    let mut terminal = HarnessTerminal::with_quit()
        .fail_draw("draw failed")
        .fail_cleanup("cleanup failed");
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("draw failure should be returned with cleanup context");

    assert_eq!(terminal_error_message(&error), "draw failed");
    assert_eq!(
        cleanup_error_message(&error).as_deref(),
        Some("cleanup failed")
    );
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn cleanup_runs_after_poll_failure() {
    let mut terminal = HarnessTerminal::with_events([]).fail_poll("poll failed");
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("poll failure should be returned");

    assert_eq!(terminal_error_message(&error), "poll failed");
    assert_eq!(
        terminal.calls,
        [
            RuntimeCall::Enter,
            RuntimeCall::Draw,
            RuntimeCall::Poll,
            RuntimeCall::Cleanup,
        ]
    );
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn cleanup_failure_context_is_retained_after_poll_failure() {
    let mut terminal = HarnessTerminal::with_events([])
        .fail_poll("poll failed")
        .fail_cleanup("cleanup failed");
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("poll failure should be returned with cleanup context");

    assert_eq!(terminal_error_message(&error), "poll failed");
    assert_eq!(
        cleanup_error_message(&error).as_deref(),
        Some("cleanup failed")
    );
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn cleanup_runs_after_read_failure() {
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]).fail_read("read failed");
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("read failure should be returned");

    assert_eq!(terminal_error_message(&error), "read failed");
    assert_eq!(
        terminal.calls,
        [
            RuntimeCall::Enter,
            RuntimeCall::Draw,
            RuntimeCall::Poll,
            RuntimeCall::Read,
            RuntimeCall::Cleanup,
        ]
    );
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn cleanup_failure_context_is_retained_after_read_failure() {
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit])
        .fail_read("read failed")
        .fail_cleanup("cleanup failed");
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("read failure should be returned with cleanup context");

    assert_eq!(terminal_error_message(&error), "read failed");
    assert_eq!(
        cleanup_error_message(&error).as_deref(),
        Some("cleanup failed")
    );
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn cleanup_failure_is_returned_after_clean_loop() {
    let mut terminal = HarnessTerminal::with_quit().fail_cleanup("cleanup failed");
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    let error = run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect_err("cleanup failure should be returned");

    assert_eq!(terminal_error_message(&error), "cleanup failed");
    assert!(terminal.cleanup_called);
}

#[test]
fn cleanup_order_restores_screen_and_cursor_before_disabling_raw_mode() {
    assert_eq!(
        terminal_cleanup_steps(true, true, false),
        vec![
            TerminalCleanupStep::LeaveAlternateScreen,
            TerminalCleanupStep::ShowCursor,
            TerminalCleanupStep::DisableRawMode,
        ]
    );
}

#[test]
fn cleanup_order_releases_the_mouse_before_leaving_the_alternate_screen() {
    // Mouse capture is enabled on the alternate screen, so releasing it after
    // leaving would write the escape sequence to the user's restored shell.
    assert_eq!(
        terminal_cleanup_steps(true, true, true),
        vec![
            TerminalCleanupStep::DisableMouseCapture,
            TerminalCleanupStep::LeaveAlternateScreen,
            TerminalCleanupStep::ShowCursor,
            TerminalCleanupStep::DisableRawMode,
        ]
    );
}

#[test]
fn cleanup_skips_mouse_release_when_capture_was_never_enabled() {
    // Enabling capture is best-effort, so a terminal that refused it must not
    // be sent a disable sequence it never asked for.
    assert!(
        !terminal_cleanup_steps(true, true, false)
            .contains(&TerminalCleanupStep::DisableMouseCapture)
    );
}

#[test]
fn cleanup_order_handles_partial_setup_after_raw_mode_started() {
    assert_eq!(
        terminal_cleanup_steps(true, false, false),
        vec![
            TerminalCleanupStep::ShowCursor,
            TerminalCleanupStep::DisableRawMode,
        ]
    );
    assert_eq!(terminal_cleanup_steps(false, false, false), vec![]);
}

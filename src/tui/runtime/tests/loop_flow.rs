//! The loop as it behaves when nothing goes wrong: splash handoff, the
//! first fetch, and how refresh deadlines are honored and reset.

use crate::tui::{app::View, ui::HitTarget};

use super::*;

#[tokio::test]
async fn run_splash_draws_every_frame_and_returns_none_when_no_key_arrives() {
    let mut terminal = HarnessTerminal::with_events([]);
    let mut app = app(None);

    let priming = run_splash(&mut terminal, &mut app)
        .await
        .expect("splash should not error without a terminal failure");

    assert_eq!(priming, None);
    assert_eq!(app.splash_frame, None);
    let draw_calls = terminal
        .calls
        .iter()
        .filter(|call| **call == RuntimeCall::Draw)
        .count();
    assert_eq!(draw_calls as u64, theme::SPLASH_TOTAL_FRAMES);
}

#[tokio::test]
async fn run_splash_skips_immediately_and_returns_the_triggering_key() {
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]);
    let mut app = app(None);

    let priming = run_splash(&mut terminal, &mut app)
        .await
        .expect("splash should not error without a terminal failure");

    assert_eq!(priming, Some(RuntimeEvent::Quit));
    assert_eq!(app.splash_frame, None);
    let draw_calls = terminal
        .calls
        .iter()
        .filter(|call| **call == RuntimeCall::Draw)
        .count();
    assert_eq!(draw_calls, 1);
}

#[tokio::test]
async fn splash_priming_event_is_honored_as_first_loop_event() {
    // A key pressed during the splash should both skip it and take
    // effect immediately in the main loop — e.g. `q` both dismisses the
    // splash and quits in the same keystroke, rather than requiring a
    // second press. The priming event must not be silently discarded.
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);
    let refresh_interval = app.refresh_interval;

    run_with_adapters_with_refresh_interval(
        &mut terminal,
        &mut app,
        &mut fetcher,
        refresh_interval,
        true,
    )
    .await
    .expect("runtime should quit cleanly via the priming event");

    assert_eq!(app.splash_frame, None);
    // One splash-frame draw, then run_loop's own pre-poll draw; the
    // queued Quit event is consumed once by the splash and never
    // reaches a second poll/read cycle.
    let draw_calls = terminal
        .calls
        .iter()
        .filter(|call| **call == RuntimeCall::Draw)
        .count();
    assert_eq!(draw_calls, 2);
}

#[tokio::test]
async fn harness_drives_normal_quit_without_fetch() {
    let mut terminal = HarnessTerminal::with_quit();
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

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
    assert!(fetcher.calls.is_empty());
}

#[tokio::test]
async fn harness_records_fetch_success_before_first_draw() {
    let mut terminal = HarnessTerminal::with_quit();
    let mut fetcher = HarnessFetcher::new([Ok(successful_payload())]);
    let url = configured_url();
    let mut app = app(Some(url.clone()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url]);
    assert_eq!(
        app.current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(42.0)
    );
    assert_eq!(app.current_error, None);
    assert_eq!(terminal.drawn_errors, [None]);
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn harness_records_fetch_failure_before_first_draw() {
    let mut terminal = HarnessTerminal::with_quit();
    let mut fetcher = HarnessFetcher::new([Err("request timed out".to_owned())]);
    let mut app = app(Some(configured_url()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(app.current_snapshot, None);
    assert_eq!(app.current_error.as_deref(), Some("request timed out"));
    assert_eq!(
        terminal.drawn_errors,
        [Some("request timed out".to_owned())]
    );
    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn initial_fetch_is_started_without_blocking_quit() {
    let mut terminal = HarnessTerminal::with_quit();
    let mut fetcher = HarnessFetcher::pending_then([None]);
    let url = configured_url();
    let mut app = app(Some(url.clone()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url]);
    assert_eq!(app.current_snapshot, None);
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
}

#[tokio::test]
async fn interval_refresh_starts_after_refresh_deadline() {
    let clock = test_clock_start();
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]).with_clock(clock.clone());
    for _ in 0..50 {
        terminal.polls.push_back(Ok(false));
        terminal.poll_advances.push_back(Duration::from_millis(100));
    }
    let mut fetcher =
        HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
    let url = configured_url();
    let mut app = app(Some(url.clone()));
    app.refresh_interval = Duration::from_secs(5);

    run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url.clone(), url]);
    assert_eq!(
        app.current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(55.0)
    );
    assert_eq!(
        app.previous_successful_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(42.0)
    );
}

#[tokio::test]
async fn runtime_refresh_schedule_can_shorten_without_mutating_app_interval() {
    let clock = test_clock_start();
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]).with_clock(clock.clone());
    for _ in 0..3 {
        terminal.polls.push_back(Ok(false));
        terminal.poll_advances.push_back(Duration::from_millis(100));
    }
    let mut fetcher =
        HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
    let url = configured_url();
    let production_interval = Duration::from_secs(MIN_REFRESH_INTERVAL_SECS);
    let mut app = TuiApp::new(Some(url.clone()), production_interval);
    let refresh_schedule_interval =
        runtime_refresh_schedule_interval(production_interval, Some("250"));

    terminal.enter().expect("terminal should enter");
    let mut fake_clock = FakeClock::new(clock);
    let result = run_loop(
        &mut terminal,
        &mut app,
        &mut fetcher,
        &mut fake_clock,
        refresh_schedule_interval,
        None,
    )
    .await;
    let cleanup_result = terminal.cleanup();

    result.expect("runtime should quit cleanly");
    cleanup_result.expect("cleanup should succeed");
    assert_eq!(refresh_schedule_interval, Duration::from_millis(250));
    assert_eq!(app.refresh_interval, production_interval);
    assert_eq!(fetcher.calls, [url.clone(), url]);
    assert_eq!(
        app.current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(55.0)
    );
}

#[tokio::test]
async fn early_false_polls_do_not_fire_interval_refresh_early() {
    let clock = test_clock_start();
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit]).with_clock(clock.clone());
    for _ in 0..50 {
        terminal.polls.push_back(Ok(false));
        terminal.poll_advances.push_back(Duration::ZERO);
    }
    let url = configured_url();
    let mut fetcher = HarnessFetcher::new([Ok(successful_payload())]);
    let mut app = app(Some(url.clone()));
    app.refresh_interval = Duration::from_secs(5);

    run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url]);
    assert!(
        terminal
            .poll_timeouts
            .iter()
            .all(|timeout| *timeout <= FETCH_RESULT_POLL_INTERVAL)
    );
}

#[tokio::test]
async fn delayed_false_poll_handles_at_most_one_interval_deadline() {
    let clock = test_clock_start();
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Quit])
        .with_clock(clock.clone())
        .poll_advance(Duration::from_secs(16));
    terminal.polls.push_back(Ok(false));
    let url = configured_url();
    let mut fetcher =
        HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
    let mut app = app(Some(url.clone()));
    app.refresh_interval = Duration::from_secs(5);

    run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url.clone(), url]);
    assert_eq!(
        app.current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(55.0)
    );
}

#[tokio::test]
async fn manual_refresh_resets_interval_from_event_read_time() {
    let clock = test_clock_start();
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Refresh, RuntimeEvent::Quit])
        .with_clock(clock.clone())
        .poll_advance(Duration::from_secs(2))
        .read_advance(Duration::from_secs(3))
        .poll_advance(Duration::from_secs(4));
    terminal.polls.push_back(Ok(true));
    terminal.polls.push_back(Ok(false));
    terminal.polls.push_back(Ok(true));
    let url = configured_url();
    let mut fetcher =
        HarnessFetcher::new([Ok(successful_payload()), Ok(later_successful_payload())]);
    let mut app = app(Some(url.clone()));
    app.refresh_interval = Duration::from_secs(5);

    run_with_harness_clock(&mut terminal, &mut app, &mut fetcher, clock)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url.clone(), url]);
    assert_eq!(
        app.current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(55.0)
    );
}

#[tokio::test]
async fn manual_refresh_during_in_flight_fetch_is_coalesced() {
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Refresh, RuntimeEvent::Quit]);
    let mut fetcher = HarnessFetcher::pending_then([
        None,
        None,
        Some(Ok(successful_payload())),
        Some(Ok(later_successful_payload())),
    ]);
    let url = configured_url();
    let mut app = app(Some(url.clone()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url.clone(), url]);
    assert_eq!(
        app.current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(55.0)
    );
    assert_eq!(
        app.previous_successful_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(42.0)
    );
}

#[tokio::test]
async fn refresh_interval_key_events_adjust_app_interval() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::IncreaseRefreshInterval,
        RuntimeEvent::IncreaseRefreshInterval,
        RuntimeEvent::DecreaseRefreshInterval,
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(app.refresh_interval, Duration::from_secs(35));
    assert!(fetcher.calls.is_empty());
    assert_eq!(
        terminal
            .calls
            .iter()
            .filter(|call| **call == RuntimeCall::Draw)
            .count(),
        4
    );
}

#[tokio::test]
async fn failure_after_success_preserves_last_successful_snapshot() {
    let mut terminal = HarnessTerminal::with_events([RuntimeEvent::Refresh, RuntimeEvent::Quit]);
    let mut fetcher =
        HarnessFetcher::new([Ok(successful_payload()), Err("refresh failed".to_owned())]);
    let url = configured_url();
    let mut app = app(Some(url.clone()));

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("runtime should quit cleanly");

    assert_eq!(fetcher.calls, [url.clone(), url]);
    assert_eq!(
        app.current_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.aqi),
        Some(42.0)
    );
    assert_eq!(app.current_error.as_deref(), Some("refresh failed"));
}

// -- Vim sequences and mouse routing ------------------------------------

#[tokio::test]
async fn gg_requires_two_presses_to_jump_to_the_top() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleThemeSettings,
        RuntimeEvent::NavDown,
        RuntimeEvent::NavDown,
        // A lone `g` arms the prefix and must not move the cursor.
        RuntimeEvent::GPrefix,
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.settings_cursor, 2, "a single g must not jump");
}

#[tokio::test]
async fn a_second_g_completes_the_jump_to_the_top() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleThemeSettings,
        RuntimeEvent::NavDown,
        RuntimeEvent::NavDown,
        RuntimeEvent::GPrefix,
        RuntimeEvent::GPrefix,
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.settings_cursor, 0);
}

#[tokio::test]
async fn an_interrupted_g_prefix_does_not_arm_a_later_g() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleThemeSettings,
        RuntimeEvent::NavDown,
        RuntimeEvent::NavDown,
        RuntimeEvent::GPrefix,
        // Any other key breaks the pending prefix, exactly as vim does.
        RuntimeEvent::NavDown,
        RuntimeEvent::GPrefix,
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(
        app.settings_cursor, 3,
        "the trailing g should still be armed"
    );
}

#[tokio::test]
async fn nav_last_jumps_to_the_end_of_the_theme_list() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleThemeSettings,
        RuntimeEvent::NavLast,
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.settings_cursor, crate::tui::theme::ALL.len() - 1);
}

#[tokio::test]
async fn leader_space_then_t_opens_the_theme_picker() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleLeader,
        RuntimeEvent::LeaderKey('t'),
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.view, View::ThemeSettings);
    assert!(!app.leader_pending, "the popup closes once resolved");
}

#[tokio::test]
async fn leader_q_quits_the_loop() {
    let mut terminal =
        HarnessTerminal::with_events([RuntimeEvent::ToggleLeader, RuntimeEvent::LeaderKey('q')]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("leader quit should end the loop cleanly");

    assert!(terminal.cleanup_called);
}

#[tokio::test]
async fn an_unbound_leader_key_dismisses_without_acting() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleLeader,
        RuntimeEvent::LeaderKey('z'),
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.view, View::Dashboard);
    assert!(!app.leader_pending);
}

#[tokio::test]
async fn a_click_selects_the_row_the_hit_map_reports() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleThemeSettings,
        RuntimeEvent::MouseClick(4, 9),
        RuntimeEvent::Quit,
    ])
    .with_hit(HitTarget::ThemeRow(5));
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.settings_cursor, 5);
    assert_eq!(app.theme, crate::tui::theme::ALL[5]);
}

#[tokio::test]
async fn a_click_on_empty_space_changes_nothing() {
    // No `with_hit`, so the hit map reports nothing at those coordinates.
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleThemeSettings,
        RuntimeEvent::MouseClick(0, 0),
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.settings_cursor, 0);
    assert_eq!(app.view, View::ThemeSettings);
}

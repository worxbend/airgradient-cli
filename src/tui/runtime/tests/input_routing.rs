//! How input reaches the app model: vim-style key sequences, the leader
//! popup, and mouse clicks resolved through the hit map.
//!
//! The timing side of the loop — deadlines, fetches, splash — is
//! [`super::loop_flow`].

use crate::tui::{app::View, ui::HitTarget};

use super::*;

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

    assert_eq!(app.theme_cursor(), 2, "a single g must not jump");
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

    assert_eq!(app.theme_cursor(), 0);
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
        app.theme_cursor(),
        3,
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

    assert_eq!(app.theme_cursor(), crate::tui::theme::ALL.len() - 1);
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

    assert_eq!(app.theme_cursor(), 5);
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

    assert_eq!(app.theme_cursor(), 0);
    assert_eq!(app.view, View::ThemeSettings);
}

#[tokio::test]
async fn the_chart_opens_toggles_and_takes_the_modal_motions() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleHistory,
        // The same motions every other modal view binds, moving between
        // metrics rather than list rows.
        RuntimeEvent::NavDown,
        RuntimeEvent::NavDown,
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.view, View::History);
    assert_eq!(app.history_metric().key, "pm25");
}

#[tokio::test]
async fn escape_closes_the_chart_back_to_the_dashboard() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleHistory,
        RuntimeEvent::Escape,
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.view, View::Dashboard);
}

#[tokio::test]
async fn the_leader_sequence_opens_the_chart() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleLeader,
        RuntimeEvent::LeaderKey('h'),
        RuntimeEvent::Quit,
    ]);
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.view, View::History);
    assert!(!app.leader_pending, "the sequence should have disarmed");
}

#[tokio::test]
async fn clicking_a_chart_metric_selects_it() {
    let mut terminal = HarnessTerminal::with_events([
        RuntimeEvent::ToggleHistory,
        RuntimeEvent::MouseClick(4, 6),
        RuntimeEvent::Quit,
    ])
    .with_hit(HitTarget::HistoryRow(3));
    let mut fetcher = HarnessFetcher::new([]);
    let mut app = app(None);

    run_with_adapters(&mut terminal, &mut app, &mut fetcher)
        .await
        .expect("loop should end cleanly");

    assert_eq!(app.history_metric().key, "pm1");
}

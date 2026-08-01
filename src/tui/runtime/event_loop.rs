//! The draw/poll/refresh loop, and the events that drive it.
//!
//! The loop never blocks on either input or the network: it polls the
//! terminal with a short timeout, applies any finished fetch, and redraws
//! only when something actually changed.

use std::time::{Duration, Instant};

use crate::tui::{
    app::{PaletteOutcome, TuiApp, View},
    runtime::{
        RuntimeError,
        fetch::{FetchScheduler, MeasureFetchWorker},
        terminal::TerminalRuntime,
    },
    theme,
};

/// Upper bound on how long the loop waits for input before checking whether a
/// background fetch finished. Also the granularity at which a fetch result can
/// appear on screen.
pub(super) const FETCH_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

const SPLASH_FRAME_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeEvent {
    Quit,
    Refresh,
    IncreaseRefreshInterval,
    DecreaseRefreshInterval,
    /// Opens the `:` command palette.
    OpenPalette,
    /// Opens the theme picker if closed, closes it if open (`t`/`T`/`F2`).
    ToggleThemeSettings,
    /// Opens the config editor if closed, closes it if open (`c`/`C`).
    ToggleConfigEditor,
    /// Closes whichever modal view/field-edit is active; quitting on a bare
    /// `Esc` at the dashboard is still handled by the `Quit` variant above.
    Escape,
    NavUp,
    NavDown,
    /// Commits a field edit, applies a theme selection, or submits the
    /// palette line, depending on which view is open.
    Confirm,
    /// A printable character typed into the palette or an editing field.
    PaletteChar(char),
    PaletteBackspace,
    Ignored,
}

/// What a raw keypress means depends on which view is open and whether a
/// config-editor field is mid-edit — `read_event` needs this to decide, for
/// example, whether `Char('t')` types the letter "t" into a URL field or
/// toggles the theme picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    /// Dashboard: single-key shortcuts (`q`, `r`, `+`/`-`, `:`, `t`, `c`).
    Normal,
    /// A modal view (theme picker / config editor) is open but no text
    /// field is being edited: arrows navigate, Enter acts, Esc closes.
    ModalNav,
    /// A text field (palette input, or a config-editor field mid-edit) is
    /// capturing every printable character.
    TextEntry,
}

fn input_mode(app: &TuiApp) -> InputMode {
    match app.view {
        View::Dashboard => InputMode::Normal,
        View::CommandPalette => InputMode::TextEntry,
        View::ConfigEditor if app.config_editor_editing.is_some() => InputMode::TextEntry,
        View::ConfigEditor | View::ThemeSettings => InputMode::ModalNav,
    }
}

/// The loop's read of wall-clock time, injected so tests can drive refresh
/// deadlines deterministically instead of sleeping.
pub(super) trait RuntimeClock {
    fn now(&mut self) -> Instant;
}

pub(super) struct SystemClock;

impl RuntimeClock for SystemClock {
    fn now(&mut self) -> Instant {
        Instant::now()
    }
}

/// When the next automatic refresh is due, and the interval it repeats on.
/// Held together because every event that changes one changes the other.
struct RefreshSchedule {
    interval: Duration,
    next_at: Instant,
}

impl RefreshSchedule {
    /// Restarts the countdown from `now`. Called after every refresh so the
    /// interval is measured between refreshes rather than drifting off a
    /// fixed origin.
    fn restart_from(&mut self, now: Instant) {
        self.next_at = now + self.interval;
    }
}

/// What the loop should do after applying an event to the app model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventOutcome {
    /// App state changed — redraw and keep going.
    Redraw,
    /// The key was not bound in this view; nothing to repaint.
    Unchanged,
    Quit,
}

/// Shows the startup splash for a fixed number of frames using the app's
/// already-resolved theme. Any keypress skips it immediately; that key is
/// returned rather than discarded so the caller can feed it into the main
/// loop as a priming event — pressing `q`/`Esc` during the splash both
/// dismisses it and quits in the same keystroke, which is what keeps the
/// single-keypress PTY shutdown tests passing unmodified.
pub(super) async fn run_splash<T>(
    terminal: &mut T,
    app: &mut TuiApp,
) -> Result<Option<RuntimeEvent>, RuntimeError>
where
    T: TerminalRuntime,
{
    let mut priming_event = None;

    for frame in 0..theme::SPLASH_TOTAL_FRAMES {
        app.splash_frame = Some(frame);
        terminal.draw(app)?;

        if terminal.poll_event(SPLASH_FRAME_INTERVAL).await? {
            priming_event = Some(terminal.read_event(InputMode::Normal).await?);
            break;
        }
    }

    app.splash_frame = None;
    Ok(priming_event)
}

/// Drives the TUI until the user quits or a terminal/fetch call fails.
///
/// However the loop ends, any in-flight fetch is cancelled on the way out; if
/// that cancellation also fails, both errors are reported together rather
/// than the later one replacing the one that actually ended the loop.
pub(super) async fn run_loop<T, F, C>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
    clock: &mut C,
    initial_refresh_schedule_interval: Duration,
    priming_event: Option<RuntimeEvent>,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
    C: RuntimeClock,
{
    let mut fetch_scheduler = FetchScheduler::default();
    let result = drive_loop(
        terminal,
        app,
        fetcher,
        clock,
        &mut fetch_scheduler,
        initial_refresh_schedule_interval,
        priming_event,
    )
    .await;
    let cancel_result = fetch_scheduler.cancel_pending_fetch(app, fetcher).await;

    match (result, cancel_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Err(primary), Err(secondary)) => {
            Err(RuntimeError::with_secondary_failure(primary, secondary))
        }
        (Ok(()), Err(error)) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
async fn drive_loop<T, F, C>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
    clock: &mut C,
    fetch_scheduler: &mut FetchScheduler,
    initial_refresh_schedule_interval: Duration,
    priming_event: Option<RuntimeEvent>,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
    C: RuntimeClock,
{
    let mut pending_priming = priming_event;

    start_refresh_if_configured(fetch_scheduler, app, fetcher).await?;
    terminal.draw(app)?;

    let mut schedule = RefreshSchedule {
        interval: initial_refresh_schedule_interval,
        next_at: clock.now() + initial_refresh_schedule_interval,
    };

    loop {
        if fetch_scheduler.apply_ready_results(app, fetcher).await? {
            terminal.draw(app)?;
        }

        let time_until_refresh = schedule.next_at.saturating_duration_since(clock.now());
        let poll_timeout = time_until_refresh.min(FETCH_RESULT_POLL_INTERVAL);

        // A splash-priming event (see `run_splash`) is consumed exactly
        // once, ahead of any real polling, so a keypress that skipped
        // the splash still takes effect on this very first iteration.
        let ready_event = if let Some(event) = pending_priming.take() {
            Some(event)
        } else if terminal.poll_event(poll_timeout).await? {
            Some(terminal.read_event(input_mode(app)).await?)
        } else {
            None
        };

        if let Some(event) = ready_event {
            let quit = handle_event(
                event,
                terminal,
                app,
                fetcher,
                clock,
                fetch_scheduler,
                &mut schedule,
            )
            .await?;
            if quit {
                break;
            }
        }

        refresh_if_due(
            terminal,
            app,
            fetcher,
            clock,
            fetch_scheduler,
            &mut schedule,
        )
        .await?;
    }

    Ok(())
}

/// Applies one event, returning whether the loop should quit.
///
/// The three timing-sensitive events restart the refresh countdown from the
/// moment the key was handled, so a manual refresh or an interval change does
/// not leave a stale deadline that fires again immediately.
#[allow(clippy::too_many_arguments)]
async fn handle_event<T, F, C>(
    event: RuntimeEvent,
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
    clock: &mut C,
    fetch_scheduler: &mut FetchScheduler,
    schedule: &mut RefreshSchedule,
) -> Result<bool, RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
    C: RuntimeClock,
{
    match event {
        RuntimeEvent::Refresh => {
            let now = clock.now();
            start_refresh_if_configured(fetch_scheduler, app, fetcher).await?;
            schedule.restart_from(now);
            terminal.draw(app)?;
        }
        RuntimeEvent::IncreaseRefreshInterval | RuntimeEvent::DecreaseRefreshInterval => {
            let now = clock.now();
            if event == RuntimeEvent::IncreaseRefreshInterval {
                app.increase_refresh_interval();
            } else {
                app.decrease_refresh_interval();
            }
            // A manual interval change also drops any test-only scheduler
            // override: from here on the app's own interval is the schedule.
            schedule.interval = app.refresh_interval;
            schedule.restart_from(now);
            terminal.draw(app)?;
        }
        view_event => match apply_view_event(view_event, app) {
            EventOutcome::Quit => return Ok(true),
            EventOutcome::Redraw => terminal.draw(app)?,
            EventOutcome::Unchanged => {}
        },
    }

    Ok(false)
}

/// Applies the events that only move view state around — no clock, no
/// network. What each one does depends on which view is open.
fn apply_view_event(event: RuntimeEvent, app: &mut TuiApp) -> EventOutcome {
    match event {
        RuntimeEvent::Quit => return EventOutcome::Quit,
        RuntimeEvent::OpenPalette => app.open_command_palette(),
        RuntimeEvent::ToggleThemeSettings => {
            if app.view == View::ThemeSettings {
                app.close_theme_settings();
            } else {
                app.open_theme_settings();
            }
        }
        RuntimeEvent::ToggleConfigEditor => {
            if app.view == View::ConfigEditor {
                app.close_config_editor();
            } else {
                app.open_config_editor();
            }
        }
        RuntimeEvent::Escape => match app.view {
            View::ThemeSettings => app.close_theme_settings(),
            View::ConfigEditor if app.config_editor_editing.is_some() => {
                app.config_editor_cancel_edit();
            }
            View::ConfigEditor => app.close_config_editor(),
            View::CommandPalette => app.close_command_palette(),
            View::Dashboard => {}
        },
        RuntimeEvent::NavUp => match app.view {
            View::ThemeSettings => app.theme_cursor_up(),
            View::ConfigEditor => app.config_editor_nav_up(),
            View::CommandPalette | View::Dashboard => {}
        },
        RuntimeEvent::NavDown => match app.view {
            View::ThemeSettings => app.theme_cursor_down(),
            View::ConfigEditor => app.config_editor_nav_down(),
            View::CommandPalette | View::Dashboard => {}
        },
        RuntimeEvent::Confirm => match app.view {
            View::ThemeSettings => app.confirm_theme_settings(),
            View::ConfigEditor => app.config_editor_confirm(),
            View::CommandPalette => {
                if app.palette_submit() == PaletteOutcome::Quit {
                    return EventOutcome::Quit;
                }
            }
            View::Dashboard => {}
        },
        RuntimeEvent::PaletteChar(c) => match app.view {
            View::CommandPalette => app.palette_push_char(c),
            View::ConfigEditor => app.config_editor_push_char(c),
            View::ThemeSettings | View::Dashboard => {}
        },
        RuntimeEvent::PaletteBackspace => match app.view {
            View::CommandPalette => app.palette_backspace(),
            View::ConfigEditor => app.config_editor_backspace(),
            View::ThemeSettings | View::Dashboard => {}
        },
        RuntimeEvent::Ignored => return EventOutcome::Unchanged,
        // Handled by `handle_event`, which needs the clock and scheduler.
        RuntimeEvent::Refresh
        | RuntimeEvent::IncreaseRefreshInterval
        | RuntimeEvent::DecreaseRefreshInterval => return EventOutcome::Unchanged,
    }

    EventOutcome::Redraw
}

/// Fires the periodic refresh once its deadline has passed, and restarts the
/// countdown. A no-op before the deadline, so it is safe to call every
/// iteration.
async fn refresh_if_due<T, F, C>(
    terminal: &mut T,
    app: &mut TuiApp,
    fetcher: &mut F,
    clock: &mut C,
    fetch_scheduler: &mut FetchScheduler,
    schedule: &mut RefreshSchedule,
) -> Result<(), RuntimeError>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
    C: RuntimeClock,
{
    let now = clock.now();
    if now < schedule.next_at {
        return Ok(());
    }

    start_refresh_if_configured(fetch_scheduler, app, fetcher).await?;
    schedule.restart_from(now);
    terminal.draw(app)
}

/// Requests a fetch and drains anything that finished immediately. Without a
/// configured device URL there is nothing to fetch, and the refresh deadline
/// is still restarted by the caller so the loop keeps its cadence.
async fn start_refresh_if_configured<F>(
    fetch_scheduler: &mut FetchScheduler,
    app: &mut TuiApp,
    fetcher: &mut F,
) -> Result<(), RuntimeError>
where
    F: MeasureFetchWorker,
{
    if app.configured_url.is_none() {
        return Ok(());
    }

    fetch_scheduler.request_refresh(app, fetcher);
    fetch_scheduler.apply_ready_results(app, fetcher).await?;
    Ok(())
}

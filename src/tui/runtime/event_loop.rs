//! The draw/poll/refresh loop, and the events that drive it.
//!
//! The loop never blocks on either input or the network: it polls the
//! terminal with a short timeout, applies any finished fetch, and redraws
//! only when something actually changed.

use std::time::{Duration, SystemTime};

use crate::tui::{
    app::{LeaderAction, TuiApp},
    runtime::{
        RuntimeError,
        event::{InputMode, RuntimeEvent, apply_hit, input_mode},
        fetch::{FetchScheduler, MeasureFetchWorker},
        schedule::{RefreshSchedule, RuntimeClock},
        terminal::TerminalRuntime,
        view_events::{EventOutcome, apply_view_event},
    },
    theme,
};

/// Upper bound on how long the loop waits for input before checking whether a
/// background fetch finished. Also the granularity at which a fetch result can
/// appear on screen.
pub(super) const FETCH_RESULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

const SPLASH_FRAME_INTERVAL: Duration = Duration::from_millis(50);

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
    let mut loop_context = LoopContext::new(
        terminal,
        app,
        fetcher,
        clock,
        initial_refresh_schedule_interval,
    );
    let result = loop_context.drive(priming_event).await;
    let cancel_result = loop_context.cancel_pending_fetch().await;

    match (result, cancel_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Err(primary), Err(secondary)) => {
            Err(RuntimeError::with_secondary_failure(primary, secondary))
        }
        (Ok(()), Err(error)) => Err(error),
    }
}

/// Everything one run of the loop needs to do its job.
///
/// These six travel together through every step, so they are one thing rather
/// than six arguments repeated on every signature. Bundling them is also what
/// lets the loop's steps be methods, which is why none of them needs
/// `#[allow(clippy::too_many_arguments)]` any more.
struct LoopContext<'a, T, F, C> {
    terminal: &'a mut T,
    app: &'a mut TuiApp,
    fetcher: &'a mut F,
    clock: &'a mut C,
    fetch_scheduler: FetchScheduler,
    schedule: RefreshSchedule,
}

impl<'a, T, F, C> LoopContext<'a, T, F, C>
where
    T: TerminalRuntime,
    F: MeasureFetchWorker,
    C: RuntimeClock,
{
    fn new(
        terminal: &'a mut T,
        app: &'a mut TuiApp,
        fetcher: &'a mut F,
        clock: &'a mut C,
        initial_refresh_schedule_interval: Duration,
    ) -> Self {
        let next_at = clock.now() + initial_refresh_schedule_interval;

        Self {
            terminal,
            app,
            fetcher,
            clock,
            fetch_scheduler: FetchScheduler::default(),
            schedule: RefreshSchedule {
                interval: initial_refresh_schedule_interval,
                next_at,
            },
        }
    }

    async fn drive(&mut self, priming_event: Option<RuntimeEvent>) -> Result<(), RuntimeError> {
        let mut pending_priming = priming_event;

        self.start_refresh_if_configured().await?;
        self.draw()?;

        // The interval the schedule was last told about. Compared against,
        // rather than assigned from, `app.refresh_interval` every iteration,
        // because the two are allowed to differ at startup: tests hand the loop
        // a short schedule interval while the app still displays the configured
        // one.
        let mut scheduled_app_interval = self.app.refresh_interval;

        loop {
            if self
                .fetch_scheduler
                .apply_ready_results(self.app, self.fetcher)
                .await?
            {
                self.draw()?;
            }

            // An alert is the one thing on screen that goes away on its own, so
            // it needs the loop to notice rather than waiting for the next
            // keypress or fetch to trigger a repaint.
            if self.app.expire_due_alerts(SystemTime::now()) {
                self.draw()?;
            }

            let time_until_refresh = self
                .schedule
                .next_at
                .saturating_duration_since(self.clock.now());
            let poll_timeout = time_until_refresh.min(FETCH_RESULT_POLL_INTERVAL);

            // A splash-priming event (see `run_splash`) is consumed exactly
            // once, ahead of any real polling, so a keypress that skipped
            // the splash still takes effect on this very first iteration.
            let ready_event = if let Some(event) = pending_priming.take() {
                Some(event)
            } else if self.terminal.poll_event(poll_timeout).await? {
                Some(self.terminal.read_event(input_mode(self.app)).await?)
            } else {
                // The poll timed out with nothing to report, which is the only
                // moment the loop has spare: advance any running animation
                // then, so a spinner turns without a keypress and an idle
                // dashboard is still never repainted.
                if self.app.is_animating() {
                    self.app.advance_animation();
                    self.draw()?;
                }
                None
            };

            if let Some(event) = ready_event {
                if self.handle_event(event).await? {
                    break;
                }

                // Any event may have changed the app's interval — the `+`/`-`
                // keys, `:refresh N` in the command palette, or Save & Close in
                // the config editor. Each of those used to only update what the
                // status line displays, leaving the loop polling on the old
                // cadence, so the interval is adopted here once for all of them.
                if self.app.refresh_interval != scheduled_app_interval {
                    scheduled_app_interval = self.app.refresh_interval;
                    self.schedule.interval = scheduled_app_interval.as_duration();
                    let now = self.clock.now();
                    self.schedule.restart_from(now);
                }
            }

            self.refresh_if_due().await?;
        }

        Ok(())
    }

    fn draw(&mut self) -> Result<(), RuntimeError> {
        self.terminal.draw(self.app)
    }

    /// Applies one event, returning whether the loop should quit.
    ///
    /// A leader sequence resolves to one of the events below before anything
    /// else happens, so the rest of this function never sees a `LeaderKey` and
    /// the two do not have to call each other.
    async fn handle_event(&mut self, event: RuntimeEvent) -> Result<bool, RuntimeError> {
        let event = match event {
            RuntimeEvent::LeaderKey(key) => match self.app.resolve_leader(key) {
                LeaderAction::Quit => return Ok(true),
                // An unbound key just closes the popup, which already happened
                // when the sequence resolved — so only the repaint is left.
                LeaderAction::Dismiss => {
                    self.draw()?;
                    return Ok(false);
                }
                action => leader_event(action),
            },
            other => other,
        };

        // The three timing-sensitive events restart the refresh countdown from
        // the moment the key was handled, so a manual refresh or an interval
        // change does not leave a stale deadline that fires again immediately.
        match event {
            RuntimeEvent::Refresh => {
                let now = self.clock.now();
                self.start_refresh_if_configured().await?;
                self.schedule.restart_from(now);
                self.draw()?;
            }
            RuntimeEvent::IncreaseRefreshInterval | RuntimeEvent::DecreaseRefreshInterval => {
                let now = self.clock.now();
                if event == RuntimeEvent::IncreaseRefreshInterval {
                    self.app.increase_refresh_interval();
                } else {
                    self.app.decrease_refresh_interval();
                }
                // The new interval is adopted by `drive`, which watches
                // `app.refresh_interval` after every event. Restarting here
                // still matters for the case where the interval did not move
                // because it was already at its minimum or maximum.
                self.schedule.restart_from(now);
                self.draw()?;
            }
            RuntimeEvent::MouseClick(column, row) => {
                match self.terminal.hit_test(column, row) {
                    Some(target) if apply_hit(self.app, target) => self.draw()?,
                    // A click on empty space is not an error, just nothing.
                    _ => {}
                }
            }
            RuntimeEvent::LeaderKey(_) => unreachable!("resolved above"),
            view_event => match apply_view_event(view_event, self.app) {
                EventOutcome::Quit => return Ok(true),
                EventOutcome::Redraw => self.draw()?,
                EventOutcome::Unchanged => {}
            },
        }

        Ok(false)
    }

    /// Fires the periodic refresh once its deadline has passed, and restarts
    /// the countdown. A no-op before the deadline, so it is safe to call every
    /// iteration.
    async fn refresh_if_due(&mut self) -> Result<(), RuntimeError> {
        let now = self.clock.now();
        if now < self.schedule.next_at {
            return Ok(());
        }

        self.start_refresh_if_configured().await?;
        self.schedule.restart_from(now);
        self.draw()
    }

    /// Requests a fetch and drains anything that finished immediately. Without
    /// a configured device URL there is nothing to fetch, and the refresh
    /// deadline is still restarted by the caller so the loop keeps its cadence.
    async fn start_refresh_if_configured(&mut self) -> Result<(), RuntimeError> {
        if self.app.configured_url.is_none() {
            return Ok(());
        }

        self.fetch_scheduler.request_refresh(self.app, self.fetcher);
        self.fetch_scheduler
            .apply_ready_results(self.app, self.fetcher)
            .await?;
        Ok(())
    }

    async fn cancel_pending_fetch(&mut self) -> Result<(), RuntimeError> {
        self.fetch_scheduler
            .cancel_pending_fetch(self.app, self.fetcher)
            .await
    }
}

/// The event a resolved leader sequence stands for.
fn leader_event(action: LeaderAction) -> RuntimeEvent {
    match action {
        LeaderAction::Refresh => RuntimeEvent::Refresh,
        LeaderAction::OpenThemes => RuntimeEvent::ToggleThemeSettings,
        LeaderAction::OpenConfig => RuntimeEvent::ToggleConfigEditor,
        LeaderAction::OpenHistory => RuntimeEvent::ToggleHistory,
        LeaderAction::OpenPalette => RuntimeEvent::OpenPalette,
        LeaderAction::LongerInterval => RuntimeEvent::IncreaseRefreshInterval,
        LeaderAction::ShorterInterval => RuntimeEvent::DecreaseRefreshInterval,
        // Both handled before this is reached — they need the loop, not an
        // event.
        LeaderAction::Quit | LeaderAction::Dismiss => RuntimeEvent::Ignored,
    }
}

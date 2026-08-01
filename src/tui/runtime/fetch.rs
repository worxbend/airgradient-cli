//! Background measurement fetching and the policy that schedules it.
//!
//! [`FetchScheduler`] holds the policy (at most one request in flight, with a
//! single coalesced follow-up) and [`MeasureFetchWorker`] is the transport
//! seam the event loop is tested against.

use std::time::{Duration, Instant, SystemTime};

use reqwest::Client;
use serde_json::Value;
use tokio::task::JoinHandle;
use url::Url;

use crate::{device, sensors, tui::app::TuiApp};

use super::RuntimeError;

/// Tracks the one in-flight fetch and whether another was asked for while it
/// was running.
///
/// Only one request is ever in flight: a device on a slow LAN can take longer
/// to answer than the refresh interval, and overlapping requests would let an
/// older response land after a newer one and show a stale reading.
#[derive(Debug, Default)]
pub(super) struct FetchScheduler {
    in_flight: bool,
    follow_up_requested: bool,
}

impl FetchScheduler {
    /// Starts a fetch, or — if one is already running — records that exactly
    /// one more should follow. Repeated requests while busy collapse into that
    /// single follow-up rather than queueing up.
    pub(super) fn request_refresh<F>(&mut self, app: &mut TuiApp, fetcher: &mut F)
    where
        F: MeasureFetchWorker,
    {
        let Some(base_url) = app.configured_url.clone() else {
            return;
        };

        if self.in_flight {
            self.follow_up_requested = true;
            return;
        }

        fetcher.start_fetch(base_url);
        self.in_flight = true;
        app.begin_fetch();
    }

    /// Drains any finished fetch into the app model, returning whether
    /// anything was applied (i.e. whether the caller needs to redraw).
    ///
    /// With nothing in flight, completions are drained and discarded: they can
    /// only be results of a cancelled fetch, and applying them would resurrect
    /// state the user already dismissed.
    pub(super) async fn apply_ready_results<F>(
        &mut self,
        app: &mut TuiApp,
        fetcher: &mut F,
    ) -> Result<bool, RuntimeError>
    where
        F: MeasureFetchWorker,
    {
        if !self.in_flight {
            while fetcher.recv_ready_fetch().await?.is_some() {}
            return Ok(false);
        }

        let mut applied_result = false;

        while let Some(completion) = fetcher.recv_ready_fetch().await? {
            self.in_flight = false;
            applied_result = true;

            match completion.result {
                Ok(payload) => {
                    let snapshot = sensors::parse_snapshot(&payload);
                    app.finish_fetch_success(
                        snapshot,
                        completion.duration,
                        completion.completed_at,
                    );
                }
                Err(error) => {
                    app.finish_fetch_failure(error, completion.duration);
                }
            }

            if self.follow_up_requested {
                self.follow_up_requested = false;
                self.request_refresh(app, fetcher);
            }
        }

        Ok(applied_result)
    }

    /// Aborts any in-flight fetch on shutdown. The scheduler and the app's
    /// fetching flag are cleared even when the abort itself fails, so a failed
    /// cancellation cannot leave the UI reporting a fetch that will never land.
    pub(super) async fn cancel_pending_fetch<F>(
        &mut self,
        app: &mut TuiApp,
        fetcher: &mut F,
    ) -> Result<(), RuntimeError>
    where
        F: MeasureFetchWorker,
    {
        let cancel_result = if self.in_flight {
            fetcher.cancel_fetch().await
        } else {
            Ok(())
        };

        self.in_flight = false;
        self.follow_up_requested = false;
        app.is_fetching = false;
        cancel_result
    }
}

#[derive(Debug)]
pub(super) struct FetchCompletion {
    pub(super) result: Result<Value, String>,
    pub(super) duration: Duration,
    pub(super) completed_at: SystemTime,
}

/// The seam between the event loop and the network.
///
/// `recv_ready_fetch` is non-blocking by contract — it returns `None` rather
/// than awaiting a result, because the loop also has to keep polling the
/// terminal for input while a fetch is outstanding.
pub(super) trait MeasureFetchWorker {
    fn start_fetch(&mut self, base_url: Url);
    async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError>;
    async fn cancel_fetch(&mut self) -> Result<(), RuntimeError>;
}

pub(super) struct BackgroundMeasureFetcher {
    client: Client,
    active_handle: Option<JoinHandle<FetchCompletion>>,
}

impl BackgroundMeasureFetcher {
    pub(super) fn new(client: Client) -> Self {
        Self {
            client,
            active_handle: None,
        }
    }
}

impl MeasureFetchWorker for BackgroundMeasureFetcher {
    fn start_fetch(&mut self, base_url: Url) {
        if let Some(handle) = self.active_handle.take() {
            handle.abort();
        }

        let client = self.client.clone();

        self.active_handle = Some(tokio::spawn(async move {
            let started = Instant::now();
            let result = device::fetch_current_measures_with_client(&client, &base_url)
                .await
                .map_err(|error| error.to_string());
            FetchCompletion {
                result,
                duration: started.elapsed(),
                completed_at: SystemTime::now(),
            }
        }));
    }

    async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
        let Some(handle) = self.active_handle.as_ref() else {
            return Ok(None);
        };

        // Checked before awaiting so an unfinished fetch never blocks the loop.
        if !handle.is_finished() {
            return Ok(None);
        }

        let handle = self
            .active_handle
            .take()
            .expect("active handle should exist after readiness check");
        observe_fetch_handle(handle).await.map(Some)
    }

    async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
        if let Some(handle) = self.active_handle.take() {
            handle.abort();
            match handle.await {
                Ok(_) => {}
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(RuntimeError::FetchTask(error.to_string())),
            }
        }

        Ok(())
    }
}

/// Turns a join failure into a runtime error, distinguishing a cancelled task
/// from a panicked one so a panic inside the fetch is not silently reported as
/// an ordinary cancellation.
pub(super) async fn observe_fetch_handle(
    handle: JoinHandle<FetchCompletion>,
) -> Result<FetchCompletion, RuntimeError> {
    handle.await.map_err(|error| {
        RuntimeError::FetchTask(if error.is_cancelled() {
            "fetch task was cancelled".to_owned()
        } else {
            error.to_string()
        })
    })
}

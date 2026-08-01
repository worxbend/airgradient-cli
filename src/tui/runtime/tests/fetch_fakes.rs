//! Fetch-worker doubles for the runtime event loop.
//!
//! [`HarnessFetcher`] is the workhorse: it hands back scripted completions
//! (including `None` for "still in flight") and counts cancellations, so the
//! scheduler's coalescing and cancel-on-shutdown rules are observable.
//! The other two spawn real tokio tasks, which is what makes cancellation of
//! genuinely pending work testable.

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, SystemTime},
};

use serde_json::Value;
use tokio::task::JoinHandle;
use url::Url;

use crate::tui::runtime::{
    RuntimeError,
    fetch::{FetchCompletion, MeasureFetchWorker},
};

use super::successful_payload;

#[derive(Debug)]
pub(super) struct HarnessFetcher {
    pub(super) completions: VecDeque<Option<Result<Value, String>>>,
    pub(super) stale_completions_after_cancel: VecDeque<Result<Value, String>>,
    pub(super) calls: Vec<Url>,
    pub(super) active_fetch: bool,
    pub(super) canceled_fetches: usize,
    pub(super) cancel_error: Option<&'static str>,
}

impl HarnessFetcher {
    pub(super) fn new(results: impl IntoIterator<Item = Result<Value, String>>) -> Self {
        Self {
            completions: results.into_iter().map(Some).collect(),
            stale_completions_after_cancel: VecDeque::new(),
            calls: Vec::new(),
            active_fetch: false,
            canceled_fetches: 0,
            cancel_error: None,
        }
    }

    pub(super) fn pending_then(
        results: impl IntoIterator<Item = Option<Result<Value, String>>>,
    ) -> Self {
        Self {
            completions: results.into_iter().collect(),
            stale_completions_after_cancel: VecDeque::new(),
            calls: Vec::new(),
            active_fetch: false,
            canceled_fetches: 0,
            cancel_error: None,
        }
    }

    pub(super) fn with_stale_after_cancel(
        mut self,
        results: impl IntoIterator<Item = Result<Value, String>>,
    ) -> Self {
        self.stale_completions_after_cancel = results.into_iter().collect();
        self
    }

    pub(super) fn fail_cancel(mut self, message: &'static str) -> Self {
        self.cancel_error = Some(message);
        self
    }
}

impl MeasureFetchWorker for HarnessFetcher {
    fn start_fetch(&mut self, base_url: Url) {
        assert!(
            !self.active_fetch,
            "scheduler started overlapping harness fetches"
        );
        self.active_fetch = true;
        self.calls.push(base_url);
    }

    async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
        if !self.active_fetch {
            return Ok(self
                .stale_completions_after_cancel
                .pop_front()
                .map(|result| FetchCompletion {
                    result,
                    duration: Duration::from_millis(25),
                    completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
                }));
        }

        let Some(result) = self.completions.pop_front().flatten() else {
            return Ok(None);
        };
        self.active_fetch = false;
        Ok(Some(FetchCompletion {
            result,
            duration: Duration::from_millis(25),
            completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
        }))
    }

    async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
        if self.active_fetch {
            self.canceled_fetches += 1;
            self.active_fetch = false;
        }
        if let Some(message) = self.cancel_error.take() {
            return Err(RuntimeError::FetchTask(message.to_owned()));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct YieldingFetcher {
    pub(super) sender: Sender<FetchCompletion>,
    pub(super) receiver: Receiver<FetchCompletion>,
    pub(super) completed: Arc<AtomicBool>,
    pub(super) active_handle: Option<JoinHandle<()>>,
    pub(super) calls: Vec<Url>,
}

impl YieldingFetcher {
    pub(super) fn new(completed: Arc<AtomicBool>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            completed,
            active_handle: None,
            calls: Vec::new(),
        }
    }
}

impl MeasureFetchWorker for YieldingFetcher {
    fn start_fetch(&mut self, base_url: Url) {
        assert!(
            self.active_handle.is_none(),
            "scheduler started overlapping yielding fetches"
        );
        self.calls.push(base_url);

        let sender = self.sender.clone();
        let completed = self.completed.clone();
        self.active_handle = Some(tokio::spawn(async move {
            tokio::task::yield_now().await;
            completed.store(true, Ordering::SeqCst);
            let _ = sender.send(FetchCompletion {
                result: Ok(successful_payload()),
                duration: Duration::from_millis(1),
                completed_at: SystemTime::UNIX_EPOCH + Duration::from_secs(60),
            });
        }));
    }

    async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
        Ok(self.receiver.try_recv().ok())
    }

    async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
        if let Some(handle) = self.active_handle.take() {
            handle.abort();
            match handle.await {
                Ok(()) => {}
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(RuntimeError::FetchTask(error.to_string())),
            }
        }
        while self.receiver.try_recv().is_ok() {}
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(super) struct SpawnedPendingFetcher {
    pub(super) active_handle: Option<JoinHandle<()>>,
    pub(super) calls: Vec<Url>,
    pub(super) cancellation_observed: bool,
}

impl MeasureFetchWorker for SpawnedPendingFetcher {
    fn start_fetch(&mut self, base_url: Url) {
        assert!(
            self.active_handle.is_none(),
            "scheduler started overlapping pending fetches"
        );
        self.calls.push(base_url);
        self.active_handle = Some(tokio::spawn(async {
            std::future::pending::<()>().await;
        }));
    }

    async fn recv_ready_fetch(&mut self) -> Result<Option<FetchCompletion>, RuntimeError> {
        Ok(None)
    }

    async fn cancel_fetch(&mut self) -> Result<(), RuntimeError> {
        if let Some(handle) = self.active_handle.take() {
            handle.abort();
            match handle.await {
                Ok(()) => {}
                Err(error) if error.is_cancelled() => {
                    self.cancellation_observed = true;
                }
                Err(error) => return Err(RuntimeError::FetchTask(error.to_string())),
            }
        }
        Ok(())
    }
}

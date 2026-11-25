//! Runs a study's calculation away from the UI thread: on a worker thread of its own natively,
//! in time slices of the caller on wasm32, whose web build has no threads. A strategy's decision
//! runs there too, natively on its worker's deep stack, and the caller waits for it.

use super::limits::Limits;
use super::program::{ScriptSpec, Span};
use super::{Outcome, Program, Request, RunState};
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc,
};

/// The stack of a thread that runs a script, reserved but only used as deep as the script goes.
/// The deepest value the limits admit, built and compared in the deepest call they admit, needs
/// under 16 MiB in a debug build.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const STACK: usize = 32 << 20;

/// Where a request stands.
pub(crate) enum Poll {
    Running(f32),
    Done(Box<Outcome>),
}

/// The calculation of one study, started by its first request.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct Runner {
    requests: mpsc::Sender<Request>,
    // Behind a mutex only for `Sync`; `&mut self` reaches it without locking.
    outcomes: Mutex<mpsc::Receiver<Outcome>>,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Runner {
    /// How long a request waits for its result before the caller goes on, so that a run of a
    /// few bars, such as a tick's, ends in the call that asked for it.
    const WAIT: std::time::Duration = std::time::Duration::from_millis(1);

    /// The runner of `program`, whose worker thread is named `thread`.
    pub fn new<P: Program>(
        thread: &str,
        program: Arc<P>,
        spec: Arc<ScriptSpec>,
        series_spans: Arc<[Span]>,
        history_spans: Arc<[Span]>,
        limits: Limits,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut state = RunState::new(
            program,
            spec,
            series_spans,
            history_spans,
            limits,
            Some(cancel.clone()),
        );
        let progress = state.progress();
        let (requests, received) = mpsc::channel::<Request>();
        let (finished, outcomes) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name(thread.into())
            .stack_size(STACK)
            .spawn(move || {
                for request in received {
                    state.begin(request);
                    let Some(outcome) = state.advance(None) else {
                        continue;
                    };
                    if finished.send(outcome).is_err() {
                        break;
                    }
                }
            });
        // Without a thread the requests go unanswered and the study reports a stop.
        drop(spawned);
        Self {
            requests,
            outcomes: Mutex::new(outcomes),
            progress,
            cancel,
        }
    }

    /// Start a run; the previous one must have ended.
    pub fn send(&mut self, request: Request) -> Poll {
        self.progress.store(0f32.to_bits(), Ordering::Relaxed);
        if self.requests.send(request).is_err() {
            return Poll::Done(Box::new(super::lost()));
        }
        let outcomes = self.outcomes.get_mut().unwrap_or_else(|e| e.into_inner());
        match outcomes.recv_timeout(Self::WAIT) {
            Ok(outcome) => Poll::Done(Box::new(outcome)),
            Err(mpsc::RecvTimeoutError::Timeout) => Poll::Running(0.0),
            Err(mpsc::RecvTimeoutError::Disconnected) => Poll::Done(Box::new(super::lost())),
        }
    }

    /// Run `request` and wait for its outcome; the previous run must have ended.
    pub fn decide(&mut self, request: Request) -> Outcome {
        if self.requests.send(request).is_err() {
            return super::lost();
        }
        let outcomes = self.outcomes.get_mut().unwrap_or_else(|e| e.into_inner());
        outcomes.recv().unwrap_or_else(|_| super::lost())
    }

    /// The run's outcome once it ended.
    pub fn poll(&mut self) -> Poll {
        let outcomes = self.outcomes.get_mut().unwrap_or_else(|e| e.into_inner());
        match outcomes.try_recv() {
            Ok(outcome) => Poll::Done(Box::new(outcome)),
            Err(mpsc::TryRecvError::Empty) => {
                Poll::Running(f32::from_bits(self.progress.load(Ordering::Relaxed)))
            }
            Err(mpsc::TryRecvError::Disconnected) => Poll::Done(Box::new(super::lost())),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for Runner {
    fn drop(&mut self) {
        // The worker ends at its next check, or once its channel closes when idle.
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The calculation of one study, run in slices of [`SLICE`](super::limits::SLICE) per call.
#[cfg(target_arch = "wasm32")]
pub(crate) struct Runner {
    // Behind a mutex only for `Sync`; `&mut self` reaches it without locking.
    state: Mutex<Box<dyn Slices>>,
}

/// A [`RunState`] whose program type is erased.
#[cfg(target_arch = "wasm32")]
trait Slices: Send {
    fn begin(&mut self, request: Request);
    fn decide(&mut self, request: Request) -> Option<Outcome>;
    fn advance(&mut self, until: web_time::Instant) -> Option<Outcome>;
    fn fraction(&self) -> f32;
}

#[cfg(target_arch = "wasm32")]
impl<P: Program> Slices for RunState<P> {
    fn begin(&mut self, request: Request) {
        RunState::begin(self, request);
    }
    fn decide(&mut self, request: Request) -> Option<Outcome> {
        RunState::begin(self, request);
        RunState::advance(self, None)
    }
    fn advance(&mut self, until: web_time::Instant) -> Option<Outcome> {
        RunState::advance(self, Some(until))
    }
    fn fraction(&self) -> f32 {
        RunState::fraction(self)
    }
}

#[cfg(target_arch = "wasm32")]
impl Runner {
    /// The runner of `program`; the web has no thread to name `thread`.
    pub fn new<P: Program>(
        _thread: &str,
        program: Arc<P>,
        spec: Arc<ScriptSpec>,
        series_spans: Arc<[Span]>,
        history_spans: Arc<[Span]>,
        limits: Limits,
    ) -> Self {
        let state = RunState::new(program, spec, series_spans, history_spans, limits, None);
        Self {
            state: Mutex::new(Box::new(state)),
        }
    }

    pub fn send(&mut self, request: Request) -> Poll {
        let state = self.state.get_mut().unwrap_or_else(|e| e.into_inner());
        state.begin(request);
        self.poll()
    }

    /// Run `request` on this thread to its end.
    pub fn decide(&mut self, request: Request) -> Outcome {
        let state = self.state.get_mut().unwrap_or_else(|e| e.into_inner());
        state.decide(request).expect("a run without a slice ends")
    }

    pub fn poll(&mut self) -> Poll {
        let state = self.state.get_mut().unwrap_or_else(|e| e.into_inner());
        match state.advance(web_time::Instant::now() + super::limits::SLICE) {
            Some(outcome) => Poll::Done(Box::new(outcome)),
            None => Poll::Running(state.fraction()),
        }
    }
}

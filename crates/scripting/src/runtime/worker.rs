//! Runs a study's calculation away from the UI thread: on a worker thread of its own natively,
//! in time slices of the caller on wasm32, whose web build has no threads.

use super::limits::Limits;
use super::program::{ScriptSpec, Span};
use super::{Outcome, Program, Request, RunState};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc,
};

/// The stack of a thread that runs a script. The deepest scripts the limits admit need under
/// 6 MiB in a debug build.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const STACK: usize = 16 << 20;

/// Where a request stands.
pub(crate) enum Poll {
    Running(f32),
    Done(Outcome),
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

    pub fn new<P: Program>(
        program: Arc<P>,
        spec: Arc<ScriptSpec>,
        series_spans: Arc<[Span]>,
        limits: Limits,
    ) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut state = RunState::new(program, spec, series_spans, limits, Some(cancel.clone()));
        let progress = state.progress();
        let (requests, received) = mpsc::channel::<Request>();
        let (finished, outcomes) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("script study".into())
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
            return Poll::Done(super::lost());
        }
        let outcomes = self.outcomes.get_mut().unwrap_or_else(|e| e.into_inner());
        match outcomes.recv_timeout(Self::WAIT) {
            Ok(outcome) => Poll::Done(outcome),
            Err(mpsc::RecvTimeoutError::Timeout) => Poll::Running(0.0),
            Err(mpsc::RecvTimeoutError::Disconnected) => Poll::Done(super::lost()),
        }
    }

    /// The run's outcome once it ended.
    pub fn poll(&mut self) -> Poll {
        let outcomes = self.outcomes.get_mut().unwrap_or_else(|e| e.into_inner());
        match outcomes.try_recv() {
            Ok(outcome) => Poll::Done(outcome),
            Err(mpsc::TryRecvError::Empty) => {
                Poll::Running(f32::from_bits(self.progress.load(Ordering::Relaxed)))
            }
            Err(mpsc::TryRecvError::Disconnected) => Poll::Done(super::lost()),
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

/// The calculation of one study, run in slices of [`Runner::SLICE`] per call.
#[cfg(target_arch = "wasm32")]
pub(crate) struct Runner {
    state: Box<dyn Slices>,
}

/// A [`RunState`] whose program type is erased.
#[cfg(target_arch = "wasm32")]
trait Slices: Send + Sync {
    fn begin(&mut self, request: Request);
    fn advance(&mut self, until: web_time::Instant) -> Option<Outcome>;
    fn fraction(&self) -> f32;
}

#[cfg(target_arch = "wasm32")]
impl<P: Program> Slices for RunState<P> {
    fn begin(&mut self, request: Request) {
        RunState::begin(self, request);
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
    /// The time a call runs bars before it returns; a bar is never split.
    const SLICE: std::time::Duration = std::time::Duration::from_millis(4);

    pub fn new<P: Program>(
        program: Arc<P>,
        spec: Arc<ScriptSpec>,
        series_spans: Arc<[Span]>,
        limits: Limits,
    ) -> Self {
        Self {
            state: Box::new(RunState::new(program, spec, series_spans, limits, None)),
        }
    }

    pub fn send(&mut self, request: Request) -> Poll {
        self.state.begin(request);
        self.poll()
    }

    pub fn poll(&mut self) -> Poll {
        match self.state.advance(web_time::Instant::now() + Self::SLICE) {
            Some(outcome) => Poll::Done(outcome),
            None => Poll::Running(self.state.fraction()),
        }
    }
}

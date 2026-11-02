//! Host-supplied options turnover for an underlying instrument.

use chrono::NaiveDate;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex, PoisonError, Weak};

/// Options whose traded contract volumes the host sums at each strike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsVolumeQuery {
    /// Underlying symbol as the feed knows it.
    pub symbol: String,
    /// Inclusive expiry dates. Use the same date at both ends for one expiry.
    pub expiries: RangeInclusive<NaiveDate>,
}

/// Traded contract volume at one strike, summed over the requested expiries.
/// This is turnover, not open interest, premium or the underlying's bar volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OptionsVolume {
    /// Strike in the underlying's price units.
    pub strike: f64,
    /// Nonnegative, finite call contract volume.
    pub calls: f64,
    /// Nonnegative, finite put contract volume.
    pub puts: f64,
}

pub(super) struct Snapshot<T> {
    values: Mutex<T>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl<T> Snapshot<T> {
    pub(super) fn new(values: T, wake: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            values: Mutex::new(values),
            wake: Box::new(wake),
        })
    }

    pub(super) fn read<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.values.lock().unwrap_or_else(PoisonError::into_inner))
    }

    pub(super) fn update<R>(&self, update: impl FnOnce(&mut T) -> Option<R>) -> Option<R> {
        let result = update(&mut self.values.lock().unwrap_or_else(PoisonError::into_inner));
        if result.is_some() {
            (self.wake)();
        }
        result
    }
}

/// A feed's options snapshots for one underlying and expiry query. Cheap to clone.
/// Only the latest snapshot is retained, so updates cannot accumulate between frames.
#[derive(Clone)]
pub struct OptionsVolumeSink(Weak<Snapshot<Arc<[OptionsVolume]>>>);

impl OptionsVolumeSink {
    /// Replace all figures, including clearing them with an empty snapshot, and wake the host.
    /// Pushes after the last consumer is dropped are ignored.
    pub fn push(&self, values: Vec<OptionsVolume>) {
        if let Some(snapshot) = self.0.upgrade() {
            snapshot.update(|latest| {
                *latest = values.into();
                Some(())
            });
        }
    }

    /// Whether a profile still consumes this query. Stop producing when false.
    pub fn is_active(&self) -> bool {
        self.0.strong_count() > 0
    }
}

#[cfg(feature = "studies")]
#[derive(Clone)]
pub(crate) struct OptionsVolumeSource(Arc<Snapshot<Arc<[OptionsVolume]>>>);

#[cfg(feature = "studies")]
impl OptionsVolumeSource {
    pub(crate) fn new(wake: impl Fn() + Send + Sync + 'static) -> (Self, OptionsVolumeSink) {
        let snapshot = Snapshot::new(Arc::from([]), wake);
        let sink = OptionsVolumeSink(Arc::downgrade(&snapshot));
        (Self(snapshot), sink)
    }

    pub(crate) fn values(&self) -> Arc<[OptionsVolume]> {
        self.0.read(Arc::clone)
    }
}

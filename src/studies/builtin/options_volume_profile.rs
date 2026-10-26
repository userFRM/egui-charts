//! Options turnover supplied by a host, independent of the underlying's bar volume.

use crate::data::{Datafeed, OptionsVolume, OptionsVolumeQuery, OptionsVolumeSource};
use crate::model::{Bar, Instrument};
use crate::studies::{
    Indicator, IndicatorValues, ProfileDisplay, StudyProfile, VolumeProfile, VolumeProfileConfig,
};
use egui::Color32;
use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Default)]
struct CachedProfile {
    values: Arc<[OptionsVolume]>,
    profile: Option<Arc<VolumeProfile>>,
}

/// Call and put turnover at strike prices on the price pane, scaled to the largest row total.
/// The host chooses an underlying and inclusive expiry range and supplies complete snapshots.
/// Clones share the subscription; dropping its last clone makes the feed's sink inactive.
#[derive(Clone)]
pub struct OptionsVolumeProfile {
    source: OptionsVolumeSource,
    symbol: String,
    matching_instrument: bool,
    config: VolumeProfileConfig,
    cache: RefCell<CachedProfile>,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl OptionsVolumeProfile {
    /// Subscribe through `feed`, outside egui closures, and wake the host on each snapshot.
    /// Pass a repaint callback, for example `move || ctx.request_repaint()` with a cloned
    /// context. Rows are centered on multiples of `row_size`, in underlying price units.
    /// Returns `None` for an empty symbol, reversed expiry range or nonpositive/nonfinite size.
    /// Add the result with `Chart::add_indicator`. The query is host-owned: recreate the
    /// study when changing the underlying, expiry range or row size. A chart with metadata
    /// for another symbol hides the profile until its underlying is shown again.
    ///
    /// ```
    /// # use egui_charts::data::{Datafeed, HistoryRequest, HistoryPage, Reply};
    /// # struct Feed;
    /// # impl Datafeed for Feed {
    /// #     fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
    /// #         reply.send(Ok(HistoryPage::default()));
    /// #     }
    /// # }
    /// use egui_charts::data::OptionsVolumeQuery;
    /// use egui_charts::studies::OptionsVolumeProfile;
    /// let expiry = chrono::NaiveDate::from_ymd_opt(2026, 12, 18).unwrap();
    /// let query = OptionsVolumeQuery { symbol: "SAMPLE".into(), expiries: expiry..=expiry };
    /// let profile = OptionsVolumeProfile::new(&Feed, query, 5.0, || {}).unwrap();
    /// ```
    pub fn new(
        feed: &dyn Datafeed,
        query: OptionsVolumeQuery,
        row_size: f64,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Option<Self> {
        if query.symbol.is_empty() || query.expiries.is_empty() {
            return None;
        }
        let mut config = VolumeProfileConfig::default().with_row_size(Some(row_size))?;
        config.display = ProfileDisplay::UpDown;
        let (source, sink) = OptionsVolumeSource::new(wake);
        let symbol = query.symbol.clone();
        feed.options_volume(query, sink);
        Some(Self {
            source,
            symbol,
            matching_instrument: true,
            config,
            cache: RefCell::new(CachedProfile::default()),
            values: IndicatorValues::new(),
            colors: vec![
                Color32::from_rgba_unmultiplied(38, 166, 154, 180),
                Color32::from_rgba_unmultiplied(239, 83, 80, 180),
            ],
            visible: true,
        })
    }
}

impl Indicator for OptionsVolumeProfile {
    fn name(&self) -> &str {
        "Options Volume Profile"
    }
    fn desc(&self) -> &str {
        "Call and put contract volume by strike, summed over the host's chosen expiries"
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.values = IndicatorValues::missing(data.len());
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.matching_instrument = instrument.is_none_or(|i| i.symbol == self.symbol);
        false
    }
    fn volume_profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
        if !self.visible || !self.matching_instrument || visible.is_empty() {
            return Vec::new();
        }
        let values = self.source.values();
        let mut cache = self.cache.borrow_mut();
        if !Arc::ptr_eq(&values, &cache.values) {
            cache.profile = VolumeProfile::from_options(
                &values,
                self.config.row_size().expect("fixed options row height"),
            )
            .map(Arc::new);
            cache.values = values;
        }
        let Some(profile) = cache.profile.clone() else {
            return Vec::new();
        };
        let mut config = self.config;
        config.up_color = self.colors[0].to_srgba_unmultiplied();
        config.down_color = self.colors[1].to_srgba_unmultiplied();
        vec![StudyProfile {
            bars: visible,
            profile,
            config,
            color: self.colors[0].to_srgba_unmultiplied(),
            right_aligned: true,
        }]
    }
    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (stored, color) in self.colors.iter_mut().zip(colors) {
            *stored = color;
        }
    }
    fn line_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn is_visible(&self) -> bool {
        self.visible
    }
    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, HistoryRequest, OptionsVolumeSink, Reply};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Feed(Mutex<Option<(OptionsVolumeQuery, OptionsVolumeSink)>>);

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }
        fn options_volume(&self, query: OptionsVolumeQuery, sink: OptionsVolumeSink) {
            *self.0.lock().unwrap() = Some((query, sink));
        }
    }

    /// Snapshot replacement must work without a new underlying bar, through clones, and old
    /// sinks must stop once their last profile is gone. Empty snapshots remove the geometry.
    #[test]
    fn options_snapshots_replace_rows_and_end_with_the_last_consumer() {
        let feed = Feed::default();
        let start = chrono::NaiveDate::from_ymd_opt(2026, 12, 18).unwrap();
        let end = chrono::NaiveDate::from_ymd_opt(2027, 1, 15).unwrap();
        for end in [start, end] {
            let query = OptionsVolumeQuery {
                symbol: "SAMPLE".into(),
                expiries: start..=end,
            };
            let wakes = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&wakes);
            let mut study = OptionsVolumeProfile::new(&feed, query.clone(), 4.0, move || {
                counter.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap();
            let (received, sink) = feed.0.lock().unwrap().clone().unwrap();
            assert_eq!(received, query);
            assert!(study.volume_profiles(0..10).is_empty());
            let clone = study.clone_box();
            for (calls, puts) in [(90.0, 50.0), (30.0, 70.0)] {
                // Intermediate snapshots coalesce before painting; totals are replaced, never added.
                sink.push(vec![OptionsVolume {
                    strike: 100.0,
                    calls: 999.0,
                    puts: 999.0,
                }]);
                sink.push(vec![OptionsVolume {
                    strike: 100.0,
                    calls,
                    puts,
                }]);
                for consumer in [&study as &dyn Indicator, &*clone] {
                    let plot = consumer.volume_profiles(2..7).remove(0);
                    assert_eq!(plot.bars, 2..7);
                    assert!(plot.right_aligned);
                    assert_eq!(plot.profile.up(), [calls]);
                    assert_eq!(plot.profile.down(), [puts]);
                    assert_eq!(plot.profile.volume(), [calls + puts]);
                }
            }
            for (symbol, shown) in [("SAMPLE", true), ("OTHER", false), ("SAMPLE", true)] {
                study.set_instrument(Some(&Instrument::new(symbol)));
                assert_eq!(study.volume_profiles(0..10).is_empty(), !shown, "{symbol}");
            }
            sink.push(Vec::new());
            assert!(clone.volume_profiles(0..10).is_empty());
            drop(study);
            assert!(sink.is_active());
            drop(clone);
            assert!(!sink.is_active());
            let before = wakes.load(Ordering::Relaxed);
            sink.push(Vec::new());
            assert_eq!(wakes.load(Ordering::Relaxed), before);
        }
    }
}

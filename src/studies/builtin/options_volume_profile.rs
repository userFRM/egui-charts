//! Call and put volume and open interest supplied by a host, independent of bar volume.

use crate::data::{
    Datafeed, OptionChain, OptionKind, OptionQuote, OptionsVolume, OptionsVolumeQuery,
    OptionsVolumeSource,
};
use crate::data::{ProfileDisplay, StudyProfile, VolumeProfile, VolumeProfileConfig};
use crate::model::{Bar, Instrument};
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, PlotStyle, StudyError, StudyInput,
};
use chrono::{NaiveDate, Utc};
use egui::Color32;
use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Default)]
struct CachedProfile {
    values: Arc<[OptionsVolume]>,
    profile: Option<Arc<VolumeProfile>>,
    chain_rows: Arc<[OptionQuote]>,
    day: Option<NaiveDate>,
    ratio: Option<f64>,
    pain: Option<f64>,
    volume_available: bool,
    interest_available: bool,
    pain_available: bool,
}

#[derive(Clone)]
enum ProfileSource {
    Volume(OptionsVolumeSource),
    Chain(OptionChain, Vec<NaiveDate>),
}

#[derive(Clone, Copy, Default)]
enum Expiries {
    #[default]
    Nearest,
    One(NaiveDate),
    Through,
}

/// Call and put volume or open interest by strike, scaled to the largest row total.
/// Hosts supply complete snapshots through the volume-only constructor or [`Self::from_chain`].
/// Clones share the subscription; dropping its last clone makes the feed's sink inactive.
#[derive(Clone)]
pub struct OptionsVolumeProfile {
    source: ProfileSource,
    expiries: Expiries,
    through_date: NaiveDate,
    open_interest: bool,
    max_pain: bool,
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
            source: ProfileSource::Volume(source),
            expiries: Expiries::Nearest,
            through_date: NaiveDate::MIN,
            open_interest: false,
            max_pain: false,
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

    /// Draw a shared option-chain subscription for `symbol` and the feed's listed expiries.
    /// The chain query must cover those expiries. Expiry inputs default to the first listed
    /// date on or after the current UTC day; a date equal to today is still eligible.
    /// Measures with missing figures in any selected contract are unavailable, never zero.
    /// Max pain needs every selected contract's open interest and multiplier. It minimises
    /// their combined intrinsic payout over listed strikes, choosing the lower strike on ties.
    /// Each row spans the common increment of the selected strikes, one unit for a single strike.
    /// Returns `None` for an empty symbol or expiry list.
    pub fn from_chain(
        chain: OptionChain,
        symbol: impl Into<String>,
        mut expiries: Vec<NaiveDate>,
    ) -> Option<Self> {
        let symbol = symbol.into();
        if symbol.is_empty() || expiries.is_empty() {
            return None;
        }
        expiries.sort_unstable();
        expiries.dedup();
        let mut config = VolumeProfileConfig::default();
        config.display = ProfileDisplay::UpDown;
        Some(Self {
            through_date: *expiries.last()?,
            source: ProfileSource::Chain(chain, expiries),
            expiries: Expiries::Nearest,
            open_interest: false,
            max_pain: false,
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

    fn refresh(&self) {
        let mut cache = self.cache.borrow_mut();
        let (values, row_size) = match &self.source {
            ProfileSource::Volume(source) => {
                let values = source.values();
                if Arc::ptr_eq(&values, &cache.values) {
                    return;
                }
                let row_size = self.config.row_size().expect("fixed options row height");
                (values, row_size)
            }
            ProfileSource::Chain(chain, expiries) => {
                let rows = chain.rows();
                let day = Utc::now().date_naive();
                if Arc::ptr_eq(&rows, &cache.chain_rows) && cache.day == Some(day) {
                    return;
                }
                let nearest = expiries.iter().copied().find(|date| *date >= day);
                let selected: Vec<_> = rows
                    .iter()
                    .filter(|row| {
                        expiries.binary_search(&row.expiry).is_ok()
                            && match self.expiries {
                                Expiries::Nearest => Some(row.expiry) == nearest,
                                Expiries::One(date) => row.expiry == date,
                                Expiries::Through => row.expiry <= self.through_date,
                            }
                    })
                    .collect();
                cache.volume_available =
                    !selected.is_empty() && selected.iter().all(|row| row.volume.is_some());
                cache.interest_available =
                    !selected.is_empty() && selected.iter().all(|row| row.open_interest.is_some());
                cache.pain_available =
                    cache.interest_available && selected.iter().all(|row| row.multiplier.is_some());
                cache.pain = if self.max_pain && cache.pain_available {
                    max_pain(&selected)
                } else {
                    None
                };
                let interest =
                    cache.interest_available && (self.open_interest || !cache.volume_available);
                let values: Arc<[OptionsVolume]> = if interest || cache.volume_available {
                    selected
                        .iter()
                        .map(|row| {
                            let value = if interest {
                                row.open_interest
                            } else {
                                row.volume
                            }
                            .expect("available measure");
                            OptionsVolume {
                                strike: row.strike,
                                calls: if row.kind == OptionKind::Call {
                                    value
                                } else {
                                    0.0
                                },
                                puts: if row.kind == OptionKind::Put {
                                    value
                                } else {
                                    0.0
                                },
                            }
                        })
                        .collect()
                } else {
                    Arc::from([])
                };
                cache.chain_rows = rows;
                cache.day = Some(day);
                let row_size = strike_step(&values);
                (values, row_size)
            }
        };
        let calls: f64 = values.iter().map(|row| row.calls).sum();
        let puts: f64 = values.iter().map(|row| row.puts).sum();
        cache.ratio = (calls > 0.0 && calls.is_finite() && puts.is_finite())
            .then_some(puts / calls)
            .filter(|ratio| ratio.is_finite());
        cache.profile = VolumeProfile::from_options(&values, row_size).map(Arc::new);
        cache.values = values;
    }
}

// The greatest common divisor of strike gaps, so every listed strike is a row centre.
fn strike_step(rows: &[OptionsVolume]) -> f64 {
    let mut strikes: Vec<_> = rows.iter().map(|row| row.strike).collect();
    strikes.sort_by(f64::total_cmp);
    let mut step = 0.0;
    for gap in strikes.windows(2).map(|pair| pair[1] - pair[0]) {
        let (mut a, mut b) = (gap, step);
        while b > 1e-6 {
            (a, b) = (b, a % b);
        }
        step = a;
    }
    if step > 1e-6 { step } else { 1.0 }
}

// Payout is convex: at each strike its slope increases by call plus put interest,
// weighted by the contract multiplier. The first nonnegative slope gives the lower minimum.
fn max_pain(rows: &[&OptionQuote]) -> Option<f64> {
    let mut weights: Vec<_> = rows
        .iter()
        .map(|row| {
            let weight = row.open_interest? * row.multiplier?;
            weight.is_finite().then_some((row.strike, row.kind, weight))
        })
        .collect::<Option<_>>()?;
    weights.sort_by(|a, b| a.0.total_cmp(&b.0));
    let puts: f64 = weights
        .iter()
        .filter(|row| row.1 == OptionKind::Put)
        .map(|row| row.2)
        .sum();
    if !puts.is_finite() {
        return None;
    }
    let mut accumulated = 0.0;
    for (strike, _, weight) in weights {
        accumulated += weight;
        if !accumulated.is_finite() {
            return None;
        }
        if accumulated >= puts {
            return Some(strike);
        }
    }
    None
}

impl Indicator for OptionsVolumeProfile {
    fn name(&self) -> &str {
        "Options Volume Profile"
    }
    fn desc(&self) -> &str {
        "Call and put volume or open interest by strike, with expiry selection, put/call ratio and max pain"
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
        self.refresh();
        let cache = self.cache.borrow();
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
        if matches!(self.source, ProfileSource::Chain(..)) {
            vec!["Put/call".into(), "Max pain".into()]
        } else {
            Vec::new()
        }
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            visible: line == 1,
            color: self.colors.get(line).copied().unwrap_or(self.colors[0]),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Line,
        }
    }
    fn legend_visible(&self, line: usize) -> bool {
        line == 0 || (line == 1 && self.max_pain)
    }
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        if !self.visible || !self.matching_instrument || bar >= self.values.len() {
            return None;
        }
        self.refresh();
        let cache = self.cache.borrow();
        match line {
            0 => cache.ratio,
            1 if self.max_pain => cache.pain,
            _ => None,
        }
    }
    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(2)
    }
    fn inputs(&self) -> Vec<StudyInput> {
        let ProfileSource::Chain(_, dates) = &self.source else {
            return Vec::new();
        };
        self.refresh();
        let cache = self.cache.borrow();
        let mut choices = vec!["Nearest".to_owned()];
        choices.extend(dates.iter().map(ToString::to_string));
        choices.push("Through date".into());
        let expiry = match self.expiries {
            Expiries::Nearest => 0,
            Expiries::One(date) => 1 + dates.binary_search(&date).expect("listed expiry"),
            Expiries::Through => 1 + dates.len(),
        };
        let mut inputs = vec![
            StudyInput::new(
                "expiry",
                "Expiry",
                InputValue::Choice(expiry),
                InputKind::Choice(choices),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "sides",
                "Sides",
                InputValue::Choice(usize::from(self.config.display == ProfileDisplay::Delta)),
                InputKind::Choice(vec!["Split".into(), "Net (calls minus puts)".into()]),
            )
            .with_default(InputValue::Choice(0)),
        ];
        inputs.push(StudyInput::new(
            "through_date",
            "Through date (YYYY-MM-DD)",
            InputValue::Text(self.through_date.to_string()),
            InputKind::Text,
        ));
        let mut measures = Vec::new();
        if cache.volume_available {
            measures.push("Volume".into());
        }
        if cache.interest_available {
            measures.push("Open interest".into());
        }
        if !measures.is_empty() {
            let selected = usize::from(
                self.open_interest && cache.volume_available && cache.interest_available,
            );
            inputs.push(
                StudyInput::new(
                    "measure",
                    "Measure",
                    InputValue::Choice(selected),
                    InputKind::Choice(measures),
                )
                .with_default(InputValue::Choice(0)),
            );
        }
        if cache.pain_available {
            inputs.push(
                StudyInput::new(
                    "max_pain",
                    "Max pain",
                    InputValue::Toggle(self.max_pain),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(false)),
            );
        }
        inputs
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), StudyError> {
        let input = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or_else(|| StudyError::UnknownInput(id.into()))?;
        if !input.accepts(&value) {
            return Err(StudyError::invalid_input(id));
        }
        match (id, value) {
            ("expiry", InputValue::Choice(choice)) => {
                let ProfileSource::Chain(_, dates) = &self.source else {
                    unreachable!()
                };
                self.expiries = if choice == 0 {
                    Expiries::Nearest
                } else if choice <= dates.len() {
                    Expiries::One(dates[choice - 1])
                } else {
                    Expiries::Through
                };
            }
            ("through_date", InputValue::Text(date)) => {
                self.through_date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                    .map_err(|_| StudyError::invalid_input(id))?;
            }
            ("measure", InputValue::Choice(choice)) => {
                let InputKind::Choice(choices) = input.kind else {
                    unreachable!()
                };
                self.open_interest = choices[choice] == "Open interest";
            }
            ("sides", InputValue::Choice(choice)) => {
                self.config.display = if choice == 0 {
                    ProfileDisplay::UpDown
                } else {
                    ProfileDisplay::Delta
                }
            }
            ("max_pain", InputValue::Toggle(enabled)) => self.max_pain = enabled,
            _ => return Err(StudyError::invalid_input(id)),
        }
        *self.cache.get_mut() = CachedProfile::default();
        Ok(())
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

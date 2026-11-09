//! Host-supplied option volume, open interest and signed gamma exposure by strike.

use crate::data::{
    Datafeed, OptionChain, OptionExposure, OptionExposureHandle, OptionExposureKind,
    OptionExposureRow, OptionKind, OptionQuote, OptionsVolume, OptionsVolumeQuery,
    OptionsVolumeSource,
};
use crate::data::{
    ExposureProfile, ProfileDisplay, StrikeExposure, StudyProfile, VolumeProfile,
    VolumeProfileConfig,
};
use crate::model::{Bar, Instrument};
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, PlotStyle, StudyError, StudyInput,
};
use chrono::{DateTime, NaiveDate, Utc};
use egui::Color32;
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Default)]
struct CachedProfile {
    values: Arc<[OptionsVolume]>,
    profile: Option<Arc<VolumeProfile>>,
    chain_rows: Arc<[OptionQuote]>,
    day: Option<NaiveDate>,
    supplied: Option<Arc<OptionExposure>>,
    exposure: Option<Arc<ExposureProfile>>,
    walls: [Option<f64>; 2],
    net: Option<f64>,
    ratio: Option<f64>,
    pain: Option<f64>,
    expected_move: Option<(NaiveDate, [f64; 2])>,
    move_time: Option<i64>,
    volume_available: bool,
    interest_available: bool,
    pain_available: bool,
    awaiting_chain: bool,
}

/// How an input the chain decides on is listed while it waits for the chain, or `None` for any
/// other id. Its saved choice of measure is kept as saved, whatever the chain then offers.
fn waiting_input(id: &str, value: InputValue) -> Option<StudyInput> {
    let (label, kind, default) = match id {
        "measure_label" if matches!(&value, InputValue::Text(label) if ["Volume", "Open interest", "Gamma exposure"].contains(&label.as_str())) =>
        {
            return Some(
                StudyInput::new(id, "Measure label", value, InputKind::Text)
                    .shown_when("measure", Vec::new()),
            );
        }
        "measure" => (
            "Measure",
            InputKind::Choice(vec![
                "Volume".into(),
                "Open interest".into(),
                "Gamma exposure".into(),
            ]),
            InputValue::Choice(0),
        ),
        "call_wall" => ("Call wall", InputKind::Toggle, InputValue::Toggle(false)),
        "put_wall" => ("Put wall", InputKind::Toggle, InputValue::Toggle(false)),
        "zero_gamma" => (
            "Zero gamma (provider)",
            InputKind::Toggle,
            InputValue::Toggle(false),
        ),
        "max_pain" => ("Max pain", InputKind::Toggle, InputValue::Toggle(false)),
        _ => return None,
    };
    Some(StudyInput::new(id, label, value, kind).with_default(default))
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

/// Call and put volume, open interest or signed gamma exposure by strike.
/// Hosts supply complete snapshots through the volume-only constructor or [`Self::from_chain`].
/// Clones share the subscription; dropping its last clone makes the feed's sink inactive.
#[derive(Clone)]
pub struct OptionsVolumeProfile {
    source: ProfileSource,
    expiries: Expiries,
    through_date: NaiveDate,
    open_interest: Cell<bool>,
    gamma: Cell<bool>,
    gamma_split: bool,
    exposure: Option<OptionExposureHandle>,
    exposure_levels: [Cell<bool>; 3],
    max_pain: Cell<bool>,
    // Inputs the chain does not yet offer, in their saved order.
    waiting_inputs: RefCell<Vec<(String, InputValue)>>,
    expected_move: bool,
    move_iv: bool,
    as_of: Option<DateTime<Utc>>,
    last_close: Option<f64>,
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
            open_interest: Cell::new(false),
            gamma: Cell::new(false),
            gamma_split: false,
            exposure: None,
            exposure_levels: Default::default(),
            max_pain: Cell::new(false),
            waiting_inputs: RefCell::default(),
            expected_move: false,
            move_iv: false,
            as_of: None,
            last_close: None,
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
    /// Volume and interest need every selected contract's figure; missing values are never zero.
    /// Max pain needs every selected contract's open interest and multiplier. It minimises
    /// their combined intrinsic payout over listed strikes, choosing the lower strike on ties.
    /// Rows are as tall as the largest common divisor of the selected strikes, so each strike
    /// is a row centre, and coarser only where the 10,000-row limit requires.
    /// Gamma uses [`OptionExposure::gamma`], with net bars by default and optional call and
    /// put walls. Missing contracts are counted in the readout. Attach a provider observation
    /// with [`Self::with_exposure`]; its signs and net values take precedence, even if empty.
    /// Expected move is off by default. Its method defaults to the ATM call mid plus put mid;
    /// ATM is the nearest listed strike, lower on a tie. Both bid/ask pairs must be present
    /// and uncrossed. Implied volatility instead uses price × IV × sqrt(calendar days / 365),
    /// with the mean of available ATM call/put IVs. The centre is the newest supplied
    /// underlying price for that expiry, or the latest valid underlying bar close when absent.
    /// A historical fallback bar has closed when its successor opens by the query's `as_of`;
    /// the last bar is eligible from its own opening time. Option last trades
    /// never replace missing bid/ask mids.
    /// A date cutoff uses the last selected expiry. Dates end at the next UTC midnight.
    /// Nearest-expiry selection and band ageing follow UTC time for live chains and the
    /// chain query's `as_of` instant for historical ones.
    /// Missing inputs or an expired date remove both bounds and their expiry/method label.
    /// The fill spans the visible pane even when only one underlying bar has arrived.
    /// Inputs saved with data that has not arrived, such as a layout restored before its quotes,
    /// are listed and saved as they were and apply once the chain and any attached exposure
    /// offer them. Saved expiry and measure labels take precedence over their choice positions;
    /// an expiry no longer listed selects Nearest. Older saves keep position semantics.
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
            as_of: chain.query().as_of,
            last_close: None,
            through_date: *expiries.last()?,
            source: ProfileSource::Chain(chain, expiries),
            expiries: Expiries::Nearest,
            open_interest: Cell::new(false),
            gamma: Cell::new(false),
            gamma_split: false,
            exposure: None,
            exposure_levels: Default::default(),
            max_pain: Cell::new(false),
            waiting_inputs: RefCell::default(),
            expected_move: false,
            move_iv: false,
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

    /// Prefer this shared gamma subscription over the chain's computed gamma once it answers.
    /// The query must name this underlying, request gamma and cover all listed expiries;
    /// otherwise returns `None`. For historical chains the host must request the same instant.
    /// The browser attaches this automatically for feeds declaring `FeedService::OptionExposure`.
    /// Zero gamma is drawn only from the provider's `OptionExposure::zero_gamma`, never inferred.
    pub fn with_exposure(mut self, exposure: OptionExposureHandle) -> Option<Self> {
        let ProfileSource::Chain(_, dates) = &self.source else {
            return None;
        };
        let query = exposure.query();
        if query.symbol != self.symbol
            || query.kind != OptionExposureKind::Gamma
            || dates.iter().any(|date| !query.expiries.contains(date))
        {
            return None;
        }
        self.exposure = Some(exposure);
        *self.cache.get_mut() = CachedProfile::default();
        Some(self)
    }

    fn gamma_selected(&self, cache: &CachedProfile) -> bool {
        self.gamma.get()
            || (!cache.volume_available && !cache.interest_available && cache.exposure.is_some())
    }

    fn refresh(&self) {
        self.refresh_cache();
        if self.waiting_inputs.borrow().is_empty() || self.cache.borrow().awaiting_chain {
            return;
        }
        // In saved order, since the measure decides which levels are offered.
        let mut waiting = Vec::new();
        for (id, value) in self.waiting_inputs.take() {
            let input = self
                .inputs()
                .into_iter()
                .find(|input| input.id == id && input.accepts(&value));
            if input.is_some_and(|input| self.set_chain_input(&id, &value, &input)) {
                *self.cache.borrow_mut() = CachedProfile::default();
            } else {
                waiting.push((id, value));
            }
        }
        self.waiting_inputs.replace(waiting);
        self.refresh_cache();
    }

    /// Set an input the chain decides on, as `input` lists it. False for any other id.
    fn set_chain_input(&self, id: &str, value: &InputValue, input: &StudyInput) -> bool {
        match (id, value, &input.kind) {
            ("measure", InputValue::Choice(choice), InputKind::Choice(choices)) => {
                self.open_interest.set(choices[*choice] == "Open interest");
                self.gamma.set(choices[*choice] == "Gamma exposure");
            }
            ("measure_label", InputValue::Text(label), _) => {
                let cache = self.cache.borrow();
                let available = match label.as_str() {
                    "Volume" => cache.volume_available,
                    "Open interest" => cache.interest_available,
                    "Gamma exposure" => {
                        cache
                            .exposure
                            .as_ref()
                            .is_some_and(|profile| !profile.rows.is_empty())
                            || cache.supplied.is_some()
                            || self.gamma.get()
                    }
                    _ => false,
                };
                if !available {
                    return false;
                }
                self.open_interest.set(label == "Open interest");
                self.gamma.set(label == "Gamma exposure");
            }
            ("max_pain", InputValue::Toggle(enabled), _) => self.max_pain.set(*enabled),
            ("call_wall", InputValue::Toggle(enabled), _) => self.exposure_levels[0].set(*enabled),
            ("put_wall", InputValue::Toggle(enabled), _) => self.exposure_levels[1].set(*enabled),
            ("zero_gamma", InputValue::Toggle(enabled), _) => self.exposure_levels[2].set(*enabled),
            _ => return false,
        }
        true
    }

    fn refresh_cache(&self) {
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
                let now = self.as_of.unwrap_or_else(Utc::now);
                let day = now.date_naive();
                let same_rows = Arc::ptr_eq(&rows, &cache.chain_rows);
                let nearest = || expiries.iter().copied().find(|date| *date >= day);
                if self.expected_move && (!same_rows || cache.move_time != Some(now.timestamp())) {
                    let expiry = match self.expiries {
                        Expiries::Nearest => nearest(),
                        Expiries::One(date) => Some(date),
                        Expiries::Through => expiries
                            .iter()
                            .rev()
                            .copied()
                            .find(|date| *date >= day && *date <= self.through_date),
                    };
                    cache.expected_move = expiry.and_then(|date| {
                        expected_move(&rows, date, now, self.move_iv, self.last_close)
                            .map(|band| (date, band))
                    });
                    cache.move_time = Some(now.timestamp());
                }
                let supplied = self
                    .exposure
                    .as_ref()
                    .and_then(OptionExposureHandle::snapshot);
                let same_exposure = match (&supplied, &cache.supplied) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                };
                if same_rows && cache.day == Some(day) && same_exposure {
                    return;
                }
                let nearest = nearest();
                let selected_expiry = |expiry| {
                    expiries.binary_search(&expiry).is_ok()
                        && match self.expiries {
                            Expiries::Nearest => Some(expiry) == nearest,
                            Expiries::One(date) => expiry == date,
                            Expiries::Through => expiry <= self.through_date,
                        }
                };
                let selected: Vec<_> = rows
                    .iter()
                    .filter(|row| selected_expiry(row.expiry))
                    .collect();
                let exposure = if let Some(supplied) = &supplied {
                    let mut snapshot = (**supplied).clone();
                    snapshot.rows.retain(|row| selected_expiry(row.expiry));
                    if snapshot.rows.len() != supplied.rows.len() {
                        snapshot.zero_gamma = None;
                    }
                    Some(snapshot)
                } else {
                    OptionExposure::gamma(selected.iter().copied())
                        .ok()
                        .flatten()
                };
                cache.exposure = exposure.and_then(|snapshot| {
                    let rows = aggregate_exposure(&snapshot.rows, supplied.is_some())?;
                    let signs = if supplied.is_some() {
                        "Provider signs"
                    } else {
                        "Calls + / puts - (convention)"
                    };
                    let mut note = format!(
                        "{}; {signs}; {} UTC",
                        snapshot.unit,
                        snapshot.time.format("%Y-%m-%d %H:%M:%S%.3f")
                    );
                    if let Some(model) = &snapshot.model {
                        note.push_str(&format!("; {model}"));
                    }
                    if snapshot.missing_contracts > 0 {
                        note.push_str(&format!(
                            "; {} missing contracts",
                            snapshot.missing_contracts
                        ));
                    }
                    Some(Arc::new(ExposureProfile {
                        snapshot,
                        rows,
                        note,
                    }))
                });
                cache.net = cache.exposure.as_ref().and_then(|snapshot| {
                    if snapshot.rows.is_empty() {
                        return None;
                    }
                    snapshot
                        .rows
                        .iter()
                        .map(|row| row.net)
                        .sum::<Option<f64>>()
                        .filter(|net| net.is_finite())
                });
                cache.walls = [false, true].map(|puts| {
                    cache.exposure.as_ref().and_then(|snapshot| {
                        snapshot
                            .rows
                            .iter()
                            .filter_map(|row| {
                                let value = if puts { row.puts?.abs() } else { row.calls? };
                                Some((value, row.strike))
                            })
                            .filter(|(value, _)| *value > 0.0)
                            .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.total_cmp(&a.1)))
                            .map(|(_, strike)| strike)
                    })
                });
                cache.awaiting_chain =
                    selected.is_empty() || (self.exposure.is_some() && supplied.is_none());
                cache.supplied = supplied;
                cache.volume_available =
                    !selected.is_empty() && selected.iter().all(|row| row.volume.is_some());
                cache.interest_available =
                    !selected.is_empty() && selected.iter().all(|row| row.open_interest.is_some());
                cache.pain_available =
                    cache.interest_available && selected.iter().all(|row| row.multiplier.is_some());
                cache.pain = if self.max_pain.get() && cache.pain_available {
                    max_pain(&selected)
                } else {
                    None
                };
                if self.gamma_selected(&cache) {
                    cache.profile = cache.exposure.as_ref().and_then(|snapshot| {
                        let max = snapshot
                            .rows
                            .iter()
                            .flat_map(|row| [row.calls, row.puts, row.net])
                            .flatten()
                            .map(f64::abs)
                            .fold(1.0, f64::max);
                        let values: Vec<_> = snapshot
                            .rows
                            .iter()
                            .map(|row| OptionsVolume {
                                strike: row.strike,
                                calls: row
                                    .calls
                                    .unwrap_or(0.0)
                                    .abs()
                                    .max(row.net.unwrap_or(0.0).abs())
                                    / max,
                                puts: row.puts.unwrap_or(0.0).abs() / max,
                            })
                            .collect();
                        let mut profile =
                            VolumeProfile::from_options(&values, strike_step(&values))?;
                        profile.exposure = Some(Arc::clone(snapshot));
                        Some(Arc::new(profile))
                    });
                    cache.chain_rows = rows;
                    cache.day = Some(day);
                    return;
                }
                let interest = cache.interest_available
                    && (self.open_interest.get() || !cache.volume_available);
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

    fn move_band(&self) -> Option<(NaiveDate, [f64; 2])> {
        if !self.matching_instrument || !self.expected_move {
            return None;
        }
        self.refresh();
        self.cache.borrow().expected_move
    }
}

fn expected_move(
    rows: &[OptionQuote],
    expiry: NaiveDate,
    now: DateTime<Utc>,
    implied: bool,
    last_close: Option<f64>,
) -> Option<[f64; 2]> {
    let end = expiry.succ_opt()?.and_hms_opt(0, 0, 0)?.and_utc();
    if now >= end {
        return None;
    }
    let rows = || rows.iter().filter(|row| row.expiry == expiry);
    let price = rows()
        .filter(|row| row.underlying_price.is_some())
        .max_by_key(|row| row.time)
        .and_then(|row| row.underlying_price)
        .or(last_close)?;
    let strike = rows()
        .min_by(|a, b| {
            (a.strike - price)
                .abs()
                .total_cmp(&(b.strike - price).abs())
                .then_with(|| a.strike.total_cmp(&b.strike))
        })?
        .strike;
    let contract = |kind| {
        rows()
            .filter(|row| row.strike == strike && row.kind == kind)
            .max_by(|a, b| a.time.cmp(&b.time).then_with(|| b.symbol.cmp(&a.symbol)))
    };
    let (call, put) = (contract(OptionKind::Call), contract(OptionKind::Put));
    let width = if implied {
        let iv = match (
            call.and_then(|row| row.implied_volatility),
            put.and_then(|row| row.implied_volatility),
        ) {
            (Some(a), Some(b)) => a * 0.5 + b * 0.5,
            (a, b) => a.or(b)?,
        };
        let days = (end - now).num_milliseconds() as f64 / 86_400_000.0;
        price * iv * (days / 365.0).sqrt()
    } else {
        let mid = |row: &OptionQuote| {
            let (bid, ask) = (row.bid?, row.ask?);
            (bid <= ask).then_some(bid * 0.5 + ask * 0.5)
        };
        mid(call?)? + mid(put?)?
    };
    let band = [price - width, price + width];
    band.iter().all(|value| value.is_finite()).then_some(band)
}

// Computed gamma counts unusable contracts separately; an absent side must not erase
// another expiry's known contribution. Provider omissions keep their sums unknown.
fn aggregate_exposure(rows: &[OptionExposureRow], supplied: bool) -> Option<Vec<StrikeExposure>> {
    let add = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) => Some(a + b),
        _ if supplied => None,
        _ => a.or(b),
    };
    let mut ordered: Vec<_> = rows.iter().collect();
    ordered.sort_by(|a, b| a.strike.total_cmp(&b.strike));
    let mut strikes: Vec<StrikeExposure> = Vec::new();
    for row in ordered {
        if let Some(previous) = strikes.last_mut()
            && previous.strike == row.strike
        {
            previous.calls = add(previous.calls, row.calls);
            previous.puts = add(previous.puts, row.puts);
            previous.net = add(previous.net, row.net);
        } else {
            strikes.push(StrikeExposure {
                strike: row.strike,
                calls: row.calls,
                puts: row.puts,
                net: row.net,
            });
        }
        let row = strikes.last()?;
        if [row.calls, row.puts, row.net]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return None;
        }
    }
    Some(strikes)
}

// Rows are centred on multiples of their height, so it is the greatest common divisor of
// the strikes (and of 1 for a single strike), coarsened to stay within the row limit.
fn strike_step(rows: &[OptionsVolume]) -> f64 {
    let (low, high) = rows
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), row| {
            (low.min(row.strike), high.max(row.strike))
        });
    let tolerance = ((high - low) / 5_000.0).max(1e-6);
    let mut step = if high > low { 0.0 } else { 1.0 };
    for row in rows {
        let (mut a, mut b) = (row.strike, step);
        while b > tolerance {
            (a, b) = (b, a % b);
        }
        step = a;
    }
    step.max(tolerance)
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
        match self.source {
            ProfileSource::Volume(_) => {
                "Call and put contract volume by strike, summed over the host's chosen expiries"
            }
            ProfileSource::Chain(..) => {
                "Call and put volume, open interest or signed gamma exposure by strike, with expiry selection and optional levels"
            }
        }
    }
    fn calculate(&mut self, data: &[Bar]) {
        let close = if self.matching_instrument {
            data.iter().enumerate().rev().find_map(|(index, bar)| {
                (!bar.is_whitespace()
                    && bar.close.is_finite()
                    && bar.close >= 0.0
                    && self
                        .as_of
                        .is_none_or(|time| data.get(index + 1).unwrap_or(bar).time <= time))
                .then_some(bar.close)
            })
        } else {
            None
        };
        if self.last_close != close {
            self.last_close = close;
            self.cache.get_mut().move_time = None;
        }
        self.values = IndicatorValues::missing(data.len());
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        let matching = instrument.is_none_or(|i| i.symbol == self.symbol);
        let changed = self.matching_instrument != matching;
        self.matching_instrument = matching;
        if changed {
            self.last_close = None;
            self.cache.get_mut().move_time = None;
        }
        changed && matches!(self.source, ProfileSource::Chain(..))
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
        if self.gamma_selected(&cache) {
            config.display = if self.gamma_split {
                ProfileDisplay::UpDown
            } else {
                ProfileDisplay::Delta
            };
        }
        vec![StudyProfile {
            bars: visible,
            profile,
            config,
            color: self.colors[0].to_srgba_unmultiplied(),
            right_aligned: true,
            width: 0.9,
        }]
    }
    fn colors(&self) -> Vec<Color32> {
        if matches!(self.source, ProfileSource::Chain(..)) {
            (0..8).map(|line| self.plot_style(line).color).collect()
        } else {
            self.colors.clone()
        }
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        if matches!(self.source, ProfileSource::Chain(..)) && colors.len() >= 8 {
            self.colors.resize(8, self.colors[0]);
        }
        for (stored, color) in self.colors.iter_mut().zip(colors) {
            *stored = color;
        }
    }
    fn line_names(&self) -> Vec<String> {
        if matches!(self.source, ProfileSource::Chain(..)) {
            self.refresh();
            let cache = self.cache.borrow();
            let gamma = if self.gamma_selected(&cache) {
                cache.exposure.as_ref().map_or_else(
                    || "Gamma exposure".into(),
                    |profile| format!("Gamma exposure ({})", profile.note),
                )
            } else {
                "Gamma exposure".into()
            };
            vec![
                "Put/call".into(),
                "Max pain".into(),
                gamma,
                "Call wall".into(),
                "Put wall".into(),
                "Zero gamma".into(),
                "Expected low".into(),
                "Expected high".into(),
            ]
        } else {
            Vec::new()
        }
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            visible: line == 1 || (3..=7).contains(&line),
            color: if (6..=7).contains(&line) {
                self.colors
                    .get(line)
                    .copied()
                    .unwrap_or(self.colors[0].to_opaque())
            } else {
                self.colors[usize::from(line == 1 || line == 4)]
            },
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Line,
        }
    }
    fn legend_visible(&self, line: usize) -> bool {
        if (6..=7).contains(&line) {
            return self.expected_move;
        }
        self.refresh();
        if self.gamma_selected(&self.cache.borrow()) {
            line == 2 || (3..=5).contains(&line) && self.exposure_levels[line - 3].get()
        } else {
            line == 0 || (line == 1 && self.max_pain.get())
        }
    }
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        if !self.matching_instrument || bar >= self.values.len() {
            return None;
        }
        if (6..=7).contains(&line) {
            return self.move_band().map(|(_, band)| band[line - 6]);
        }
        self.refresh();
        let cache = self.cache.borrow();
        if self.gamma_selected(&cache) {
            return match line {
                2 => cache.net,
                3 | 4 if self.exposure_levels[line - 3].get() => cache.walls[line - 3],
                5 if self.exposure_levels[2].get() => cache
                    .exposure
                    .as_ref()
                    .and_then(|profile| profile.snapshot.zero_gamma),
                _ => None,
            };
        }
        match line {
            0 => cache.ratio,
            1 if self.max_pain.get() => cache.pain,
            _ => None,
        }
    }
    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        if !self.visible {
            return Vec::new();
        }
        let Some((_, [low, high])) = self.move_band() else {
            return Vec::new();
        };
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Level(low),
            second: crate::studies::FillBoundary::Level(high),
            color: self.colors[0].gamma_multiply(0.16),
            inverted_color: None,
        }]
    }
    fn plot_label(&self) -> Option<(usize, String)> {
        if !self.visible {
            return None;
        }
        let (expiry, _) = self.move_band()?;
        let method = if self.move_iv {
            "Implied volatility"
        } else {
            "ATM straddle"
        };
        Some((7, format!("Expected move\n{expiry}\n{method}")))
    }
    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(2)
    }
    fn format_plot_value(&self, line: usize, value: f64) -> String {
        if line == 2 {
            format!("{value:+.2}")
        } else {
            self.value_format().format(value, None)
        }
    }
    fn inputs(&self) -> Vec<StudyInput> {
        let ProfileSource::Chain(_, dates) = &self.source else {
            return Vec::new();
        };
        self.refresh();
        let cache = self.cache.borrow();
        let gamma = self.gamma_selected(&cache);
        let mut choices = vec!["Nearest".to_owned()];
        choices.extend(dates.iter().map(ToString::to_string));
        choices.push("Through date".into());
        let expiry = match self.expiries {
            Expiries::Nearest => 0,
            Expiries::One(date) => 1 + dates.binary_search(&date).expect("listed expiry"),
            Expiries::Through => 1 + dates.len(),
        };
        let expiry_label = choices[expiry].clone();
        let mut inputs = vec![
            StudyInput::new(
                "expiry",
                "Expiry",
                InputValue::Choice(expiry),
                InputKind::Choice(choices),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "expiry_label",
                "Expiry label",
                InputValue::Text(expiry_label),
                InputKind::Text,
            )
            .shown_when("expiry", Vec::new()),
            StudyInput::new(
                "sides",
                "Sides",
                InputValue::Choice(usize::from(if gamma {
                    !self.gamma_split
                } else {
                    self.config.display == ProfileDisplay::Delta
                })),
                InputKind::Choice(vec![
                    "Split".into(),
                    if gamma {
                        "Net (calls plus puts)".into()
                    } else {
                        "Net (calls minus puts)".into()
                    },
                ]),
            )
            .with_default(InputValue::Choice(usize::from(gamma))),
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
        if cache
            .exposure
            .as_ref()
            .is_some_and(|snapshot| !snapshot.rows.is_empty())
            || cache.supplied.is_some()
            || self.gamma.get()
        {
            measures.push("Gamma exposure".into());
        }
        if !measures.is_empty() {
            let selected = if gamma {
                measures.len() - 1
            } else {
                usize::from(
                    self.open_interest.get() && cache.volume_available && cache.interest_available,
                )
            };
            let label = measures[selected].clone();
            inputs.push(
                StudyInput::new(
                    "measure",
                    "Measure",
                    InputValue::Choice(selected),
                    InputKind::Choice(measures),
                )
                .with_default(InputValue::Choice(0)),
            );
            inputs.push(
                StudyInput::new(
                    "measure_label",
                    "Measure label",
                    InputValue::Text(label),
                    InputKind::Text,
                )
                .shown_when("measure", Vec::new()),
            );
        }
        if gamma {
            for (index, id, label) in [
                (0, "call_wall", "Call wall"),
                (1, "put_wall", "Put wall"),
                (2, "zero_gamma", "Zero gamma (provider)"),
            ] {
                if index < 2 || cache.supplied.is_some() {
                    inputs.push(
                        StudyInput::new(
                            id,
                            label,
                            InputValue::Toggle(self.exposure_levels[index].get()),
                            InputKind::Toggle,
                        )
                        .with_default(InputValue::Toggle(false)),
                    );
                }
            }
        }
        if cache.pain_available && !gamma {
            inputs.push(
                StudyInput::new(
                    "max_pain",
                    "Max pain",
                    InputValue::Toggle(self.max_pain.get()),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(false)),
            );
        }
        let waiting = self.waiting_inputs.borrow();
        inputs.retain(|input| !waiting.iter().any(|(id, _)| *id == input.id));
        inputs.extend(
            waiting
                .iter()
                .filter_map(|(id, value)| waiting_input(id, value.clone())),
        );
        if let Some(label) = inputs.iter().position(|input| input.id == "measure_label") {
            let label = inputs.remove(label);
            let position = inputs
                .iter()
                .position(|input| input.id == "measure")
                .map_or(inputs.len(), |position| position + 1);
            inputs.insert(position, label);
        }
        inputs.extend([
            StudyInput::new(
                "expected_move",
                "Expected move",
                InputValue::Toggle(self.expected_move),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(false)),
            StudyInput::new(
                "move_method",
                "Expected move method",
                InputValue::Choice(usize::from(self.move_iv)),
                InputKind::Choice(vec!["ATM straddle".into(), "Implied volatility".into()]),
            )
            .with_default(InputValue::Choice(0)),
        ]);
        inputs
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), StudyError> {
        let value = match (id, value, &self.source) {
            ("expiry", InputValue::Choice(choice), ProfileSource::Chain(_, dates)) => {
                InputValue::Choice(choice.min(dates.len() + 1))
            }
            (_, value, _) => value,
        };
        let input = self.inputs().into_iter().find(|input| input.id == id);
        let chain_input = matches!(self.source, ProfileSource::Chain(..))
            && waiting_input(id, value.clone()).is_some_and(|input| input.accepts(&value));
        let unavailable_label = chain_input
            && id == "measure_label"
            && !input
                .as_ref()
                .is_some_and(|input| self.set_chain_input(id, &value, input));
        let waiting = self.waiting_inputs.get_mut();
        if chain_input {
            let superseded = if id == "measure_label" {
                "measure"
            } else if id == "measure" {
                "measure_label"
            } else {
                ""
            };
            waiting.retain(|(saved, _)| saved != superseded);
        }
        let held = waiting.iter().position(|(saved, _)| saved == id);
        if chain_input
            && (held.is_some()
                || unavailable_label
                || !input.as_ref().is_some_and(|input| input.accepts(&value)))
        {
            if id == "measure_label" {
                self.open_interest.set(false);
                self.gamma.set(false);
                *self.cache.get_mut() = CachedProfile::default();
            }
            match held {
                Some(held) => waiting[held].1 = value,
                None => waiting.push((id.to_owned(), value)),
            }
            return Ok(());
        }
        let input = input.ok_or_else(|| StudyError::UnknownInput(id.into()))?;
        if !input.accepts(&value) {
            return Err(StudyError::invalid_input(id));
        }
        match (id, value) {
            (id, value) if self.set_chain_input(id, &value, &input) => {}
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
            ("expiry_label", InputValue::Text(label)) => {
                let ProfileSource::Chain(_, dates) = &self.source else {
                    unreachable!()
                };
                self.expiries = if label == "Through date" {
                    Expiries::Through
                } else if let Ok(date) = NaiveDate::parse_from_str(&label, "%Y-%m-%d")
                    && dates.binary_search(&date).is_ok()
                {
                    Expiries::One(date)
                } else {
                    Expiries::Nearest
                };
            }
            ("through_date", InputValue::Text(date)) => {
                self.through_date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                    .map_err(|_| StudyError::invalid_input(id))?;
            }
            ("sides", InputValue::Choice(choice)) => {
                self.gamma_split = choice == 0;
                self.config.display = if choice == 0 {
                    ProfileDisplay::UpDown
                } else {
                    ProfileDisplay::Delta
                }
            }
            ("expected_move", InputValue::Toggle(enabled)) => self.expected_move = enabled,
            ("move_method", InputValue::Choice(choice)) => self.move_iv = choice == 1,
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
    use crate::data::{
        HistoryPage, HistoryRequest, OptionChainQuery, OptionChainSink, OptionExposureQuery,
        OptionExposureSink, OptionsVolumeSink, Reply,
    };
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

    #[derive(Default)]
    struct ChainFeed(
        Mutex<Option<OptionChainSink>>,
        Mutex<Option<OptionExposureSink>>,
    );

    impl Datafeed for ChainFeed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }
        fn option_chain(&self, _: OptionChainQuery, sink: OptionChainSink) {
            *self.0.lock().unwrap() = Some(sink);
        }
        fn option_exposure(&self, _: OptionExposureQuery, sink: OptionExposureSink) {
            *self.1.lock().unwrap() = Some(sink);
        }
    }

    /// ATM ties use the lower listed strike even when its quotes are incomplete. Neither
    /// a different strike, last trade nor another expiry may fill a missing input.
    #[test]
    fn expected_move_tracks_expiry_method_and_snapshot_replacement() {
        let expiry = NaiveDate::from_ymd_opt(2024, 4, 1).unwrap();
        let next = expiry + chrono::TimeDelta::days(4);
        let end = expiry
            .succ_opt()
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let time = end - chrono::TimeDelta::hours(2190); // 91.25 calendar days.
        let rows = |expiry, time| {
            [105.0, 95.0]
                .into_iter()
                .flat_map(|strike| {
                    [OptionKind::Call, OptionKind::Put].map(|kind| {
                        let mut row = OptionQuote::new(
                            format!("{expiry}-{strike}-{kind:?}"),
                            expiry,
                            strike,
                            kind,
                            time,
                        );
                        row.underlying_price = Some(100.0);
                        row.bid = Some(if kind == OptionKind::Call { 2.0 } else { 3.0 });
                        row.ask = row.bid.map(|bid| bid + 2.0);
                        row.last = Some(999.0);
                        row.implied_volatility =
                            Some(if kind == OptionKind::Call { 0.1 } else { 0.3 });
                        if strike == 105.0 {
                            row.ask = Some(99.0);
                            row.implied_volatility = Some(0.9);
                        }
                        row
                    })
                })
                .collect::<Vec<_>>()
        };
        let feed = ChainFeed::default();
        let mut query = OptionChainQuery::new("SAMPLE", expiry..=next);
        query.as_of = Some(time);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let sink = feed.0.lock().unwrap().clone().unwrap();
        let mut quotes = rows(expiry, time);
        let mut later = rows(next, time);
        for row in &mut later {
            row.bid = Some(5.0);
            row.ask = Some(6.0);
        }
        quotes.extend(later);
        sink.push(quotes).unwrap();
        let mut study =
            OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry, next]).unwrap();
        let bars = vec![Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0); 2];
        study.calculate(&bars);
        assert_eq!(study.plot_value(6, 0), None);
        assert!(study.fills().is_empty());
        assert!(study.plot_label().is_none());
        study
            .set_input("expected_move", InputValue::Toggle(true))
            .unwrap();
        for (method, bounds, label) in [
            (0, [93.0, 107.0], "ATM straddle"),
            (1, [90.0, 110.0], "Implied volatility"),
        ] {
            study
                .set_input("move_method", InputValue::Choice(method))
                .unwrap();
            for (line, expected) in [6, 7].into_iter().zip(bounds) {
                assert!((study.plot_value(line, 0).unwrap() - expected).abs() < 1e-10);
                assert_eq!(
                    study.clone_box().plot_value(line, 1),
                    study.plot_value(line, 0)
                );
                assert!(study.plot_style(line).visible);
                assert!(study.legend_visible(line));
            }
            let (_, text) = study.plot_label().unwrap();
            assert!(text.contains(&expiry.to_string()) && text.contains(label));
            assert!(!study.fills().is_empty());
        }
        for (line, color) in [(6, Color32::YELLOW), (7, Color32::LIGHT_BLUE)] {
            study.set_color(line, color);
            assert_eq!(study.plot_style(line).color, color);
            assert_eq!(study.colors()[line], color);
        }
        #[cfg(feature = "widget")]
        for count in [1, 2] {
            let data = (0..count)
                .map(|day| {
                    Bar::new(
                        time + chrono::TimeDelta::days(day),
                        100.0,
                        101.0,
                        99.0,
                        100.0,
                        1.0,
                    )
                })
                .collect();
            let mut chart = crate::widget::Chart::new(crate::model::BarData::from_bars(data));
            chart.set_symbol("SAMPLE");
            chart.set_timeframe(crate::model::Timeframe::Day1);
            chart.add_indicator(study.clone_box());
            let fill_color = study.fills()[0].color;
            let ctx = egui::Context::default();
            let mut painted = false;
            for _ in 0..2 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                    },
                );
                painted |= output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Mesh(mesh)
                        if mesh.vertices.len() >= 4
                            && mesh.vertices.iter().all(|vertex| vertex.color == fill_color)
                            && mesh.calc_bounds().left() <= chart.get_rendered_price_rect().left()
                            && mesh.calc_bounds().right() >= chart.get_rendered_price_rect().right())
                });
                output.drop_without_applying_deltas();
            }
            assert!(
                painted,
                "expected move does not fill the pane with {count} underlying bars"
            );
        }
        study
            .set_input("move_method", InputValue::Choice(0))
            .unwrap();
        for selection in [2, 3] {
            // One later expiry or all expiries through its date.
            study
                .set_input("expiry", InputValue::Choice(selection))
                .unwrap();
            assert_eq!(study.plot_value(6, 0), Some(89.0));
            assert_eq!(study.plot_value(7, 0), Some(111.0));
            assert!(study.plot_label().unwrap().1.contains(&next.to_string()));
        }

        for (as_of, present) in [
            (end - chrono::TimeDelta::milliseconds(1), true),
            (end, false),
            (end + chrono::TimeDelta::milliseconds(1), false),
        ] {
            let mut query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
            query.as_of = Some(as_of);
            let chain = OptionChain::new(&feed, query, || {}).unwrap();
            feed.0
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .push(rows(expiry, as_of))
                .unwrap();
            let mut study =
                OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
            study.calculate(&bars);
            study.set_input("expiry", InputValue::Choice(1)).unwrap();
            study
                .set_input("expected_move", InputValue::Toggle(true))
                .unwrap();
            assert_eq!(study.plot_value(6, 0).is_some(), present, "{as_of}");
            assert_eq!(study.plot_label().is_some(), present, "{as_of}");
        }

        // Live snapshots can change quotes without appending an underlying bar.
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let chain = OptionChain::new(
            &feed,
            OptionChainQuery::new("SAMPLE", expiry..=expiry),
            || {},
        )
        .unwrap();
        let sink = feed.0.lock().unwrap().clone().unwrap();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        study.calculate(&bars);
        study
            .set_input("expected_move", InputValue::Toggle(true))
            .unwrap();
        for case in 0..10 {
            let mut quotes = rows(expiry, Utc::now());
            match case {
                1 => quotes[2].bid = None,
                2 => quotes[3].ask = None,
                3 => quotes[2].bid = Some(9.0), // Crossed market.
                4 => quotes
                    .iter_mut()
                    .for_each(|row| row.underlying_price = None),
                5 => quotes.retain(|row| row.strike != 95.0 || row.kind != OptionKind::Put),
                6 => quotes
                    .iter_mut()
                    .for_each(|row| row.implied_volatility = None),
                7 => {
                    quotes[2].implied_volatility = None; // One ATM IV remains usable.
                    for row in &mut quotes {
                        row.bid = None;
                        row.ask = None;
                    }
                }
                8 => quotes.clear(),
                9 => quotes.iter_mut().for_each(|row| {
                    row.underlying_price = Some(200.0);
                    row.bid = Some(5.0);
                    row.ask = Some(6.0);
                }),
                _ => {}
            }
            sink.push(quotes).unwrap();
            if case == 6 || case == 8 {
                study
                    .set_input("move_method", InputValue::Choice(usize::from(case == 6)))
                    .unwrap();
            }
            let present = matches!(case, 0 | 4 | 7 | 9);
            assert_eq!(study.plot_value(6, 0).is_some(), present, "case {case}");
            assert_eq!(study.plot_value(7, 1).is_some(), present, "case {case}");
            assert_eq!(study.plot_label().is_some(), present, "case {case}");
        }
        assert_eq!(study.plot_value(6, 0), Some(189.0));
        assert_eq!(study.plot_value(7, 1), Some(211.0));
        let mut quotes = rows(expiry, Utc::now());
        quotes
            .iter_mut()
            .for_each(|row| row.underlying_price = None);
        sink.push(quotes).unwrap();
        assert_eq!(study.plot_value(6, 0), Some(93.0));
        let mut changed = bars.clone();
        changed[1].close = 99.0;
        study.calculate(&changed);
        assert_eq!(study.plot_value(6, 0), Some(92.0));
        assert_eq!(study.plot_value(7, 1), Some(106.0));
        changed.iter_mut().for_each(|bar| bar.close = f64::NAN);
        study.calculate(&changed);
        assert_eq!(study.plot_value(6, 0), None);
        assert!(study.plot_label().is_none());
        study.calculate(&bars);
        sink.push(rows(expiry, Utc::now())).unwrap();
        assert_eq!(study.plot_value(6, 0), Some(93.0));
        study.set_visible(false);
        assert_eq!(study.plot_value(6, 0), Some(93.0));
        assert!(study.volume_profiles(0..bars.len()).is_empty());
        assert!(study.fills().is_empty());
        assert!(study.plot_label().is_none());
        study.set_visible(true);
        study.set_instrument(Some(&Instrument::new("OTHER")));
        assert_eq!(study.plot_value(7, 0), None);
        assert!(study.plot_label().is_none());
    }

    /// The four contracts in the issue give +5/-6 net, and walls at 100/110; a later
    /// snapshot removes 110 without an underlying bar. Provider figures replace the model.
    #[test]
    fn gamma_profiles_replace_signed_strikes_and_walls_without_new_bars() {
        for supplied in [false, true] {
            let feed = ChainFeed::default();
            let expiry = Utc::now().date_naive();
            let next = expiry.succ_opt().unwrap();
            let time = Utc::now();
            let chain =
                OptionChain::new(&feed, OptionChainQuery::new("SAMPLE", expiry..=next), || {})
                    .unwrap();
            let sink = feed.0.lock().unwrap().clone().unwrap();
            let mut rows: Vec<_> = [
                (100.0, OptionKind::Call, 0.12),
                (100.0, OptionKind::Put, 0.07),
                (110.0, OptionKind::Call, 0.02),
                (110.0, OptionKind::Put, 0.08),
            ]
            .into_iter()
            .map(|(strike, kind, gamma)| {
                let mut row =
                    OptionQuote::new(format!("{strike}-{kind:?}"), expiry, strike, kind, time);
                // gamma * 1 contract * 1 unit * 100² * 0.01 = gamma * 100.
                row.gamma = Some(gamma);
                row.open_interest = Some(1.0);
                row.multiplier = Some(1.0);
                row.underlying_price = Some(100.0);
                row
            })
            .collect();
            sink.push(rows.clone()).unwrap();
            let mut study =
                OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry, next]).unwrap();
            if supplied {
                let handle = OptionExposureHandle::new(
                    &feed,
                    OptionExposureQuery::new("SAMPLE", expiry..=next, OptionExposureKind::Gamma),
                    || {},
                )
                .unwrap();
                study = study.with_exposure(handle).unwrap();
            }
            let input = study
                .inputs()
                .into_iter()
                .find(|input| input.id == "measure")
                .unwrap();
            let InputKind::Choice(choices) = input.kind else {
                panic!("measure choices")
            };
            let gamma = choices
                .iter()
                .position(|name| name == "Gamma exposure")
                .expect("chain gamma is offered");
            study
                .set_input("measure", InputValue::Choice(gamma))
                .unwrap();
            study.calculate(&[Bar::new(time, 105.0, 112.0, 98.0, 105.0, 1.0)]);
            assert!(!study.legend_visible(3));
            assert_eq!(study.plot_value(3, 0), None);
            assert_eq!(study.plot_value(4, 0), None);
            for id in ["call_wall", "put_wall"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            for replace in [false, true] {
                if replace {
                    rows.truncate(2);
                }
                if !supplied || !replace {
                    sink.push(rows.clone()).unwrap();
                }
                if supplied {
                    let exposure_rows = if replace {
                        vec![(100.0, 12.0, -7.0, 5.0)]
                    } else {
                        vec![(100.0, 12.0, -7.0, 5.0), (110.0, 2.0, -8.0, -6.0)]
                    };
                    let mut snapshot = OptionExposure::new(
                        OptionExposureKind::Gamma,
                        time + chrono::TimeDelta::minutes(i64::from(replace)),
                        "host units",
                        exposure_rows
                            .into_iter()
                            .map(|(strike, calls, puts, net)| {
                                let mut row = OptionExposureRow::new(expiry, strike);
                                row.calls = Some(calls);
                                row.puts = Some(puts);
                                row.net = Some(net);
                                row
                            })
                            .collect(),
                    );
                    snapshot.model = Some("Positions".into());
                    snapshot.zero_gamma = (!replace).then_some(105.0);
                    feed.1
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .push(snapshot)
                        .unwrap();
                    if !replace {
                        study
                            .set_input("zero_gamma", InputValue::Toggle(true))
                            .unwrap();
                        // Updating the chain alone must never add to supplied exposure.
                        let mut changed = rows.clone();
                        changed.iter_mut().for_each(|row| row.gamma = Some(999.0));
                        sink.push(changed).unwrap();
                    }
                }
                let clone = study.clone_box();
                for consumer in [&study as &dyn Indicator, &*clone] {
                    let plot = consumer.volume_profiles(0..1).remove(0);
                    assert_eq!(plot.config.display, ProfileDisplay::Delta);
                    let exposure = &plot.profile.exposure.as_ref().unwrap().snapshot;
                    assert_eq!(exposure.rows.len(), if replace { 1 } else { 2 });
                    for (row, expected) in exposure.rows.iter().zip([5.0, -6.0]) {
                        assert!((row.net.unwrap() - expected).abs() < 1e-10);
                    }
                    assert_eq!(consumer.plot_value(3, 0), Some(100.0));
                    assert_eq!(
                        consumer.plot_value(4, 0),
                        Some(if replace { 100.0 } else { 110.0 })
                    );
                    assert_eq!(
                        consumer.plot_value(5, 0),
                        (supplied && !replace).then_some(105.0)
                    );
                    let note = &consumer.line_names()[2];
                    assert!(note.contains("UTC"));
                    assert!(note.contains(if supplied {
                        "host units; Provider signs"
                    } else {
                        "price currency per 1% move; Calls + / puts - (convention)"
                    }));
                    if supplied {
                        assert!(note.contains("Positions"));
                    }
                    assert_eq!(
                        exposure.time,
                        time + chrono::TimeDelta::minutes(i64::from(supplied && replace))
                    );
                }
                #[cfg(feature = "widget")]
                if !replace {
                    for (split, expected) in [
                        (false, vec![75.0, 90.0]),
                        (true, vec![90.0, 52.5, 15.0, 60.0]),
                    ] {
                        study
                            .set_input("sides", InputValue::Choice(usize::from(!split)))
                            .unwrap();
                        let plot = study.volume_profiles(0..1).remove(0);
                        for direction in [-10.0, -0.1, 0.1, 10.0] {
                            let ctx = egui::Context::default();
                            let mut output = ctx.run_ui(Default::default(), |ui| {
                                crate::painting::volume_profile::paint_profile(
                                    ui.painter(),
                                    &plot.profile,
                                    plot.config,
                                    plot.color,
                                    100.0..300.0,
                                    crate::painting::volume_profile::ProfileStyle::Study {
                                        right_aligned: true,
                                        width: 0.9,
                                    },
                                    &|price| 300.0 + direction * (price - 105.0) as f32,
                                );
                            });
                            let shapes = std::mem::take(&mut output.shapes);
                            output.drop_without_applying_deltas();
                            let rects: Vec<_> = shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Rect(rect) => Some(rect.rect),
                                    _ => None,
                                })
                                .collect();
                            assert_eq!(rects.len(), expected.len());
                            for (index, (rect, width)) in rects.iter().zip(&expected).enumerate() {
                                assert!((rect.width() - width).abs() < 1e-4);
                                assert!(rect.height() > 0.0);
                                let negative = if split { index % 2 == 1 } else { index == 1 };
                                assert!(
                                    (if negative { rect.right() } else { rect.left() } - 200.0)
                                        .abs()
                                        < 1e-4
                                );
                            }
                        }
                    }
                    study.set_input("sides", InputValue::Choice(1)).unwrap();
                    assert_eq!(
                        study.volume_profiles(0..1)[0]
                            .profile
                            .exposure
                            .as_ref()
                            .unwrap()
                            .snapshot
                            .rows
                            .len(),
                        2
                    );
                }
            }
            if supplied {
                let mut row = OptionExposureRow::new(expiry, 100.0);
                row.calls = Some(-4.0);
                row.puts = Some(2.0);
                row.net = Some(13.0);
                let mut snapshot =
                    OptionExposure::new(OptionExposureKind::Gamma, time, "host units", vec![row]);
                let exposure_sink = feed.1.lock().unwrap().clone().unwrap();
                snapshot.zero_gamma = Some(0.0);
                exposure_sink.push(snapshot.clone()).unwrap();
                assert_eq!(
                    study.volume_profiles(0..1)[0]
                        .profile
                        .exposure
                        .as_ref()
                        .unwrap()
                        .snapshot
                        .rows[0]
                        .net,
                    Some(13.0)
                );
                assert_eq!(study.plot_value(5, 0), Some(0.0));
                let mut unknown_call = OptionExposureRow::new(next, 100.0);
                unknown_call.puts = Some(-2.0);
                unknown_call.net = Some(-2.0);
                snapshot.rows.push(unknown_call);
                exposure_sink.push(snapshot.clone()).unwrap();
                study.set_input("expiry", InputValue::Choice(3)).unwrap();
                let plot = study.volume_profiles(0..1).remove(0);
                let strike = &plot.profile.exposure.as_ref().unwrap().rows[0];
                assert_eq!(strike.calls, None, "provider unknown stays unknown");
                assert_eq!(strike.puts, Some(0.0));
                assert_eq!(strike.net, Some(11.0));
                snapshot.rows.clear();
                snapshot.zero_gamma = None;
                exposure_sink.push(snapshot).unwrap();
            } else {
                for (separate_sides, calls, puts, net, later_net) in [
                    (false, 24.0, -14.0, 10.0, 5.0),
                    (true, 12.0, -7.0, 5.0, -7.0),
                ] {
                    let mut later = rows.clone();
                    later.iter_mut().for_each(|row| row.expiry = next);
                    let mut selected = rows.clone();
                    if separate_sides {
                        selected.truncate(1);
                        selected.push(later[1].clone());
                    } else {
                        selected.extend(later);
                    }
                    sink.push(selected).unwrap();
                    study.set_input("expiry", InputValue::Choice(3)).unwrap();
                    let plot = study.volume_profiles(0..1).remove(0);
                    assert_eq!(
                        plot.profile.exposure.as_ref().unwrap().snapshot.rows.len(),
                        2
                    );
                    let strikes = &plot.profile.exposure.as_ref().unwrap().rows;
                    assert_eq!(strikes.len(), 1);
                    for (actual, expected) in [
                        (strikes[0].calls, calls),
                        (strikes[0].puts, puts),
                        (strikes[0].net, net),
                    ] {
                        assert!(
                            (actual.expect("known computed contribution") - expected).abs() < 1e-10
                        );
                    }
                    study.set_input("expiry", InputValue::Choice(2)).unwrap();
                    assert!((study.plot_value(2, 0).unwrap() - later_net).abs() < 1e-10);
                }
                sink.push(Vec::new()).unwrap();
            }
            assert!(study.volume_profiles(0..1).is_empty());
            assert_eq!(study.plot_value(3, 0), None);
            assert_eq!(study.plot_value(4, 0), None);
            assert_eq!(study.plot_value(5, 0), None);
        }
    }

    #[test]
    fn walls_need_exposure_on_their_side() {
        let (chain, sink, _, expiry, bars) = expected_move_chain();
        let time = bars[0].time;
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        study.calculate(&bars);
        for (calls, puts, walls) in [
            ([0.0, 0.0], [0.0, 0.0], [None, None]),
            ([1.0, 3.0], [0.0, 0.0], [Some(110.0), None]),
            ([0.0, 0.0], [3.0, 1.0], [None, Some(100.0)]),
            ([1.0, 1.0], [1.0, 1.0], [Some(100.0), Some(100.0)]),
        ] {
            let rows = [100.0, 110.0]
                .into_iter()
                .enumerate()
                .flat_map(|(index, strike)| {
                    [OptionKind::Call, OptionKind::Put].map(move |kind| {
                        let mut row = OptionQuote::new(
                            format!("{strike}-{kind:?}"),
                            expiry,
                            strike,
                            kind,
                            time,
                        );
                        row.gamma = Some(0.05);
                        row.multiplier = Some(100.0);
                        row.underlying_price = Some(105.0);
                        // One contract gives 0.05 * 100 * 105² * 0.01 = 551.25.
                        row.open_interest = Some(if kind == OptionKind::Call {
                            calls[index]
                        } else {
                            puts[index]
                        });
                        row
                    })
                })
                .collect();
            sink.push(rows).unwrap();
            for id in ["call_wall", "put_wall"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            assert_eq!([study.plot_value(3, 0), study.plot_value(4, 0)], walls);
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

    /// The chain, quotes, expiry and two underlying bars of
    /// `expected_move_tracks_expiry_method_and_snapshot_replacement`, for one expiry. The ATM
    /// straddle at 95 costs a call mid of 3 and a put mid of 4, so the expected move is 93..107.
    /// The quotes arrive when the caller pushes them to the returned sink.
    fn expected_move_chain() -> (
        OptionChain,
        OptionChainSink,
        Vec<OptionQuote>,
        NaiveDate,
        Vec<Bar>,
    ) {
        let expiry = NaiveDate::from_ymd_opt(2024, 4, 1).unwrap();
        let end = expiry
            .succ_opt()
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let time = end - chrono::TimeDelta::hours(2190);
        let rows: Vec<_> = [105.0, 95.0]
            .into_iter()
            .flat_map(|strike| {
                [OptionKind::Call, OptionKind::Put].map(|kind| {
                    let mut row = OptionQuote::new(
                        format!("{expiry}-{strike}-{kind:?}"),
                        expiry,
                        strike,
                        kind,
                        time,
                    );
                    row.underlying_price = Some(100.0);
                    row.bid = Some(if kind == OptionKind::Call { 2.0 } else { 3.0 });
                    row.ask = row.bid.map(|bid| bid + 2.0);
                    row.last = Some(999.0);
                    row.implied_volatility = Some(if kind == OptionKind::Call { 0.1 } else { 0.3 });
                    if strike == 105.0 {
                        row.ask = Some(99.0);
                        row.implied_volatility = Some(0.9);
                    }
                    row
                })
            })
            .collect();
        let feed = ChainFeed::default();
        let mut query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
        query.as_of = Some(time);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let sink = feed.0.lock().unwrap().clone().unwrap();
        let bars = vec![Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0); 2];
        (chain, sink, rows, expiry, bars)
    }

    /// A latest-value line on an option level sits at the value its plot and scale tag show.
    #[cfg(feature = "widget")]
    #[test]
    fn option_levels_draw_their_latest_value_lines() {
        use crate::scales::{PriceMap, PriceRange, PriceTransform};
        let (chain, sink, rows, expiry, bars) = expected_move_chain();
        sink.push(rows).unwrap();
        let mut profile = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        profile
            .set_input("expected_move", InputValue::Toggle(true))
            .unwrap();
        let mut study = crate::studies::ConfiguredStudy::new(Box::new(profile));
        let mut settings = study.settings();
        settings.price_line_plots = vec![6];
        study.apply_settings(&[], settings).unwrap();
        study.calculate(&bars);
        // The expected low: 100 less the 95 straddle of 3 + 4.
        assert!((study.plot_value(6, 1).unwrap() - 93.0).abs() < 1e-10);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 120.0));
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(80.0, 120.0),
            0.0,
            120.0,
            false,
        );
        let coords = crate::chart::coords::ChartMapping::new(rect, 30.0, 0, 1, 0.0, map);
        let color = study.plot_style(6).color;
        let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            crate::chart::rendering::IndicatorRenderer::render_study(
                &crate::chart::rendering::RenderContext::new(ui.painter(), rect),
                &study,
                &bars,
                0,
                &map,
                &coords,
            );
        });
        let shapes = std::mem::take(&mut output.shapes);
        output.drop_without_applying_deltas();
        // One mesh of one-pixel dots across the plot at 120 - (93 - 80) / 40 * 120 = 81.
        let lines: Vec<f32> = shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh)
                    if !mesh.vertices.is_empty()
                        && mesh.vertices.iter().all(|vertex| vertex.color == color) =>
                {
                    let bounds = mesh.calc_bounds();
                    (bounds.height() <= 1.0 && bounds.width() >= 200.0).then_some(bounds.center().y)
                }
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 1, "latest-value lines at {lines:?}");
        assert!((lines[0] - 81.0).abs() <= 0.5, "{lines:?}");
    }

    /// A calculated profile over `chain` with the expected move on and its band hidden, saved.
    fn saved_hidden_band(
        chain: OptionChain,
        expiry: NaiveDate,
        bars: &[Bar],
    ) -> crate::studies::StudyTemplate {
        let mut study = crate::studies::ConfiguredStudy::new(Box::new(
            OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap(),
        ));
        study.calculate(bars);
        let settings = study.settings();
        study
            .apply_settings(
                &[("expected_move".into(), InputValue::Toggle(true))],
                settings,
            )
            .unwrap();
        assert_eq!(study.fills().len(), 1);
        let mut settings = study.settings();
        settings.fills[0].visible = false;
        study.apply_settings(&[], settings).unwrap();
        assert!(study.fills().is_empty());
        crate::studies::StudyTemplate::from_study(&study)
    }

    /// A factory that builds the profile over `chain`, as the indicator dialog registers it.
    fn profile_factory(chain: OptionChain, expiry: NaiveDate) -> crate::studies::IndicatorFactory {
        let mut factory = crate::studies::IndicatorFactory::new();
        factory.register("Options Volume Profile", move || {
            Box::new(
                OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", vec![expiry]).unwrap(),
            )
        });
        factory
    }

    /// A band hidden in a saved template stays hidden when the study is rebuilt before it knows
    /// its band, and when its settings are applied again. Quotes without an underlying price
    /// wait for the last close.
    #[test]
    fn saved_band_style_survives_a_template() {
        let (chain, sink, mut rows, expiry, bars) = expected_move_chain();
        for row in &mut rows {
            row.underlying_price = None;
            row.implied_volatility = None;
        }
        sink.push(rows).unwrap();
        let template = saved_hidden_band(chain.clone(), expiry, &bars);
        let mut loaded = template
            .create_with(&profile_factory(chain, expiry))
            .unwrap();
        loaded.calculate(&bars);
        // The last close of 100 stands in for the underlying, so the move is 93..107 again.
        assert!((loaded.plot_value(6, 1).unwrap() - 93.0).abs() < 1e-10);
        assert!(
            loaded.fills().is_empty(),
            "saved band styles {:?}, restored {:?}",
            template.settings.fills,
            loaded.settings().fills
        );
        // The settings dialog applies every input, and the chart's own inputs recalculate the
        // study without bars, so it declares no band until the chart calculates it again.
        let inputs: Vec<_> = loaded
            .inputs()
            .into_iter()
            .map(|input| (input.id, input.value))
            .collect();
        let settings = loaded.settings();
        loaded.apply_settings(&inputs, settings).unwrap();
        loaded.calculate(&bars);
        assert!(loaded.fills().is_empty(), "{:?}", loaded.settings().fills);
    }

    /// A profile saved with chain-dependent inputs restores before its quotes arrive, as a layout
    /// does at start-up, and applies them once they do.
    #[test]
    fn saved_profile_restores_before_its_quotes_arrive() {
        let (chain, sink, rows, expiry, bars) = expected_move_chain();
        sink.push(rows).unwrap();
        let mut template = saved_hidden_band(chain, expiry, &bars);
        let wall = template.inputs.iter_mut().find(|(id, _)| id == "call_wall");
        wall.unwrap().1 = InputValue::Toggle(true);
        let (later, later_sink, later_rows, _, _) = expected_move_chain();
        let mut loaded = template
            .create_with(&profile_factory(later, expiry))
            .unwrap_or_else(|error| panic!("restore failed: {error:?}"));
        // Saved again before the quotes arrive, it holds what it was saved with.
        assert_eq!(
            crate::studies::StudyTemplate::from_study(loaded.as_ref()).inputs,
            template.inputs
        );
        later_sink.push(later_rows).unwrap();
        loaded.calculate(&bars);
        assert!((loaded.plot_value(6, 1).unwrap() - 93.0).abs() < 1e-10);
        // The saved inputs now apply, so the chain lists them with their saved values.
        assert_eq!(
            crate::studies::StudyTemplate::from_study(loaded.as_ref()).inputs,
            template.inputs
        );
        assert!(loaded.fills().is_empty());

        // Quotes with volume offer no walls. Keep the saved wall until gamma is selected.
        let (later, later_sink, mut later_rows, _, _) = expected_move_chain();
        let mut loaded = template
            .create_with(&profile_factory(later, expiry))
            .unwrap();
        for row in &mut later_rows {
            row.volume = Some(1.0);
        }
        later_sink.push(later_rows).unwrap();
        assert!(loaded.inputs().iter().any(|input| input.id == "call_wall"));
        loaded
            .set_input("call_wall", InputValue::Toggle(true))
            .unwrap();
        // An id it can never offer fails even before quotes arrive.
        let (later, _later_sink, _, _, _) = expected_move_chain();
        let mut waiting = OptionsVolumeProfile::from_chain(later, "SAMPLE", vec![expiry]).unwrap();
        assert!(matches!(
            waiting.set_input("unheard_of", InputValue::Toggle(true)),
            Err(StudyError::UnknownInput(_))
        ));
    }
    /// A hidden profile keeps the values that cursor payloads with hidden studies, and alerts on
    /// its outputs, read.
    #[cfg(all(feature = "widget", feature = "drawings"))]
    #[test]
    fn hidden_profile_keeps_its_values_for_cursor_payloads_and_alerts() {
        use crate::model::{AlertSource, LogicalRange};
        use crate::widget::test_frames::{chart, frame};
        use crate::widget::{Chart, ChartEvent, CursorDataPolicy};

        let (chain, sink, rows, expiry, _) = expected_move_chain();
        sink.push(rows).unwrap();
        let mut profile = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        profile
            .set_input("expected_move", InputValue::Toggle(true))
            .unwrap();
        let ctx = egui::Context::default();
        let mut chart = chart(&[99.0], &[101.0]);
        chart.config.show_volume = false;
        let id = chart.add_indicator(Box::new(profile));
        chart.set_cursor_data_policy(CursorDataPolicy::IncludeHidden);
        for time in [0.0, 0.1, 0.2] {
            frame(&ctx, &mut chart, time, vec![]);
        }
        chart.set_visible_logical_range(LogicalRange::new(40.0, 80.0));
        frame(&ctx, &mut chart, 0.3, vec![]);
        chart.drain_events().for_each(drop);
        // The expected low and high in the payload of a pointer moved onto bar `index`.
        let band = |chart: &mut Chart, index: usize, time: f64| {
            let pos = egui::Pos2::new(
                chart.get_chart_mapping().idx_to_x(index),
                chart.last_rendered_price_rect.center().y,
            );
            frame(&ctx, chart, time, vec![egui::Event::PointerMoved(pos)]);
            let pointer = chart
                .drain_events()
                .filter_map(|event| match event {
                    ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                    _ => None,
                })
                .last()
                .expect("a cursor payload");
            pointer
                .studies
                .iter()
                .filter(|(study, row)| *study == id && row.plot.starts_with("Expected"))
                .map(|(_, row)| (row.plot.clone(), row.value))
                .collect::<Vec<_>>()
        };
        // Strikes 95 and 105 are both 5 from the underlying at 100, so the lower one is at the
        // money: its call mid is (2 + 4) / 2 = 3 and its put mid (3 + 5) / 2 = 4, a move of 7.
        let expected = vec![
            ("Expected low".to_owned(), Some(93.0)),
            ("Expected high".to_owned(), Some(107.0)),
        ];
        assert_eq!(band(&mut chart, 60, 1.0), expected, "visible profile");
        assert!(chart.set_indicator_visible(id, false));
        frame(&ctx, &mut chart, 1.5, vec![]);
        chart.drain_events().for_each(drop);
        let hidden = band(&mut chart, 61, 2.0);
        let output = AlertSource::Study {
            id: id.0.to_string(),
            output: 6,
        };
        let alert = chart.alert_value(&output, 61);
        assert_eq!((hidden, alert), (expected, Some(93.0)), "hidden profile");
    }
    #[test]
    fn saved_expiry_choice_survives_a_rolled_listing() {
        let dates = |days: &[u32]| {
            days.iter()
                .map(|&day| NaiveDate::from_ymd_opt(2099, 11, day).unwrap())
                .collect::<Vec<_>>()
        };
        let make = |dates: Vec<NaiveDate>| {
            let feed = ChainFeed::default();
            let chain = OptionChain::new(
                &feed,
                OptionChainQuery::new("SAMPLE", dates[0]..=*dates.last().unwrap()),
                || {},
            )
            .unwrap();
            let rows = dates
                .iter()
                .map(|&expiry| {
                    let mut row = OptionQuote::new(
                        expiry.to_string(),
                        expiry,
                        100.0,
                        OptionKind::Call,
                        Utc::now(),
                    );
                    row.volume = Some(expiry.format("%d").to_string().parse().unwrap());
                    row
                })
                .collect();
            feed.0.lock().unwrap().as_ref().unwrap().push(rows).unwrap();
            let mut factory = crate::studies::IndicatorFactory::new();
            factory.register("Options Volume Profile", move || {
                Box::new(
                    OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", dates.clone())
                        .unwrap(),
                )
            });
            factory
        };
        let original = make(dates(&[6, 13, 20]));
        for (choice, listed, labelled, expected, selected) in [
            (3, vec![13, 20, 27], true, 20.0, 2),
            (4, vec![13, 20, 27], true, 33.0, 4),
            (4, vec![13, 20], true, 33.0, 3),
            (3, vec![13, 27], true, 13.0, 0),
            (3, vec![13, 20, 27], false, 27.0, 3),
            (4, vec![13, 20], false, 33.0, 3),
        ] {
            let mut study = original.create("Options Volume Profile").unwrap();
            study
                .set_input("expiry", InputValue::Choice(choice))
                .unwrap();
            let mut saved = crate::studies::StudyTemplate::from_study(study.as_ref());
            if !labelled {
                saved.inputs.retain(|(id, _)| !id.ends_with("_label"));
            }
            let restored = saved.create_with(&make(dates(&listed))).unwrap();
            let plot = restored.volume_profiles(0..1).remove(0);
            assert_eq!(
                plot.profile.up(),
                [expected],
                "choice {choice}, {listed:?}, labelled {labelled}"
            );
            let inputs = restored.inputs();
            assert_eq!(
                inputs.iter().find(|i| i.id == "expiry").unwrap().value,
                InputValue::Choice(selected)
            );
            assert!(
                inputs
                    .iter()
                    .filter(|i| i.id.ends_with("_label"))
                    .all(|i| !i.is_shown(&inputs))
            );
        }
    }

    #[test]
    fn saved_measure_restores_by_name() {
        let (chain, sink, rows, expiry, bars) = expected_move_chain();
        let rows: Vec<_> = rows
            .into_iter()
            .take(2)
            .map(|mut row| {
                row.strike = 100.0;
                let call = row.kind == OptionKind::Call;
                row.volume = Some(if call { 10.0 } else { 5.0 });
                row.open_interest = Some(if call { 4.0 } else { 2.0 });
                row.gamma = Some(0.05);
                row.multiplier = Some(100.0);
                row
            })
            .collect();
        sink.push(rows.clone()).unwrap();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        study.set_input("measure", InputValue::Choice(1)).unwrap();
        study
            .set_input("max_pain", InputValue::Toggle(true))
            .unwrap();
        let saved = crate::studies::StudyTemplate::from_study(&study);
        for ready in [false, true] {
            for missing in [0, 1, 2] {
                let (later, later_sink, _, _, _) = expected_move_chain();
                let mut changed = rows.clone();
                changed[1].volume = None;
                if missing == 1 {
                    changed.iter_mut().for_each(|row| row.multiplier = None);
                } else if missing == 2 {
                    changed[1].volume = Some(5.0);
                    changed[1].open_interest = None;
                }
                if ready {
                    later_sink.push(changed.clone()).unwrap();
                }
                let factory = profile_factory(later, expiry);
                let mut restored = saved.create_with(&factory).unwrap();
                if !ready {
                    // Saving before the first response must retain the selected measure and level.
                    let again = crate::studies::StudyTemplate::from_study(restored.as_ref());
                    restored = again.create_with(&factory).unwrap();
                    later_sink.push(changed).unwrap();
                }
                restored.calculate(&bars);
                let plot = restored.volume_profiles(0..bars.len()).remove(0);
                assert_eq!(plot.profile.up(), [if missing == 2 { 10.0 } else { 4.0 }]);
                assert_eq!(plot.profile.down(), [if missing == 2 { 5.0 } else { 2.0 }]);
                assert_eq!(restored.plot_value(1, 0), (missing == 0).then_some(100.0));
                #[cfg(all(feature = "widget", feature = "drawings"))]
                if missing != 0 {
                    let mut chart =
                        crate::widget::Chart::new(crate::model::BarData::from_bars(bars.clone()));
                    chart.set_symbol("SAMPLE");
                    chart.add_indicator(restored.clone_box());
                    let snapshot = chart.snapshot_with(&factory).unwrap();
                    chart.restore_with(&snapshot, &factory).unwrap();
                    assert_eq!(chart.indicators().next().unwrap().1.plot_value(1, 0), None);
                }
                let again = crate::studies::StudyTemplate::from_study(restored.as_ref());
                restored = again.create_with(&factory).unwrap();
                restored.calculate(&bars);
                later_sink.push(rows.clone()).unwrap();
                assert_eq!(restored.plot_value(1, 0), Some(100.0));
                assert_eq!(restored.volume_profiles(0..1)[0].profile.up(), [4.0]);
            }
        }
    }
    #[test]
    fn historical_fallback_close_known_at_as_of() {
        let (chain, sink, mut rows, expiry, _) = expected_move_chain();
        rows.iter_mut().for_each(|row| row.underlying_price = None);
        sink.push(rows).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let bars: Vec<_> = [100.0, 110.0, 120.0]
            .into_iter()
            .enumerate()
            .map(|(day, price)| {
                Bar::new(
                    start + chrono::TimeDelta::days(day as i64),
                    price,
                    price + 1.0,
                    price - 1.0,
                    price,
                    1.0,
                )
            })
            .collect();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        study
            .set_input("expected_move", InputValue::Toggle(true))
            .unwrap();
        for (count, bounds) in [(3, [93.0, 107.0]), (2, [8.5, 211.5]), (1, [93.0, 107.0])] {
            // With a successor after the query instant, the middle bar's final close is unknown.
            // The last bar has no successor, so its close is eligible from its own opening time.
            study.calculate(&bars[..count]);
            assert_eq!(
                [study.plot_value(6, 0), study.plot_value(7, 0)],
                bounds.map(Some),
                "{count} bars"
            );
        }
    }
    #[test]
    fn options_volume_profile_has_help_article() {
        let feed = Feed::default();
        let expiry = NaiveDate::from_ymd_opt(2026, 12, 18).unwrap();
        let study = OptionsVolumeProfile::new(
            &feed,
            OptionsVolumeQuery {
                symbol: "SAMPLE".into(),
                expiries: expiry..=expiry,
            },
            4.0,
            || {},
        )
        .unwrap();
        assert_eq!(
            study.help_url(),
            Some(
                "https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.OptionsVolumeProfile.html"
            )
        );
    }
}

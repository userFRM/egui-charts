//! Host-supplied option volume, open interest and signed gamma exposure by strike.

use crate::data::{
    Datafeed, OptionChain, OptionExposure, OptionExposureHandle, OptionExposureKind,
    OptionExposureRow, OptionKind, OptionQuote, OptionsVolume, OptionsVolumeQuery,
    OptionsVolumeSource,
};
use crate::data::{
    ExposureProfile, OptionWalls, ProfileDisplay, StrikeExposure, StudyProfile, VolumeProfile,
    VolumeProfileConfig,
};
use crate::model::Bar;
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, PlotStyle, StudyError, StudyInput, palette,
};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use egui::Color32;
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
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
    // The last close the strike window was drawn around, while it limits the strikes.
    window_close: Option<f64>,
    // The underlying price supplied with the quotes that the strike window follows.
    underlying: Option<f64>,
    // At their expiration, the bar of each group of expirations and its profile, oldest first.
    groups: Vec<(usize, Option<Arc<VolumeProfile>>)>,
    // Set by each rebuild and taken when held inputs are retried.
    rebuilt: bool,
}

/// Every measure, in the order a chain that reports them all lists them.
const MEASURES: [&str; 3] = ["Volume", "Open interest", "Gamma exposure"];

/// The most listed strikes Strikes around price draws on each side of the price.
const MAX_STRIKES_AROUND: usize = 1_000;

/// How expirations drawn at their dates are grouped.
const GROUPINGS: [&str; 3] = ["Each expiration", "By week", "By month"];

/// The bars the profile of a lone expiration drawn at its date spans.
const LONE_EXPIRATION_BARS: usize = 20;

/// The Sides choices of volume and open interest.
const VOLUME_SIDES: [&str; 5] = [
    "Split",
    "Net (calls minus puts)",
    "Calls plus puts",
    "Calls plus puts by expiration",
    "Puts / calls",
];

/// The Sides input of each of [`MEASURES`], with its choices and its default: split bars for
/// volume and open interest, net bars for gamma exposure.
const SIDES: [(&str, &[&str], usize); 3] = [
    ("volume_sides", &VOLUME_SIDES, 0),
    ("interest_sides", &VOLUME_SIDES, 0),
    ("gamma_sides", &["Split", "Net (calls plus puts)"], 1),
];

const TEAL: Color32 = Color32::from_rgba_unmultiplied_const(38, 166, 154, 180);
const RED: Color32 = Color32::from_rgba_unmultiplied_const(239, 83, 80, 180);
const BLUE: Color32 = Color32::from_rgba_unmultiplied_const(41, 98, 255, 180);

/// The chain profile's bar colour inputs with their defaults, in the order of its `bar_colors`.
const BAR_COLORS: [(&str, &str, Color32); 5] = [
    ("call_color", "Calls colour", BLUE),
    ("put_color", "Puts colour", RED),
    ("net_positive_color", "Net positive colour", TEAL),
    ("net_negative_color", "Net negative colour", RED),
    ("total_color", "Calls plus puts colour", BLUE),
];

/// Colours of the expiries of a profile drawn by expiry, by the expiry's place in the listing,
/// so that each keeps its colour while nearer ones expire.
const EXPIRY_COLORS: [Color32; 6] = [
    palette::BLUE,
    palette::WARM_ORANGE,
    palette::PURPLE,
    palette::PINK,
    palette::TEAL,
    palette::BROWN,
];

/// How an input the chain decides on is listed while it waits for the chain, or `None` for any
/// other id. A measure that waits is listed among [`MEASURES`], whatever the chain then offers.
fn waiting_input(id: &str, value: InputValue) -> Option<StudyInput> {
    let (label, kind, default) = match id {
        "measure_label" if matches!(&value, InputValue::Text(label) if MEASURES.contains(&label.as_str())) =>
        {
            return Some(
                StudyInput::new(id, "Measure label", value, InputKind::Text)
                    .with_default(InputValue::Text("Volume".into()))
                    .shown_when("measure", Vec::new()),
            );
        }
        "measure" => (
            "Measure",
            InputKind::Choice(MEASURES.map(Into::into).to_vec()),
            InputValue::Choice(0),
        ),
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

#[derive(Clone, Debug)]
enum ProfileSource {
    Volume(OptionsVolumeSource),
    Chain(OptionChain, Vec<NaiveDate>),
}

#[derive(Clone, Copy, Debug, Default)]
enum Expiries {
    #[default]
    Nearest,
    One(NaiveDate),
    Through,
}

/// Call and put volume, open interest or signed gamma exposure by strike.
/// Hosts supply complete snapshots through the volume-only constructor or [`Self::from_chain`].
/// Clones share the subscription; dropping its last clone makes the feed's sink inactive.
#[derive(Clone, Debug)]
pub struct OptionsVolumeProfile {
    source: ProfileSource,
    expiries: Expiries,
    through_date: NaiveDate,
    // Under Through date, the first listed expiries summed that have not expired; 0 for all.
    max_expiries: usize,
    // Listed strikes drawn on each side of the price; 0 for all.
    strikes_around: usize,
    // Draw a profile beside the date of each upcoming expiration instead of one at the plot's edge.
    at_expiration: bool,
    // At their expiration, the upcoming expirations drawn, from the nearest.
    expirations: usize,
    // At their expiration, expirations apart or by week or month, as GROUPINGS lists them.
    grouping: usize,
    expiry_labels: bool,
    timeframe: Option<crate::model::Timeframe>,
    // The bar of each listed expiry at UTC midnight, extrapolated past the last bar.
    // History loaded before the first bar moves them.
    expiry_bars: Vec<(NaiveDate, usize)>,
    open_interest: Cell<bool>,
    gamma: Cell<bool>,
    // The Sides choice of each of MEASURES, as its input lists it.
    sides: [usize; 3],
    put_call: bool,
    // Write each row's figures beside it.
    show_values: bool,
    current_price: bool,
    // Split calls and puts, net positive and negative, then calls plus puts, as BAR_COLORS.
    bar_colors: [Color32; 5],
    left: bool,
    // The widest row's share of the plot's width, in percent.
    width_percent: f64,
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
    matching_symbol: bool,
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
    /// study when changing the underlying, expiry range or row size. A chart showing another
    /// symbol hides the profile until it shows the underlying again.
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
    /// let query = OptionsVolumeQuery::new("SAMPLE", expiry..=expiry);
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
            max_expiries: 0,
            strikes_around: 0,
            at_expiration: false,
            expirations: 10,
            grouping: 0,
            expiry_labels: true,
            timeframe: None,
            expiry_bars: Vec::new(),
            open_interest: Cell::new(false),
            gamma: Cell::new(false),
            sides: SIDES.map(|(.., default)| default),
            put_call: true,
            show_values: false,
            current_price: true,
            bar_colors: BAR_COLORS.map(|(.., color)| color),
            left: false,
            width_percent: 27.0,
            exposure: None,
            exposure_levels: Default::default(),
            max_pain: Cell::new(false),
            waiting_inputs: RefCell::default(),
            expected_move: false,
            move_iv: false,
            as_of: None,
            last_close: None,
            symbol,
            matching_symbol: true,
            config,
            cache: RefCell::new(CachedProfile::default()),
            values: IndicatorValues::new(),
            colors: vec![BLUE, RED],
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
    /// Gamma uses [`OptionExposure::gamma`]. Missing contracts are counted in the readout.
    /// Attach a provider observation with [`Self::with_exposure`]; its signs and net values
    /// take precedence, even if empty.
    /// Each measure keeps its own Sides choice: volume and open interest draw split call and
    /// put bars by default, or calls minus puts, calls plus puts, calls plus puts by
    /// expiration, each expiration a segment of its strike's row in a colour of its own, named
    /// with its date in a key, or puts divided by calls, which leaves a strike without calls
    /// empty. Gamma exposure draws net bars by default, or split bars. Call
    /// and put walls, off by default, mark the strikes with the largest call and the largest
    /// put figure of the measure in size, summed over the selected expiries, the lower strike
    /// on a tie; a side whose figures are all zero has none, and volume or open interest drawn in
    /// place of a saved measure that waits for its data has none. Split, net and calls plus puts
    /// bars take their own colour inputs, calls blue and puts red by default, and puts divided
    /// by calls the puts colour. Every row takes its display's colours at one opacity; the
    /// profile marks no point of control or value area. Values, off by default, writes the
    /// calls × puts figures of each volume or open interest row beside it, with at most two
    /// decimals and no trailing zeros, abbreviated with K, M or B from a thousand, or the row's
    /// ratio of puts to calls. The profile takes the right or the left 30% of the plot, its
    /// widest row from 1% to 30% of the plot's width, 27% by default. Layout can instead place
    /// each upcoming expiration beside its date on the time scale, on the chosen side,
    /// separately or grouped by week or month. Maximum expirations counts from the nearest, 10
    /// by default. Each group shares the strike window and row height, names its own walls
    /// beside their rows, past their bars and figures, and labels its latest date on the time
    /// scale while Expiration labels is on. Groups mapping to one bar are combined; without
    /// chart bars no dated profiles are drawn.
    /// Under Through date, Maximum expirations keeps the first N listed expiries on or after the
    /// current day; 0 keeps every listed expiry up to the date. Strikes around price draws the
    /// strikes at the underlying price the expected move uses and the N nearest below and above
    /// it, 10 by default and up to 1,000, every strike at 0 or while no price is known; the
    /// put/call ratio, the walls, net gamma and max pain use every selected strike. A dotted
    /// current price line, on by default, marks that price.
    /// New profiles show from the daily interval up, since daily option figures do not line up
    /// with intraday bars; their interval visibility settings can widen that.
    /// The put/call and net gamma readouts show in the legend, never on the price scale; the
    /// Put/call ratio input and their visibility in the style tab hide them.
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
    /// are listed and saved as they were and apply as their required data becomes available.
    /// Saved expiry and measure labels take precedence over their choice positions;
    /// an expiry no longer listed selects Nearest. Older saves keep position semantics: a
    /// measure position counts in the chain's own listing once the chain has answered and
    /// lists it, and otherwise in the order Volume, Open interest, Gamma exposure. A measure
    /// that waits is listed and saved at its place in that order, and a choice made meanwhile
    /// names its measure in it.
    /// New saves contain hidden `expiry_label` and `measure_label` inputs and the per-measure
    /// `volume_sides`, `interest_sides` and `gamma_sides`, which older releases do not accept;
    /// saves with the former `sides` input restore it for every measure. Editors applying all
    /// inputs must update the matching label on an explicit choice edit or reset, and preserve
    /// it on unrelated edits while a measure is unavailable.
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
            max_expiries: 0,
            strikes_around: 10,
            at_expiration: false,
            expirations: 10,
            grouping: 0,
            expiry_labels: true,
            timeframe: None,
            expiry_bars: Vec::new(),
            open_interest: Cell::new(false),
            gamma: Cell::new(false),
            sides: SIDES.map(|(.., default)| default),
            put_call: true,
            show_values: false,
            current_price: true,
            bar_colors: BAR_COLORS.map(|(.., color)| color),
            left: false,
            width_percent: 27.0,
            exposure: None,
            exposure_levels: Default::default(),
            max_pain: Cell::new(false),
            waiting_inputs: RefCell::default(),
            expected_move: false,
            move_iv: false,
            symbol,
            matching_symbol: true,
            config,
            cache: RefCell::new(CachedProfile::default()),
            values: IndicatorValues::new(),
            // Put/call, max pain, net gamma, call wall, put wall, zero gamma, the expected low
            // and high and the current price.
            colors: vec![
                TEAL,
                RED,
                TEAL,
                BLUE,
                RED,
                TEAL,
                TEAL.to_opaque(),
                TEAL.to_opaque(),
                palette::GRAY,
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

    /// Whether wall `side`, 0 for calls and 1 for puts, is drawn: its switch is on, and volume or
    /// open interest drawn in place of a saved measure that waits for its data shows no walls,
    /// as before those measures had them.
    fn wall_drawn(&self, side: usize, cache: &CachedProfile) -> bool {
        self.exposure_levels[side].get()
            && (self.gamma_selected(cache)
                || !self
                    .waiting_inputs
                    .borrow()
                    .iter()
                    .any(|(id, _)| id == "measure_label"))
    }

    /// The measure drawn, as its place in [`MEASURES`].
    fn drawn_measure(&self, cache: &CachedProfile) -> usize {
        if self.gamma_selected(cache) {
            2
        } else {
            usize::from(
                cache.interest_available && (self.open_interest.get() || !cache.volume_available),
            )
        }
    }

    fn refresh(&self) {
        self.refresh_cache();
        // Held inputs can only apply once new data or a setting has rebuilt the cache. The flag is
        // taken only while inputs are held, and a rebuild during this retry, from an applied input
        // or a snapshot arriving meanwhile, leaves it set for the next lookup.
        if !self.waiting_inputs.borrow().is_empty()
            && std::mem::take(&mut self.cache.borrow_mut().rebuilt)
        {
            let (awaiting_chain, supplied) = {
                let cache = self.cache.borrow();
                (cache.awaiting_chain, cache.supplied.is_some())
            };
            if awaiting_chain && !supplied {
                return;
            }
            // In saved order, since the measure decides which levels are offered.
            let pending = self.waiting_inputs.take();
            let mut waiting = Vec::new();
            for (id, value) in pending {
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
        let (values, profile) = match &self.source {
            ProfileSource::Volume(source) => {
                let values = source.values();
                if Arc::ptr_eq(&values, &cache.values) {
                    return;
                }
                cache.rebuilt = true;
                let row_size = self.config.row_size().expect("fixed options row height");
                let profile = VolumeProfile::from_options(&values, row_size);
                (values, profile)
            }
            ProfileSource::Chain(chain, expiries) => {
                let rows = chain.rows();
                let now = self.as_of.unwrap_or_else(Utc::now);
                let day = now.date_naive();
                let same_rows = Arc::ptr_eq(&rows, &cache.chain_rows);
                let nearest = expiries.iter().copied().find(|date| *date >= day);
                // The listed dates the profile sums, oldest first.
                let dates: Vec<NaiveDate> = match self.expiries {
                    _ if self.at_expiration => expiries
                        .iter()
                        .copied()
                        .filter(|date| *date >= day)
                        .take(self.expirations)
                        .collect(),
                    Expiries::Nearest => nearest.into_iter().collect(),
                    Expiries::One(date) => vec![date],
                    Expiries::Through if self.max_expiries == 0 => expiries
                        .iter()
                        .copied()
                        .filter(|date| *date <= self.through_date)
                        .collect(),
                    Expiries::Through => expiries
                        .iter()
                        .copied()
                        .filter(|date| *date >= day && *date <= self.through_date)
                        .take(self.max_expiries)
                        .collect(),
                };
                // The expiry whose rows give the expected move and the strike window's price.
                let move_expiry = match self.expiries {
                    Expiries::Through if !self.at_expiration => {
                        dates.iter().rev().copied().find(|date| *date >= day)
                    }
                    _ => dates.first().copied(),
                };
                if self.expected_move && (!same_rows || cache.move_time != Some(now.timestamp())) {
                    cache.expected_move = move_expiry.and_then(|date| {
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
                // A close can move the strike window without new quotes.
                let window_close = (self.strikes_around > 0)
                    .then_some(self.last_close)
                    .flatten();
                if same_rows
                    && cache.day == Some(day)
                    && same_exposure
                    && cache.window_close == window_close
                {
                    return;
                }
                cache.rebuilt = true;
                cache.window_close = window_close;
                cache.underlying = move_expiry.and_then(|date| underlying_price(&rows, date));
                let price = cache.underlying.or(self.last_close);
                // The chain rows of the listed dates in `dates`, and their exposure, supplied or
                // computed from those rows.
                let selected_of = |dates: &[NaiveDate]| -> Vec<&OptionQuote> {
                    rows.iter()
                        .filter(|row| dates.binary_search(&row.expiry).is_ok())
                        .collect()
                };
                let exposure_of = |dates: &[NaiveDate], selected: &[&OptionQuote]| {
                    if dates.is_empty() {
                        return None;
                    }
                    let snapshot = if let Some(supplied) = &supplied {
                        let mut snapshot = (**supplied).clone();
                        snapshot
                            .rows
                            .retain(|row| dates.binary_search(&row.expiry).is_ok());
                        if snapshot.rows.len() != supplied.rows.len() {
                            snapshot.zero_gamma = None;
                        }
                        Some(snapshot)
                    } else {
                        OptionExposure::gamma(selected.iter().copied())
                            .ok()
                            .flatten()
                    };
                    snapshot.and_then(|snapshot| exposure_profile(snapshot, supplied.is_some()))
                };
                let selected = selected_of(&dates);
                cache.exposure = exposure_of(&dates, &selected);
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
                cache.walls = exposure_walls(cache.exposure.as_deref());
                cache.awaiting_chain = selected.is_empty();
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
                let gamma = self.gamma_selected(&cache);
                let interest = self.drawn_measure(&cache) == 1;
                let sides = self.sides[self.drawn_measure(&cache)];
                let contracts = interest || cache.volume_available;
                // The figures the rows show: exposure scaled to its largest figure, or volume or
                // open interest by contract.
                let figures_of = |selected: &[&OptionQuote], exposure: Option<&ExposureProfile>| {
                    if gamma {
                        exposure.map(exposure_figures).unwrap_or_default()
                    } else if contracts {
                        contract_figures(selected, interest)
                    } else {
                        Vec::new()
                    }
                };
                let figures = figures_of(&selected, cache.exposure.as_deref());
                // Rows keep the height of every selected strike as the window moves, and the
                // profiles drawn at their expirations share both.
                let row_size = strike_step(&figures);
                let window = strike_window(
                    figures.iter().map(|value| value.strike),
                    price,
                    self.strikes_around,
                );
                let rows_of =
                    |dates: &[NaiveDate],
                     selected: &[&OptionQuote],
                     figures: &[OptionsVolume],
                     exposure: Option<&Arc<ExposureProfile>>| {
                        if gamma {
                            exposure_rows(exposure?, figures, row_size, window.as_ref())
                        } else {
                            let window = window.as_ref();
                            contract_rows(
                                sides, figures, selected, dates, expiries, row_size, window,
                            )
                        }
                    };
                let profile = rows_of(&dates, &selected, &figures, cache.exposure.as_ref());
                // Each group of upcoming expirations at the bar of its latest date, groups at one
                // bar together, with the walls of its own figures.
                let mut groups: Vec<(usize, Vec<NaiveDate>)> = Vec::new();
                if self.at_expiration {
                    let key = |date: &NaiveDate| match self.grouping {
                        1 => (date.iso_week().year(), date.iso_week().week()),
                        2 => (date.year(), date.month()),
                        _ => (date.year(), date.ordinal()),
                    };
                    for group in dates.chunk_by(|a, b| key(a) == key(b)) {
                        let latest = group.last().expect("a group has a date");
                        let Some(&(_, bar)) =
                            self.expiry_bars.iter().find(|(date, _)| date == latest)
                        else {
                            continue;
                        };
                        match groups.last_mut() {
                            Some((at, merged)) if *at == bar => merged.extend_from_slice(group),
                            _ => groups.push((bar, group.to_vec())),
                        }
                    }
                }
                let walls = [0, 1].map(|side| self.wall_drawn(side, &cache));
                let wall_colors =
                    [3, 4].map(|line| self.plot_style(line).color.to_srgba_unmultiplied());
                cache.groups = groups
                    .into_iter()
                    .map(|(bar, dates)| {
                        let selected = selected_of(&dates);
                        let exposure = gamma.then(|| exposure_of(&dates, &selected)).flatten();
                        let figures = figures_of(&selected, exposure.as_deref());
                        let profile = rows_of(&dates, &selected, &figures, exposure.as_ref()).map(
                            |mut profile| {
                                let latest = *dates.last().expect("a group has a date");
                                if self.expiry_labels {
                                    let listed =
                                        expiries.binary_search(&latest).unwrap_or_default();
                                    let color = EXPIRY_COLORS[listed % EXPIRY_COLORS.len()];
                                    profile.expiry_tag =
                                        Some((latest, color.to_srgba_unmultiplied()));
                                }
                                if walls.contains(&true) {
                                    let strikes = if gamma {
                                        exposure_walls(exposure.as_deref())
                                    } else {
                                        option_walls(&figures)
                                    };
                                    profile.walls = Some(OptionWalls {
                                        strikes: [0, 1]
                                            .map(|side| strikes[side].filter(|_| walls[side])),
                                        colors: wall_colors,
                                    });
                                }
                                Arc::new(profile)
                            },
                        );
                        (bar, profile)
                    })
                    .collect();
                cache.supplied = supplied;
                cache.chain_rows = rows;
                cache.day = Some(day);
                if gamma {
                    cache.profile = profile.map(Arc::new);
                    return;
                }
                (Arc::from(figures), profile)
            }
        };
        let calls: f64 = values.iter().map(|row| row.calls).sum();
        let puts: f64 = values.iter().map(|row| row.puts).sum();
        cache.ratio = put_call_ratio(calls, puts);
        cache.walls = option_walls(&values);
        cache.profile = profile.map(Arc::new);
        cache.values = values;
    }

    fn move_band(&self) -> Option<(NaiveDate, [f64; 2])> {
        if !self.matching_symbol || !self.expected_move {
            return None;
        }
        self.refresh();
        self.cache.borrow().expected_move
    }
}

/// The newest underlying price supplied with `expiry`'s quotes.
fn underlying_price(rows: &[OptionQuote], expiry: NaiveDate) -> Option<f64> {
    rows.iter()
        .filter(|row| row.expiry == expiry && row.underlying_price.is_some())
        .max_by_key(|row| row.time)
        .and_then(|row| row.underlying_price)
}

/// The strikes drawn with `around` listed strikes on each side of `price`, from the lowest to the
/// highest kept: every strike at the price and the `around` nearest below and above it. `None`,
/// drawing every strike, for no limit or no finite price.
fn strike_window(
    strikes: impl IntoIterator<Item = f64>,
    price: Option<f64>,
    around: usize,
) -> Option<std::ops::RangeInclusive<f64>> {
    let price = price.filter(|price| price.is_finite() && around > 0)?;
    let mut strikes: Vec<f64> = strikes.into_iter().collect();
    strikes.sort_by(f64::total_cmp);
    strikes.dedup();
    let below = strikes.partition_point(|strike| *strike < price);
    let above = strikes.partition_point(|strike| *strike <= price);
    let kept =
        &strikes[below.saturating_sub(around)..above.saturating_add(around).min(strikes.len())];
    Some(*kept.first()?..=*kept.last()?)
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
    let price = underlying_price(rows, expiry).or(last_close)?;
    let rows = || rows.iter().filter(|row| row.expiry == expiry);
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

/// A chain snapshot's exposure by strike, with the note its readout shows: the unit, whose signs
/// it uses, its time, the model and any missing contracts. `None` for an unrepresentable sum.
fn exposure_profile(snapshot: OptionExposure, supplied: bool) -> Option<Arc<ExposureProfile>> {
    let rows = aggregate_exposure(&snapshot.rows, supplied)?;
    let signs = if supplied {
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
}

/// The strikes with the largest call and the largest put exposure in size, the lower strike on
/// a tie; a side without exposure has none.
fn exposure_walls(exposure: Option<&ExposureProfile>) -> [Option<f64>; 2] {
    [false, true].map(|puts| {
        exposure.and_then(|snapshot| {
            snapshot
                .rows
                .iter()
                .filter_map(|row| {
                    let value = if puts { row.puts?.abs() } else { row.calls? };
                    Some((value, row.strike))
                })
                .filter(|(value, _)| *value != 0.0)
                .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.total_cmp(&a.1)))
                .map(|(_, strike)| strike)
        })
    })
}

/// Exposure by strike as row figures in size, scaled to the largest: the larger of the call and
/// the net exposure as calls, and the put exposure as puts.
fn exposure_figures(exposure: &ExposureProfile) -> Vec<OptionsVolume> {
    let max = exposure
        .rows
        .iter()
        .flat_map(|row| [row.calls, row.puts, row.net])
        .flatten()
        .map(f64::abs)
        .fold(1.0, f64::max);
    exposure
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
        .collect()
}

/// Rows `row_size` high of `exposure`'s scaled `figures` at the strikes of `window`, which also
/// limits the exposure the rows draw.
fn exposure_rows(
    exposure: &Arc<ExposureProfile>,
    figures: &[OptionsVolume],
    row_size: f64,
    window: Option<&std::ops::RangeInclusive<f64>>,
) -> Option<VolumeProfile> {
    let drawn = |strike: &f64| window.is_none_or(|w| w.contains(strike));
    let kept: Vec<_> = figures
        .iter()
        .copied()
        .filter(|value| drawn(&value.strike))
        .collect();
    let mut profile = VolumeProfile::from_options(&kept, row_size)?;
    profile.exposure = Some(if window.is_some() {
        let mut shown = (**exposure).clone();
        shown.rows.retain(|row| drawn(&row.strike));
        Arc::new(shown)
    } else {
        Arc::clone(exposure)
    });
    Some(profile)
}

/// The open interest, or the volume, of each contract of `selected`, as the calls or the puts
/// of its strike. Every contract must report it.
fn contract_figures(selected: &[&OptionQuote], interest: bool) -> Vec<OptionsVolume> {
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
}

/// Rows `row_size` high of the contract `figures` of `selected` at the strikes of `window`, as
/// the Sides choice `sides` draws them. By expiration, each of `dates` is a part in the colour of
/// its place among the `listed` expiries.
fn contract_rows(
    sides: usize,
    figures: &[OptionsVolume],
    selected: &[&OptionQuote],
    dates: &[NaiveDate],
    listed: &[NaiveDate],
    row_size: f64,
    window: Option<&std::ops::RangeInclusive<f64>>,
) -> Option<VolumeProfile> {
    // The strikes drawn, each with the place of its expiry among `dates`.
    let (drawn, part): (Vec<_>, Vec<_>) = figures
        .iter()
        .zip(selected)
        .filter(|(value, _)| window.is_none_or(|w| w.contains(&value.strike)))
        .map(|(value, row)| {
            let part = dates.binary_search(&row.expiry).unwrap_or_default();
            (*value, part)
        })
        .unzip();
    match sides {
        3 => {
            let labels = dates
                .iter()
                .map(|date| {
                    let listed = listed.binary_search(date).unwrap_or_default();
                    let color = EXPIRY_COLORS[listed % EXPIRY_COLORS.len()];
                    (date.to_string(), color.to_srgba_unmultiplied())
                })
                .collect();
            VolumeProfile::from_option_parts(&drawn, &part, labels, row_size)
        }
        // Each strike's puts over its calls; a strike without calls has no row.
        4 => {
            let ratios: Vec<_> = strike_sums(&drawn)
                .into_iter()
                .filter_map(|(strike, [calls, puts])| {
                    Some(OptionsVolume {
                        strike,
                        calls: put_call_ratio(calls, puts)?,
                        puts: 0.0,
                    })
                })
                .collect();
            VolumeProfile::from_options(&ratios, row_size).map(|mut profile| {
                profile.ratio_rows = true;
                profile
            })
        }
        _ => VolumeProfile::from_options(&drawn, row_size),
    }
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

/// The call and put figures of each strike, summed over the selected expiries, from the lowest
/// strike.
fn strike_sums(values: &[OptionsVolume]) -> Vec<(f64, [f64; 2])> {
    let mut ordered: Vec<_> = values.iter().collect();
    ordered.sort_by(|a, b| a.strike.total_cmp(&b.strike));
    let mut strikes: Vec<(f64, [f64; 2])> = Vec::new();
    for value in ordered {
        match strikes.last_mut() {
            Some((strike, sums)) if *strike == value.strike => {
                sums[0] += value.calls;
                sums[1] += value.puts;
            }
            _ => strikes.push((value.strike, [value.calls, value.puts])),
        }
    }
    strikes
}

/// Puts divided by calls; `None` without calls or for an unrepresentable figure.
fn put_call_ratio(calls: f64, puts: f64) -> Option<f64> {
    (calls > 0.0 && calls.is_finite() && puts.is_finite())
        .then_some(puts / calls)
        .filter(|ratio| ratio.is_finite())
}

/// Strikes with the largest call and the largest put figure, each summed over the selected
/// expiries, the lower strike on a tie. A side with no positive figure has no wall, and an
/// unrepresentable sum leaves both out.
fn option_walls(values: &[OptionsVolume]) -> [Option<f64>; 2] {
    let strikes = strike_sums(values);
    if strikes
        .iter()
        .flat_map(|(_, sums)| *sums)
        .any(|sum| !sum.is_finite())
    {
        return [None, None];
    }
    [0, 1].map(|side| {
        strikes
            .iter()
            .filter(|(_, sums)| sums[side] > 0.0)
            .max_by(|a, b| {
                a.1[side]
                    .total_cmp(&b.1[side])
                    .then_with(|| b.0.total_cmp(&a.0))
            })
            .map(|(strike, _)| *strike)
    })
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
        let close = if self.matching_symbol {
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
        if let ProfileSource::Chain(_, dates) = &self.source {
            // The time scale continues past the last bar, where upcoming expiries fall.
            let bars: Vec<_> = dates
                .iter()
                .filter_map(|date| {
                    let time = date.and_hms_opt(0, 0, 0)?.and_utc();
                    let index = crate::model::index_at_with_timeframe(
                        data,
                        time,
                        self.timeframe,
                        Some(&crate::model::TimezoneId::utc()),
                    )?;
                    (index.is_finite() && index >= -0.5)
                        .then(|| (*date, index.round().max(0.0) as usize))
                })
                .collect();
            if self.expiry_bars != bars {
                self.expiry_bars = bars;
                *self.cache.get_mut() = CachedProfile::default();
            }
        }
        self.values = IndicatorValues::missing(data.len());
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn set_timeframe(&mut self, timeframe: crate::model::Timeframe) {
        self.timeframe = Some(timeframe);
    }
    // The option chain belongs to the chart's underlying, so the profile cannot follow another symbol.
    // The option chain belongs to the chart's underlying, so the profile cannot follow another symbol.
    fn draws_on_other_bars(&self) -> bool {
        false
    }
    fn set_chart_symbol(&mut self, symbol: &str) -> bool {
        // The chart's instrument may name the underlying otherwise, such as `X:ABC` for `ABC`.
        let matching = symbol.is_empty() || symbol == self.symbol;
        let changed = self.matching_symbol != matching;
        self.matching_symbol = matching;
        if changed {
            self.last_close = None;
            self.cache.get_mut().move_time = None;
        }
        changed && matches!(self.source, ProfileSource::Chain(..))
    }
    fn volume_profiles(&self, visible: Range<usize>) -> Vec<StudyProfile> {
        if !self.visible || !self.matching_symbol || visible.is_empty() {
            return Vec::new();
        }
        self.refresh();
        let cache = self.cache.borrow();
        let mut config = self.config;
        let (mut sides, mut color) = ([self.colors[0], self.colors[1]], self.colors[0]);
        let (mut right_aligned, mut width) = (true, 0.9);
        if matches!(self.source, ProfileSource::Chain(..)) {
            let [call, put, positive, negative, total] = self.bar_colors;
            (config.display, sides, color) = match self.sides[self.drawn_measure(&cache)] {
                0 => (ProfileDisplay::UpDown, [call, put], call),
                1 => (ProfileDisplay::Delta, [positive, negative], positive),
                4 => (ProfileDisplay::Total, [call, put], put),
                _ => (ProfileDisplay::Total, [call, put], total),
            };
            (right_aligned, width) = (!self.left, (self.width_percent / 30.0) as f32);
            config.show_values = self.show_values && !self.gamma_selected(&cache);
        }
        config.up_color = sides[0].to_srgba_unmultiplied();
        config.down_color = sides[1].to_srgba_unmultiplied();
        let color = color.to_srgba_unmultiplied();
        if !self.at_expiration {
            return cache
                .profile
                .iter()
                .map(|profile| StudyProfile {
                    bars: visible.clone(),
                    profile: Arc::clone(profile),
                    config,
                    color,
                    right_aligned,
                    strip: Some(0.3),
                    width,
                    volume_decimals: None,
                })
                .collect();
        }
        // Rows grow from each group's date to the side chosen, into the bars before the next group
        // that way, or as many as from the group on the other side when none is there.
        let groups = &cache.groups;
        groups
            .iter()
            .enumerate()
            .filter_map(|(group, (bar, profile))| {
                let profile = profile.as_ref()?;
                let (before, after) = (group > 0, group + 1 < groups.len());
                let room = match (self.left, before, after) {
                    (false, _, true) | (true, false, true) => groups[group + 1].0 - bar,
                    (true, true, _) | (false, true, false) => bar - groups[group - 1].0,
                    _ => LONE_EXPIRATION_BARS,
                };
                let bars = if self.left {
                    bar.saturating_add(1).saturating_sub(room)..bar.saturating_add(1)
                } else {
                    *bar..bar.saturating_add(room)
                };
                (bars.start < visible.end && visible.start < bars.end).then(|| StudyProfile {
                    bars,
                    profile: Arc::clone(profile),
                    config,
                    color,
                    right_aligned: self.left,
                    strip: None,
                    width: 0.9,
                    volume_decimals: None,
                })
            })
            .collect()
    }
    fn colors(&self) -> Vec<Color32> {
        if matches!(self.source, ProfileSource::Chain(..)) {
            (0..9).map(|line| self.plot_style(line).color).collect()
        } else {
            self.colors.clone()
        }
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        if matches!(self.source, ProfileSource::Chain(..)) && colors.len() >= 8 {
            self.colors.resize(9, self.colors[0]);
        }
        for (stored, color) in self.colors.iter_mut().zip(colors) {
            *stored = color;
        }
        // The walls of the profiles drawn at their expirations hold their colours.
        *self.cache.get_mut() = CachedProfile::default();
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
                "Current price".into(),
            ]
        } else {
            Vec::new()
        }
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            visible: true,
            color: self.colors.get(line).copied().unwrap_or(self.colors[0]),
            width: 1.0,
            pattern: if line == 8 {
                crate::model::LineStyle::Dotted
            } else {
                crate::model::LineStyle::Solid
            },
            plot_type: crate::studies::PlotType::Line,
        }
    }
    fn legend_visible(&self, line: usize) -> bool {
        if (6..=7).contains(&line) {
            return self.expected_move;
        }
        if line == 8 {
            return self.current_price;
        }
        self.refresh();
        if (3..=4).contains(&line) {
            // Profiles drawn at their expirations mark their own walls.
            !self.at_expiration && self.wall_drawn(line - 3, &self.cache.borrow())
        } else if self.gamma_selected(&self.cache.borrow()) {
            line == 2 || line == 5 && self.exposure_levels[2].get()
        } else {
            (line == 0 && self.put_call) || (line == 1 && self.max_pain.get())
        }
    }
    fn readout_only(&self, line: usize) -> bool {
        // The put/call ratio and net gamma are far from the prices they would share a scale with.
        matches!(self.source, ProfileSource::Chain(..)) && (line == 0 || line == 2)
    }
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        if !self.matching_symbol || bar >= self.values.len() {
            return None;
        }
        if (6..=7).contains(&line) {
            return self.move_band().map(|(_, band)| band[line - 6]);
        }
        self.refresh();
        let cache = self.cache.borrow();
        if line == 8 {
            // The price the strike window follows, which a new close can move without new quotes.
            return self
                .current_price
                .then_some(cache.underlying.or(self.last_close))
                .flatten()
                .filter(|price| price.is_finite());
        }
        if (3..=4).contains(&line) {
            return (!self.at_expiration && self.wall_drawn(line - 3, &cache))
                .then_some(cache.walls[line - 3])
                .flatten();
        }
        if self.gamma_selected(&cache) {
            return match line {
                2 => cache.net,
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
    fn settings(&self) -> crate::studies::StudySettings {
        // Daily option figures do not line up with the bars of an intraday chart.
        crate::studies::StudySettings {
            timeframe_visibility: crate::model::TimeframeVisibility::higher_intervals(
                crate::model::Timeframe::Day1,
            ),
            ..crate::studies::settings::declared_settings(self)
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
                "layout",
                "Layout",
                InputValue::Choice(usize::from(self.at_expiration)),
                InputKind::Choice(vec!["Edge of plot".into(), "At expiration".into()]),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "expirations",
                "Maximum expirations",
                InputValue::Length(self.expirations),
                InputKind::Length(1, 1_000),
            )
            .with_default(InputValue::Length(10))
            .shown_when("layout", vec![1]),
            StudyInput::new(
                "grouping",
                "Group expirations",
                InputValue::Choice(self.grouping),
                InputKind::Choice(GROUPINGS.map(Into::into).to_vec()),
            )
            .with_default(InputValue::Choice(0))
            .shown_when("layout", vec![1]),
            StudyInput::new(
                "expiry_labels",
                "Expiration labels",
                InputValue::Toggle(self.expiry_labels),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(true))
            .shown_when("layout", vec![1]),
            StudyInput::new(
                "expiry",
                "Expiry",
                InputValue::Choice(expiry),
                InputKind::Choice(choices),
            )
            .with_default(InputValue::Choice(0))
            .shown_when("layout", vec![0]),
            StudyInput::new(
                "expiry_label",
                "Expiry label",
                InputValue::Text(expiry_label),
                InputKind::Text,
            )
            .with_default(InputValue::Text("Nearest".into()))
            .shown_when("expiry", Vec::new()),
        ];
        inputs.push(
            StudyInput::new(
                "through_date",
                "Through date (YYYY-MM-DD)",
                InputValue::Text(self.through_date.to_string()),
                InputKind::Text,
            )
            .shown_when("expiry", vec![1 + dates.len()]),
        );
        inputs.push(
            StudyInput::new(
                "max_expiries",
                "Maximum expirations (0 = all)",
                InputValue::Length(self.max_expiries),
                InputKind::Length(0, usize::MAX),
            )
            .with_default(InputValue::Length(0))
            .shown_when("expiry", vec![1 + dates.len()]),
        );
        let mut measures: Vec<String> = Vec::new();
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
        for ((id, choices, default), choice) in SIDES.into_iter().zip(self.sides) {
            inputs.push(
                StudyInput::new(
                    id,
                    "Sides",
                    InputValue::Choice(choice),
                    InputKind::Choice(choices.iter().map(|choice| (*choice).into()).collect()),
                )
                .with_default(InputValue::Choice(default)),
            );
        }
        inputs.push(
            StudyInput::new(
                "strikes_around_price",
                "Strikes around price (0 = all)",
                InputValue::Length(self.strikes_around),
                InputKind::Length(0, MAX_STRIKES_AROUND),
            )
            .with_default(InputValue::Length(10)),
        );
        inputs.extend([
            StudyInput::new(
                "put_call",
                "Put/call ratio",
                InputValue::Toggle(self.put_call),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(true)),
            StudyInput::new(
                "show_values",
                "Values",
                InputValue::Toggle(self.show_values),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(false)),
            StudyInput::new(
                "current_price",
                "Current price line",
                InputValue::Toggle(self.current_price),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(true)),
        ]);
        for (index, id, label) in [
            (0, "call_wall", "Call wall"),
            (1, "put_wall", "Put wall"),
            (2, "zero_gamma", "Zero gamma (provider)"),
        ] {
            if index < 2 || gamma && cache.supplied.is_some() {
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
        // A measure that waits is listed in the fixed order, in which a choice then names it.
        let held_measure = waiting.iter().find_map(|(id, value)| match value {
            InputValue::Text(label) if id == "measure_label" => {
                MEASURES.iter().position(|measure| measure == label)
            }
            _ => None,
        });
        inputs.retain(|input| {
            !waiting
                .iter()
                .any(|(id, _)| *id == input.id || (id == "measure_label" && input.id == "measure"))
        });
        inputs.extend(
            held_measure
                .and_then(|position| waiting_input("measure", InputValue::Choice(position))),
        );
        inputs.extend(
            waiting
                .iter()
                .filter_map(|(id, value)| waiting_input(id, value.clone())),
        );
        if let Some(label) = inputs.iter().position(|input| input.id == "measure_label") {
            let mut label = inputs.remove(label);
            let position = inputs
                .iter()
                .position(|input| input.id == "measure")
                .map_or(inputs.len(), |position| {
                    if let InputKind::Choice(choices) = &inputs[position].kind
                        && let Some(default) = choices.first()
                    {
                        label.default = InputValue::Text(default.clone());
                    }
                    position + 1
                });
            inputs.insert(position, label);
        }
        // Each measure's Sides shows while the editor's Measure names that measure, and the ratio
        // and figure switches while it names volume or open interest.
        let listed = inputs
            .iter()
            .find_map(|input| match (input.id.as_str(), &input.kind) {
                ("measure", InputKind::Choice(choices)) => Some(choices.clone()),
                _ => None,
            });
        let at = |measure: usize| {
            listed
                .iter()
                .flatten()
                .position(|name| name == MEASURES[measure])
        };
        for input in &mut inputs {
            let shown: Vec<_> = match SIDES.iter().position(|(id, ..)| *id == input.id) {
                // Until the chain lists a measure, the Sides of the measure drawn shows.
                Some(measure) if listed.is_some() || measure != self.drawn_measure(&cache) => {
                    at(measure).into_iter().collect()
                }
                None if matches!(input.id.as_str(), "put_call" | "show_values")
                    && listed.is_some() =>
                {
                    at(0).into_iter().chain(at(1)).collect()
                }
                _ => continue,
            };
            input.shown_when = Some(("measure".into(), shown));
        }
        inputs.extend([
            StudyInput::new(
                "placement",
                "Placement",
                InputValue::Choice(usize::from(self.left)),
                InputKind::Choice(vec!["Right".into(), "Left".into()]),
            )
            .with_default(InputValue::Choice(0)),
            StudyInput::new(
                "profile_width",
                "Width (% of plot)",
                InputValue::Number(self.width_percent),
                InputKind::Number(1.0, 30.0),
            )
            .with_default(InputValue::Number(27.0))
            .shown_when("layout", vec![0]),
        ]);
        for ((id, label, default), color) in BAR_COLORS.into_iter().zip(self.bar_colors) {
            inputs.push(
                StudyInput::new(id, label, InputValue::Color(color), InputKind::Color)
                    .with_default(InputValue::Color(default)),
            );
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
            // A larger limit saved before the input had a maximum still restores.
            ("strikes_around_price", InputValue::Length(count), _) => {
                InputValue::Length(count.min(MAX_STRIKES_AROUND))
            }
            (_, value, _) => value,
        };
        let input = self.inputs().into_iter().find(|input| input.id == id);
        // A measure position names its measure in the fixed order while a measure waits, which
        // lists that order, while the chain is awaited, and when the chain lists no such
        // position. Only a position the answered chain lists counts in the chain's own listing.
        if let (InputValue::Choice(position), ProfileSource::Chain(..)) = (&value, &self.source)
            && id == "measure"
            && let Some(measure) = MEASURES.get(*position)
            && (self.cache.borrow().awaiting_chain
                || self
                    .waiting_inputs
                    .borrow()
                    .iter()
                    .any(|(held, _)| held == "measure_label")
                || !input.as_ref().is_some_and(|input| input.accepts(&value)))
        {
            return self.set_input("measure_label", InputValue::Text((*measure).into()));
        }
        let chain_input = matches!(self.source, ProfileSource::Chain(..))
            && waiting_input(id, value.clone()).is_some_and(|input| input.accepts(&value));
        let unavailable_label = chain_input
            && id == "measure_label"
            && !input
                .as_ref()
                .is_some_and(|input| self.set_chain_input(id, &value, input));
        let waiting = self.waiting_inputs.get_mut();
        let held = waiting.iter().position(|(saved, _)| saved == id);
        if chain_input
            && (held.is_some()
                || unavailable_label
                || !input.as_ref().is_some_and(|input| input.accepts(&value)))
        {
            // A held measure shows the default until it applies, as before quotes arrive.
            if id == "measure_label" {
                self.open_interest.set(false);
                self.gamma.set(false);
            }
            // The rebuild on the next refresh retries the held value.
            *self.cache.get_mut() = CachedProfile::default();
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
            ("layout", InputValue::Choice(choice)) => self.at_expiration = choice == 1,
            ("expirations", InputValue::Length(count)) => self.expirations = count,
            ("grouping", InputValue::Choice(choice)) => self.grouping = choice,
            ("expiry_labels", InputValue::Toggle(shown)) => self.expiry_labels = shown,
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
            ("volume_sides", InputValue::Choice(choice)) => self.sides[0] = choice,
            ("interest_sides", InputValue::Choice(choice)) => self.sides[1] = choice,
            ("gamma_sides", InputValue::Choice(choice)) => self.sides[2] = choice,
            ("put_call", InputValue::Toggle(shown)) => self.put_call = shown,
            ("show_values", InputValue::Toggle(shown)) => self.show_values = shown,
            ("current_price", InputValue::Toggle(shown)) => self.current_price = shown,
            ("max_expiries", InputValue::Length(count)) => self.max_expiries = count,
            ("strikes_around_price", InputValue::Length(count)) => self.strikes_around = count,
            ("placement", InputValue::Choice(choice)) => self.left = choice == 1,
            ("profile_width", InputValue::Number(percent)) => self.width_percent = percent,
            (id, InputValue::Color(color)) => {
                let Some(index) = BAR_COLORS.iter().position(|(bar, ..)| *bar == id) else {
                    return Err(StudyError::invalid_input(id));
                };
                self.bar_colors[index] = color;
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
            chart.set_timeframe(crate::model::Timeframe::Day1);
            chart.add_indicator(study.clone_box());
            let fill_color = study.fills()[0].color;
            let ctx = egui::Context::default();
            let painted = |chart: &mut crate::widget::Chart| {
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
                painted
            };
            for (symbol, shown) in [("SAMPLE", true), ("OTHER", false), ("SAMPLE", true)] {
                chart.set_symbol(symbol);
                assert_eq!(painted(&mut chart), shown, "{symbol} before resolution");
                // A feed may describe a symbol under another name.
                chart.set_instrument(Some(crate::model::Instrument::new(format!("X:{symbol}"))));
                assert_eq!(
                    painted(&mut chart),
                    shown,
                    "{symbol} with {count} underlying bars"
                );
            }
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
        study.set_chart_symbol("OTHER");
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
                            .set_input("gamma_sides", InputValue::Choice(usize::from(!split)))
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
                                        strip: true,
                                        walls_visible: [true; 2],
                                        width: 0.9,
                                        volume_decimals: None,
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
                    study
                        .set_input("gamma_sides", InputValue::Choice(1))
                        .unwrap();
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
                assert_eq!(study.plot_value(3, 0), Some(100.0), "signed provider calls");
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
                exposure_sink.push(snapshot.clone()).unwrap();
                study
                    .set_input(
                        "through_date",
                        InputValue::Text(expiry.pred_opt().unwrap().to_string()),
                    )
                    .unwrap();
                assert!(study.volume_profiles(0..1).is_empty());
                assert_eq!(
                    study.plot_value(5, 0),
                    None,
                    "a provider level needs a selected expiration"
                );
                study
                    .set_input("through_date", InputValue::Text(next.to_string()))
                    .unwrap();
                assert_eq!(study.plot_value(5, 0), Some(0.0));
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
        let feed = ChainFeed::default();
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let time = Utc::now();
        let chain = OptionChain::new(
            &feed,
            OptionChainQuery::new("SAMPLE", expiry..=expiry),
            || {},
        )
        .unwrap();
        let sink = feed.0.lock().unwrap().clone().unwrap();
        let bars = [Bar::new(time, 105.0, 112.0, 98.0, 105.0, 1.0)];
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
            study.set_input("measure", InputValue::Choice(1)).unwrap();
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
            for (symbol, shown) in [("SAMPLE", true), ("OTHER", false), ("", true)] {
                study.set_chart_symbol(symbol);
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

    /// A live chain listing E1, a day from today, and E2, a week after it, with a call and a
    /// put at 100 in each: call and put volume 20 and 10 at E1 and 5 and 5 at E2, and at both
    /// open interest 3 for the call and 1 for the put, gamma 0.05, a multiplier of 100 and the
    /// underlying at 100. It lists Volume, Open interest and Gamma exposure. One contract's
    /// gamma is 0.05 * 100 * 100² * 0.01 = 500, so E1's calls give +1500 and its puts -500.
    fn chain_by_expiry() -> (OptionChain, Vec<NaiveDate>) {
        let today = Utc::now().date_naive();
        let dates = vec![
            today + chrono::TimeDelta::days(1),
            today + chrono::TimeDelta::days(8),
        ];
        let feed = ChainFeed::default();
        let query = OptionChainQuery::new("SAMPLE", dates[0]..=dates[1]);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let rows = dates
            .iter()
            .zip([[20.0, 10.0], [5.0, 5.0]])
            .flat_map(|(&expiry, volumes)| {
                [OptionKind::Call, OptionKind::Put]
                    .into_iter()
                    .zip(volumes)
                    .map(move |(kind, volume)| {
                        let call = kind == OptionKind::Call;
                        let symbol = format!("{expiry}-{kind:?}");
                        let mut row = OptionQuote::new(symbol, expiry, 100.0, kind, Utc::now());
                        row.volume = Some(volume);
                        row.open_interest = Some(if call { 3.0 } else { 1.0 });
                        row.gamma = Some(0.05);
                        row.multiplier = Some(100.0);
                        row.underlying_price = Some(100.0);
                        row
                    })
            })
            .collect();
        feed.0.lock().unwrap().as_ref().unwrap().push(rows).unwrap();
        (chain, dates)
    }

    /// A live chain listing one expiry, tomorrow, with call and put volume 10 and 5 at 100, 0 and
    /// 7 at 105, and 4 and 8 at 110.
    fn chain_with_a_strike_without_calls() -> (OptionChain, Vec<NaiveDate>) {
        chain_with_volumes(&[
            (100.0, [10.0, 5.0]),
            (105.0, [0.0, 7.0]),
            (110.0, [4.0, 8.0]),
        ])
    }

    /// A live chain listing one expiry, tomorrow, with the given call and put volume at each
    /// strike.
    fn chain_with_volumes(strikes: &[(f64, [f64; 2])]) -> (OptionChain, Vec<NaiveDate>) {
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let feed = ChainFeed::default();
        let query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let rows = strikes
            .iter()
            .flat_map(|&(strike, volumes)| {
                [OptionKind::Call, OptionKind::Put]
                    .into_iter()
                    .zip(volumes)
                    .map(move |(kind, volume)| {
                        let symbol = format!("{strike}-{kind:?}");
                        let mut row = OptionQuote::new(symbol, expiry, strike, kind, Utc::now());
                        row.volume = Some(volume);
                        row
                    })
            })
            .collect();
        feed.0.lock().unwrap().as_ref().unwrap().push(rows).unwrap();
        (chain, vec![expiry])
    }

    /// Applies `study`'s inputs with `edits` made to them, as the settings editor does: a choice
    /// picked there also names the pick in the choice's hidden label.
    fn apply_edits(study: &mut dyn Indicator, edits: &[(&str, InputValue)]) {
        let listed = study.inputs();
        let edited = |id: &str| {
            edits
                .iter()
                .find_map(|(edit, value)| (*edit == id).then(|| value.clone()))
        };
        let inputs: Vec<_> = listed
            .iter()
            .map(|input| {
                let label = input.id.strip_suffix("_label").and_then(|choice| {
                    let InputValue::Choice(pick) = edited(choice)? else {
                        return None;
                    };
                    listed.iter().find_map(|input| match &input.kind {
                        InputKind::Choice(choices) if input.id == choice => {
                            choices.get(pick).cloned()
                        }
                        _ => None,
                    })
                });
                let value = edited(&input.id)
                    .or(label.map(InputValue::Text))
                    .unwrap_or_else(|| input.value.clone());
                (input.id.clone(), value)
            })
            .collect();
        let settings = study.settings();
        study.apply_settings(&inputs, settings).unwrap();
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
        assert_eq!(study.line_names()[8], "Current price");
        assert_eq!(study.plot_style(8).pattern, crate::model::LineStyle::Dotted);
        assert_eq!(study.plot_value(8, 0), Some(100.0));
        assert!(study.legend_visible(8));
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

    /// A factory that builds the profile over `chain` and its listed `dates`, as the indicator
    /// dialog registers it.
    fn profile_factory(
        chain: OptionChain,
        dates: Vec<NaiveDate>,
    ) -> crate::studies::IndicatorFactory {
        let mut factory = crate::studies::IndicatorFactory::new();
        factory.register("Options Volume Profile", move || {
            Box::new(
                OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", dates.clone()).unwrap(),
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
            .create_with(&profile_factory(chain, vec![expiry]))
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
            .create_with(&profile_factory(later, vec![expiry]))
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

        // Walls are offered for every measure, so quotes with volume list the saved wall.
        let (later, later_sink, mut later_rows, _, _) = expected_move_chain();
        let mut loaded = template
            .create_with(&profile_factory(later, vec![expiry]))
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
        for (choice, listed, labelled, maximum, expected, selected) in [
            (3, vec![13, 20, 27], true, 0, 20.0, 2),
            (4, vec![13, 20, 27], true, 0, 33.0, 4),
            (4, vec![13, 20], true, 0, 33.0, 3),
            (3, vec![13, 27], true, 0, 13.0, 0),
            (3, vec![13, 20, 27], false, 0, 27.0, 3),
            (4, vec![13, 20], false, 0, 33.0, 3),
            // Through 20 November, of which only the 13th remains first after the roll.
            (4, vec![13, 20, 27], true, 1, 13.0, 4),
        ] {
            let mut study = original.create("Options Volume Profile").unwrap();
            study
                .set_input("expiry", InputValue::Choice(choice))
                .unwrap();
            // Every other input away from its default, which the restored study holds as saved.
            let color = |n: u8| InputValue::Color(Color32::from_rgba_unmultiplied(n, 2, 3, 200));
            let others = [
                ("max_expiries", InputValue::Length(maximum)),
                ("volume_sides", InputValue::Choice(2)),
                ("interest_sides", InputValue::Choice(1)),
                ("gamma_sides", InputValue::Choice(0)),
                ("strikes_around_price", InputValue::Length(2)),
                ("put_call", InputValue::Toggle(false)),
                ("placement", InputValue::Choice(1)),
                ("profile_width", InputValue::Number(12.5)),
                ("call_color", color(1)),
                ("put_color", color(2)),
                ("net_positive_color", color(3)),
                ("net_negative_color", color(4)),
                ("total_color", color(5)),
            ];
            for (id, value) in others.clone() {
                study.set_input(id, value).unwrap();
            }
            let mut saved = crate::studies::StudyTemplate::from_study(study.as_ref());
            if !labelled {
                saved.inputs.retain(|(id, _)| !id.ends_with("_label"));
            }
            let restored = saved.create_with(&make(dates(&listed))).unwrap();
            let plot = restored.volume_profiles(0..1).remove(0);
            assert_eq!(
                plot.profile.up(),
                [expected],
                "choice {choice}, {listed:?}, labelled {labelled}, maximum {maximum}"
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
            for (id, value) in others {
                let input = inputs.iter().find(|input| input.id == id);
                assert_eq!(input.map(|input| &input.value), Some(&value), "{id}");
            }
        }
    }

    #[test]
    fn saved_measure_restores_by_name() {
        let feed = ChainFeed::default();
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let time = Utc::now();
        let bars = vec![Bar::new(time, 100.0, 101.0, 99.0, 100.0, 1.0); 2];
        let chain = OptionChain::new(
            &feed,
            OptionChainQuery::new("SAMPLE", expiry..=expiry),
            || {},
        )
        .unwrap();
        let sink = feed.0.lock().unwrap().clone().unwrap();
        let rows: Vec<_> = [OptionKind::Call, OptionKind::Put]
            .into_iter()
            .map(|kind| {
                let mut row = OptionQuote::new(format!("100-{kind:?}"), expiry, 100.0, kind, time);
                let call = row.kind == OptionKind::Call;
                row.volume = Some(if call { 10.0 } else { 5.0 });
                row.open_interest = Some(if call { 4.0 } else { 2.0 });
                row.gamma = Some(0.05);
                row.multiplier = Some(100.0);
                row.underlying_price = Some(100.0);
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
        for ready in [true, false] {
            for (missing, pending_exposure) in [(0, false), (1, false), (2, false), (0, true)] {
                let feed = ChainFeed::default();
                let later = OptionChain::new(
                    &feed,
                    OptionChainQuery::new("SAMPLE", expiry..=expiry),
                    || {},
                )
                .unwrap();
                let later_sink = feed.0.lock().unwrap().clone().unwrap();
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
                let exposure = pending_exposure.then(|| {
                    OptionExposureHandle::new(
                        &feed,
                        OptionExposureQuery::new(
                            "SAMPLE",
                            expiry..=expiry,
                            OptionExposureKind::Gamma,
                        ),
                        || {},
                    )
                    .unwrap()
                });
                let mut factory = crate::studies::IndicatorFactory::new();
                factory.register("Options Volume Profile", move || {
                    let mut profile =
                        OptionsVolumeProfile::from_chain(later.clone(), "SAMPLE", vec![expiry])
                            .unwrap();
                    if let Some(exposure) = &exposure {
                        profile = profile.with_exposure(exposure.clone()).unwrap();
                    }
                    Box::new(profile)
                });
                let mut restored = saved.create_with(&factory).unwrap();
                if !ready {
                    // Saving before the first response must retain the selected measure and level.
                    let again = crate::studies::StudyTemplate::from_study(restored.as_ref());
                    assert!(
                        again
                            .inputs
                            .iter()
                            .any(|(id, value)| id == "measure" && *value == InputValue::Choice(1))
                    );
                    assert!(again.inputs.iter().any(|(id, value)| id == "measure_label"
                        && *value == InputValue::Text("Open interest".into())));
                    restored = again.create_with(&factory).unwrap();
                    later_sink.push(changed).unwrap();
                }
                restored.calculate(&bars);
                let plot = restored.volume_profiles(0..bars.len()).remove(0);
                assert_eq!(plot.profile.up(), [if missing == 2 { 10.0 } else { 4.0 }]);
                assert_eq!(plot.profile.down(), [if missing == 2 { 5.0 } else { 2.0 }]);
                assert_eq!(restored.plot_value(1, 0), (missing == 0).then_some(100.0));
                #[cfg(feature = "ui")]
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
    /// A profile saved by an earlier release has no labels. Restored before its chain answers,
    /// its measure position counts among all three measures, whichever feed answers first, and
    /// a measure that the restored chain does not list is saved again as it was.
    #[test]
    fn earlier_saved_measure_keeps_its_position() {
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let time = Utc::now();
        // A call and a put at 100 with volume 10 and 5 and, when reported, open interest 4 and 2.
        let rows = |interest: bool| -> Vec<OptionQuote> {
            [OptionKind::Call, OptionKind::Put]
                .into_iter()
                .map(|kind| {
                    let call = kind == OptionKind::Call;
                    let mut row =
                        OptionQuote::new(format!("100-{kind:?}"), expiry, 100.0, kind, time);
                    row.volume = Some(if call { 10.0 } else { 5.0 });
                    row.open_interest = interest.then_some(if call { 4.0 } else { 2.0 });
                    row.gamma = Some(0.05);
                    row.multiplier = Some(100.0);
                    row.underlying_price = Some(100.0);
                    row
                })
                .collect()
        };
        let answer = |feed: &ChainFeed, interest: Option<bool>, exposure: bool| {
            if exposure {
                let mut row = OptionExposureRow::new(expiry, 100.0);
                (row.calls, row.puts, row.net) = (Some(12.0), Some(-7.0), Some(5.0));
                let snapshot =
                    OptionExposure::new(OptionExposureKind::Gamma, time, "host units", vec![row]);
                let sink = feed.1.lock().unwrap();
                sink.as_ref().unwrap().push(snapshot).unwrap();
            }
            if let Some(interest) = interest {
                let sink = feed.0.lock().unwrap();
                sink.as_ref().unwrap().push(rows(interest)).unwrap();
            }
        };
        let factory = |exposure: bool| {
            let feed = ChainFeed::default();
            let query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
            let chain = OptionChain::new(&feed, query, || {}).unwrap();
            let handle = exposure.then(|| {
                let query =
                    OptionExposureQuery::new("SAMPLE", expiry..=expiry, OptionExposureKind::Gamma);
                OptionExposureHandle::new(&feed, query, || {}).unwrap()
            });
            let mut factory = crate::studies::IndicatorFactory::new();
            factory.register("Options Volume Profile", move || {
                let profile =
                    OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", vec![expiry])
                        .unwrap();
                Box::new(match &handle {
                    Some(handle) => profile.with_exposure(handle.clone()).unwrap(),
                    None => profile,
                })
            });
            (factory, feed)
        };
        // Saved over Volume, Open interest and Gamma exposure: Volume with the provider answering
        // first, after or before (`early`) the restore, and Open interest restored over a chain
        // that lists Volume alone.
        for (exposure, early, position, interest, up) in [
            (true, false, 0, true, 10.0),
            (true, true, 0, true, 10.0),
            (false, false, 1, false, 4.0),
        ] {
            let (original, feed) = factory(exposure);
            answer(&feed, Some(true), exposure);
            let mut study = original.create("Options Volume Profile").unwrap();
            study
                .set_input("measure", InputValue::Choice(position))
                .unwrap();
            let mut saved = crate::studies::StudyTemplate::from_study(study.as_ref());
            saved.inputs.retain(|(id, _)| !id.ends_with("_label"));
            // Restored, read after any provider answer and before the chain's, and saved again.
            let (restoring, feed) = factory(exposure);
            answer(&feed, None, exposure && early);
            let restored = saved.create_with(&restoring).unwrap();
            answer(&feed, None, exposure && !early);
            restored.inputs();
            answer(&feed, Some(interest), false);
            let saved = crate::studies::StudyTemplate::from_study(restored.as_ref());
            let (later, feed) = factory(exposure);
            answer(&feed, Some(true), exposure);
            let restored = saved.create_with(&later).unwrap();
            assert_eq!(
                restored.volume_profiles(0..1)[0].profile.up(),
                [up],
                "position {position}, early {early}"
            );
        }
    }
    /// A measure position the chain does not list names its measure in the order of all three,
    /// so the editor lists the measure that is applied or waits, and a pick applies the measure
    /// it names, also once the chain offers the one that waited.
    #[test]
    fn earlier_saved_measure_applies_as_the_editor_lists_it() {
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let time = Utc::now();
        // A call and a put at 100 with volume 10 and 5, open interest 4 and 2 and gamma 0.05;
        // gamma exposure, scaled to its largest value, draws the call at 1.
        let rows = |put_volume: bool, gamma: bool| {
            [OptionKind::Call, OptionKind::Put].map(|kind| {
                let call = kind == OptionKind::Call;
                let mut row = OptionQuote::new(format!("100-{kind:?}"), expiry, 100.0, kind, time);
                row.volume = (call || put_volume).then_some(if call { 10.0 } else { 5.0 });
                row.open_interest = Some(if call { 4.0 } else { 2.0 });
                row.gamma = gamma.then_some(0.05);
                row.multiplier = Some(100.0);
                row.underlying_price = Some(100.0);
                row
            })
        };
        // Neither chain lists position 2, saved as Gamma exposure among all three. Without the
        // put's volume it lists Open interest and Gamma exposure, which applies at once; without
        // gamma it lists Volume and Open interest, and Gamma exposure waits while Volume shows.
        for (put_volume, gamma, shown) in [(false, true, 1.0), (true, false, 10.0)] {
            let feed = ChainFeed::default();
            let query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
            let chain = OptionChain::new(&feed, query, || {}).unwrap();
            let sink = feed.0.lock().unwrap().clone().unwrap();
            sink.push(rows(put_volume, gamma).to_vec()).unwrap();
            let mut study =
                OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
            study.set_input("measure", InputValue::Choice(2)).unwrap();
            let inputs = study.inputs();
            let input = inputs.iter().find(|input| input.id == "measure").unwrap();
            let InputKind::Choice(choices) = &input.kind else {
                panic!("measure choices")
            };
            let listed = |name: &str| choices.iter().position(|choice| choice == name).unwrap();
            assert_eq!(input.value, InputValue::Choice(listed("Gamma exposure")));
            assert_eq!(
                study.volume_profiles(0..1)[0].profile.up(),
                [shown],
                "gamma {gamma}"
            );
            study
                .set_input("measure", InputValue::Choice(listed("Open interest")))
                .unwrap();
            assert_eq!(
                study.volume_profiles(0..1)[0].profile.up(),
                [4.0],
                "gamma {gamma}"
            );
            // The pick replaces the measure that waited, which does not apply once offered.
            sink.push(rows(true, true).to_vec()).unwrap();
            assert_eq!(
                study.volume_profiles(0..1)[0].profile.up(),
                [4.0],
                "gamma {gamma}, every measure"
            );
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
    /// Daily option snapshots default to daily and longer charts, for either host source.
    #[test]
    fn options_profiles_start_on_daily_intervals() {
        let feed = Feed::default();
        let (chain, dates) = chain_by_expiry();
        let expiry = dates[0];
        let volume = OptionsVolumeProfile::new(
            &feed,
            OptionsVolumeQuery::new("SAMPLE", expiry..=expiry),
            1.0,
            || {},
        )
        .unwrap();
        let chain = OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates).unwrap();
        for profile in [volume, chain] {
            let mut study = crate::studies::ConfiguredStudy::new(Box::new(profile));
            let visibility = study.settings().timeframe_visibility;
            for (interval, shown) in [("5min", false), ("1h", false), ("1D", true), ("1W", true)] {
                assert_eq!(visibility.is_visible_on(interval), shown, "{interval}");
            }
            // The chain belongs to the chart's underlying: no Symbol, Interval or close wait.
            let ids: Vec<_> = study.inputs().into_iter().map(|input| input.id).collect();
            for (id, value) in [
                ("calculation_symbol", InputValue::Symbol("OTHER".into())),
                (
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(crate::model::Timeframe::Week1)),
                ),
                ("wait_for_close", InputValue::Toggle(true)),
            ] {
                assert!(!ids.iter().any(|listed| listed == id), "{id}");
                assert!(study.set_input(id, value).is_err(), "{id}");
            }
        }
    }
    #[cfg(feature = "ui")]
    #[test]
    fn profile_editor_updates_choice_labels_on_edit_and_reset() {
        use crate::ui::dialogs::indicator_dialog::IndicatorSettingsDialog;
        use egui_kittest::{Harness, kittest::Queryable};

        let (chain, sink, mut rows, expiry, _) = expected_move_chain();
        for row in &mut rows {
            row.volume = Some(10.0);
            row.open_interest = Some(4.0);
        }
        sink.push(rows).unwrap();
        let mut profile = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        profile.set_input("expiry", InputValue::Choice(1)).unwrap();
        profile.set_input("measure", InputValue::Choice(1)).unwrap();
        for (theme, width, reset) in [
            (crate::theme::Theme::dark(), 1440.0, false),
            (crate::theme::Theme::light(), 1440.0, true),
            (crate::theme::Theme::dark(), 360.0, true),
            (crate::theme::Theme::light(), 360.0, false),
        ] {
            let mut study = crate::studies::ConfiguredStudy::new(Box::new(profile.clone()));
            let mut dialog = IndicatorSettingsDialog::default();
            dialog.open(crate::widget::SeriesId(2), &study, None);
            let mut edit = None;
            let mut harness = Harness::builder()
                .with_context_setup({
                    let theme = theme.clone();
                    move |ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &theme);
                    }
                })
                .with_size(egui::vec2(width, 900.0))
                .build_ui(|ui| {
                    if let Some(next) = dialog.show(ui.ctx(), &theme) {
                        edit = Some(next);
                    }
                });
            harness.run();
            if reset {
                harness.get_by_label("Reset inputs").click();
                harness.run();
            } else {
                for (input, label) in [("Expiry", "Nearest"), ("Measure", "Volume")] {
                    harness
                        .get_by_role_and_label(egui::Role::ComboBox, input)
                        .click();
                    harness.run();
                    harness.get_by_label(label).click();
                    harness.run();
                }
            }
            harness.get_by_label("Apply").click();
            harness.run();
            drop(harness);
            let edit = edit.expect("an applied profile edit");
            let original = study.inputs();
            for (id, label) in [("expiry_label", "Nearest"), ("measure_label", "Volume")] {
                assert_eq!(
                    original
                        .iter()
                        .find(|input| input.id == id)
                        .unwrap()
                        .default,
                    InputValue::Text(label.into()),
                    "default {id}"
                );
                assert_eq!(
                    edit.inputs.iter().find(|(saved, _)| saved == id).unwrap().1,
                    InputValue::Text(label.into()),
                    "reset {reset}: {id}"
                );
            }
            study.apply_settings(&edit.inputs, edit.settings).unwrap();
            let inputs = study.inputs();
            for id in ["expiry", "measure"] {
                assert_eq!(
                    inputs.iter().find(|input| input.id == id).unwrap().value,
                    InputValue::Choice(0),
                    "reset {reset}: {id}"
                );
            }
        }
    }

    /// Each measure keeps its own Sides choice when the settings editor applies every input with
    /// only the measure changed, and a saved template restores each: Volume and Open interest
    /// draw split bars and Gamma exposure net bars until another choice is made for them.
    #[test]
    fn each_measure_keeps_its_sides_through_the_editor() {
        use InputValue::Choice;
        use ProfileDisplay::{Delta, UpDown};
        let (chain, dates) = chain_by_expiry();
        let factory = profile_factory(chain.clone(), dates.clone());
        let mut study: Box<dyn Indicator> = Box::new(crate::studies::ConfiguredStudy::new(
            Box::new(OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates).unwrap()),
        ));
        let display = |study: &dyn Indicator| study.volume_profiles(0..1)[0].config.display;
        assert_eq!(display(&*study), UpDown);
        // The measures are listed as Volume, Open interest and Gamma exposure.
        for (edits, shown) in [
            (vec![("measure", Choice(2))], Delta),
            (vec![("measure", Choice(0))], UpDown),
            (vec![("volume_sides", Choice(1))], Delta),
            (vec![("measure", Choice(2))], Delta),
            (vec![("measure", Choice(0))], Delta),
            (
                vec![("measure", Choice(2)), ("gamma_sides", Choice(0))],
                UpDown,
            ),
            (vec![("measure", Choice(1))], UpDown),
            (vec![("measure", Choice(0))], Delta),
        ] {
            apply_edits(&mut *study, &edits);
            assert_eq!(display(&*study), shown, "{edits:?}");
        }
        let mut restored = crate::studies::StudyTemplate::from_study(&*study)
            .create_with(&factory)
            .unwrap();
        assert_eq!(display(&*restored), Delta);
        apply_edits(&mut *restored, &[("measure", Choice(2))]);
        assert_eq!(display(&*restored), UpDown);
    }

    /// The put/call readout shows in the legend while its input and its line in the style tab
    /// are on, and a saved template restores both; it is never drawn on the price scale. Net
    /// gamma follows its line the same way.
    #[test]
    fn put_call_readout_follows_its_toggle_and_style_line() {
        let (chain, dates) = chain_by_expiry();
        let factory = profile_factory(chain.clone(), dates.clone());
        let mut study: Box<dyn Indicator> = Box::new(crate::studies::ConfiguredStudy::new(
            Box::new(OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates).unwrap()),
        ));
        study.calculate(&[Bar::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 1.0)]);
        let show_line = |study: &mut dyn Indicator, line: usize, shown: bool| {
            let mut settings = study.settings();
            settings.plots[line].visible = shown;
            study.apply_settings(&[], settings).unwrap();
        };
        let restored = |study: &dyn Indicator| {
            crate::studies::StudyTemplate::from_study(study)
                .create_with(&factory)
                .unwrap()
        };
        assert!(study.legend_visible(0));
        show_line(&mut *study, 0, false);
        assert!(!study.legend_visible(0));
        assert!(!restored(&*study).legend_visible(0));
        show_line(&mut *study, 0, true);
        assert!(study.legend_visible(0));
        assert!(
            !study.plot_style(0).visible,
            "a ratio drawn on the price scale"
        );
        study
            .set_input("put_call", InputValue::Toggle(false))
            .unwrap();
        assert!(!study.legend_visible(0));
        assert!(!restored(&*study).legend_visible(0));
        // Cursor payloads and alerts still read 10 put contracts to 20 call contracts.
        assert_eq!(study.plot_value(0, 0), Some(0.5));
        apply_edits(&mut *study, &[("measure", InputValue::Choice(2))]);
        assert!(study.legend_visible(2));
        assert!(
            !study.plot_style(2).visible,
            "net gamma drawn on the price scale"
        );
        show_line(&mut *study, 2, false);
        assert!(!study.legend_visible(2));
    }

    /// A profile saved while one Sides choice served every measure and its readouts showed with
    /// their lines unticked draws each measure as before: split or net bars as saved, in its
    /// first two line colours, at the right of the plot, with its readout in the legend.
    #[test]
    fn profiles_saved_with_one_sides_choice_draw_as_before() {
        use ProfileDisplay::{Delta, UpDown};
        // Saved over Volume with Net picked and the line colours rgba(1, 2, 3, 200) and
        // rgba(4, 5, 6, 200).
        let saved = r#"{"name":"Options Volume Profile","inputs":[["expiry",{"Choice":0}],["expiry_label",{"Text":"Nearest"}],["sides",{"Choice":1}],["through_date",{"Text":"2026-11-26"}],["measure",{"Choice":0}],["measure_label",{"Text":"Volume"}],["max_pain",{"Toggle":false}],["expected_move",{"Toggle":false}],["move_method",{"Choice":0}],["calculation_timeframe",{"Timeframe":null}],["wait_for_close",{"Toggle":true}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.OptionsVolumeProfile.html","plots":[{"color":[1,2,2,200],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[3,4,5,200],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[27,117,109,180],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":false},{"color":[27,117,109,180],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[169,59,56,180],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[27,117,109,180],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[38,166,154,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true},{"color":[38,166,154,255],"width":1.0,"pattern":"Solid","plot_type":"JoinedLine","visible":true}],"precision":2,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[2,7,7,16],"untagged_plots":[0,1,2,3,4,5,6,7],"price_line_plots":[],"plot_variants":[[],[],[],[],[],[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#;
        let (chain, dates) = chain_by_expiry();
        let factory = profile_factory(chain, dates);
        for (sides, measure, label, display, readout) in [
            (0, 0, "Volume", UpDown, 0),
            (1, 0, "Volume", Delta, 0),
            (0, 2, "Gamma exposure", UpDown, 2),
            (1, 2, "Gamma exposure", Delta, 2),
        ] {
            let mut template: crate::studies::StudyTemplate = serde_json::from_str(saved).unwrap();
            for (id, value) in &mut template.inputs {
                match id.as_str() {
                    "sides" => *value = InputValue::Choice(sides),
                    "measure" => *value = InputValue::Choice(measure),
                    "measure_label" => *value = InputValue::Text(label.into()),
                    _ => {}
                }
            }
            let colours =
                [0, 1].map(|line| template.settings.plots[line].color.to_srgba_unmultiplied());
            let mut restored = template
                .create_with(&factory)
                .unwrap_or_else(|error| panic!("{label}, sides {sides}: {error}"));
            let plot = restored.volume_profiles(0..1).remove(0);
            assert_eq!(
                (
                    plot.config.display,
                    [plot.config.up_color, plot.config.down_color]
                ),
                (display, colours),
                "{label}, sides {sides}"
            );
            assert_eq!((plot.right_aligned, plot.width), (true, 0.9));
            assert!(restored.legend_visible(readout), "{label}");
            assert!(!restored.plot_style(readout).visible, "{label}");
            // Saved before the strike limit, the current price line and the daily interval
            // default: every strike, no price line and every interval.
            let input = |id: &str| {
                let inputs = restored.inputs();
                inputs
                    .into_iter()
                    .find(|input| input.id == id)
                    .map(|input| input.value)
            };
            assert_eq!(input("strikes_around_price"), Some(InputValue::Length(0)));
            assert_eq!(input("current_price"), Some(InputValue::Toggle(false)));
            assert_eq!(input("layout"), Some(InputValue::Choice(0)));
            restored.calculate(&[Bar::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 1.0)]);
            assert_eq!(restored.plot_value(8, 0), None);
            assert!(!restored.legend_visible(8), "{label}");
            let intervals = restored.settings().timeframe_visibility;
            assert!(intervals.is_visible_on(&crate::model::Timeframe::Min5.as_str()));
        }
    }

    /// Each Sides choice draws its display in colours of its own, so that colours chosen for one
    /// display stay when another is shown, and the put/call ratio does not follow the choice.
    /// Calls start blue and puts red, and puts divided by calls leave a strike without calls
    /// empty.
    #[test]
    fn sides_choose_their_display_and_colours() {
        use InputValue::{Choice, Color};
        use ProfileDisplay::{Delta, Total, UpDown};
        let (chain, dates) = chain_by_expiry();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates).unwrap();
        study.calculate(&[Bar::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 1.0)]);
        let sides = |study: &OptionsVolumeProfile| {
            let plot = study.volume_profiles(0..1).remove(0);
            let sides = (plot.config.display != Total)
                .then_some([plot.config.up_color, plot.config.down_color]);
            (plot.config.display, sides, plot.color)
        };
        let blue = Color32::from_rgba_unmultiplied(41, 98, 255, 180).to_srgba_unmultiplied();
        let red = Color32::from_rgba_unmultiplied(239, 83, 80, 180).to_srgba_unmultiplied();
        assert_eq!(sides(&study), (UpDown, Some([blue, red]), blue));
        assert_eq!(study.colors()[3].to_srgba_unmultiplied(), blue, "call wall");
        for (id, color) in [
            (
                "call_color",
                Color32::from_rgba_unmultiplied(41, 98, 255, 180),
            ),
            (
                "put_color",
                Color32::from_rgba_unmultiplied(239, 83, 80, 180),
            ),
            (
                "total_color",
                Color32::from_rgba_unmultiplied(41, 98, 255, 180),
            ),
        ] {
            let input = study
                .inputs()
                .into_iter()
                .find(|input| input.id == id)
                .unwrap();
            assert_eq!(input.default, Color(color));
        }
        let color = |n: u8| Color32::from_rgba_unmultiplied(n, 2 * n, 3 * n, 200);
        let ids = [
            "call_color",
            "put_color",
            "net_positive_color",
            "net_negative_color",
            "total_color",
        ];
        for (id, n) in ids.into_iter().zip(1..) {
            study.set_input(id, Color(color(n))).unwrap();
        }
        let rgba = |n| color(n).to_srgba_unmultiplied();
        // Calls 20 and puts 10 at the nearest expiry. The measures are listed as Volume, Open
        // interest and Gamma exposure.
        for (measure, id, choice, drawn) in [
            (
                0,
                "volume_sides",
                0,
                (UpDown, Some([rgba(1), rgba(2)]), rgba(1)),
            ),
            (
                0,
                "volume_sides",
                1,
                (Delta, Some([rgba(3), rgba(4)]), rgba(3)),
            ),
            (0, "volume_sides", 2, (Total, None, rgba(5))),
            (0, "volume_sides", 3, (Total, None, rgba(5))),
            (0, "volume_sides", 4, (Total, None, rgba(2))),
            (
                2,
                "gamma_sides",
                0,
                (UpDown, Some([rgba(1), rgba(2)]), rgba(1)),
            ),
            (
                2,
                "gamma_sides",
                1,
                (Delta, Some([rgba(3), rgba(4)]), rgba(3)),
            ),
        ] {
            study.set_input("measure", Choice(measure)).unwrap();
            study.set_input(id, Choice(choice)).unwrap();
            assert_eq!(sides(&study), drawn, "{id} {choice}");
            if measure == 0 {
                let plot = study.volume_profiles(0..1).remove(0);
                let rows = [
                    plot.profile.up(),
                    plot.profile.down(),
                    plot.profile.volume(),
                ];
                // Puts divided by calls take the place of the calls, with no puts beside them.
                let drawn = if choice == 4 {
                    [[0.5], [0.0], [0.5]]
                } else {
                    [[20.0], [10.0], [30.0]]
                };
                assert_eq!(rows, drawn, "{id} {choice}");
                assert_eq!(study.plot_value(0, 0), Some(0.5), "{id} {choice}");
            }
        }
        // Split colours chosen later leave the net ones, and a net colour the split ones.
        study.set_input("measure", Choice(0)).unwrap();
        study.set_input("call_color", Color(color(6))).unwrap();
        study.set_input("put_color", Color(color(7))).unwrap();
        study.set_input("volume_sides", Choice(1)).unwrap();
        assert_eq!(sides(&study).1, Some([rgba(3), rgba(4)]));
        study
            .set_input("net_positive_color", Color(color(8)))
            .unwrap();
        study.set_input("volume_sides", Choice(0)).unwrap();
        assert_eq!(sides(&study).1, Some([rgba(6), rgba(7)]));
        // A strike without calls has no ratio, and the readout still divides every put by every
        // call: (5 + 7 + 8) / (10 + 0 + 4).
        let (chain, dates) = chain_with_a_strike_without_calls();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates).unwrap();
        study.calculate(&[Bar::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 1.0)]);
        study.set_input("volume_sides", Choice(4)).unwrap();
        let profile = study.volume_profiles(0..1).remove(0).profile;
        assert_eq!([profile.low(), profile.high()], [97.5, 112.5]);
        assert_eq!(profile.up(), [0.5, 0.0, 2.0]);
        assert_eq!(study.plot_value(0, 0), Some(20.0 / 14.0));
    }

    /// The call and put walls of volume and open interest are the strikes with the largest call
    /// and put figure of the measure, the lower strike on a tie, and a side without any figure
    /// has none.
    #[test]
    fn volume_and_interest_walls_follow_their_measure() {
        let feed = ChainFeed::default();
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let sink = feed.0.lock().unwrap().clone().unwrap();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        study.calculate(&[Bar::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 1.0)]);
        // Volume and open interest of the calls and of the puts at 90, 100 and 110. The chain
        // lists Volume and Open interest.
        let calls = [[10.0, 30.0, 20.0], [7.0, 2.0, 9.0]];
        let puts = [[25.0, 5.0, 5.0], [1.0, 8.0, 3.0]];
        let tied = [[10.0, 30.0, 30.0], [1.0; 3]];
        let none = [[0.0; 3], [1.0; 3]];
        for (calls, puts, measure, walls) in [
            (calls, puts, 0, [Some(100.0), Some(90.0)]),
            (calls, puts, 1, [Some(110.0), Some(100.0)]),
            (tied, none, 0, [Some(100.0), None]),
        ] {
            let rows = [90.0, 100.0, 110.0]
                .into_iter()
                .enumerate()
                .flat_map(|(index, strike)| {
                    [(OptionKind::Call, calls), (OptionKind::Put, puts)].map(
                        |(kind, [volume, interest])| {
                            let symbol = format!("{strike}-{kind:?}");
                            let mut row =
                                OptionQuote::new(symbol, expiry, strike, kind, Utc::now());
                            row.volume = Some(volume[index]);
                            row.open_interest = Some(interest[index]);
                            row
                        },
                    )
                })
                .collect();
            sink.push(rows).unwrap();
            study
                .set_input("measure", InputValue::Choice(measure))
                .unwrap();
            for id in ["call_wall", "put_wall"] {
                study.set_input(id, InputValue::Toggle(true)).unwrap();
            }
            let drawn = [study.plot_value(3, 0), study.plot_value(4, 0)];
            assert_eq!(drawn, walls, "measure {measure}");
            assert!(study.legend_visible(3), "measure {measure}");
        }
        study
            .set_input("call_wall", InputValue::Toggle(false))
            .unwrap();
        assert_eq!(study.plot_value(3, 0), None);
        assert!(!study.legend_visible(3));
        // A saved measure the chain cannot draw yet waits, and the volume drawn in its place
        // shows no walls, as a profile saved before volume had walls drew none.
        study
            .set_input("call_wall", InputValue::Toggle(true))
            .unwrap();
        study
            .set_input("measure_label", InputValue::Text("Gamma exposure".into()))
            .unwrap();
        assert_eq!(study.plot_value(3, 0), None);
        assert!(!study.legend_visible(3));
    }

    /// Each Sides display paints every strike in its own colours at one opacity, with no line
    /// across the profile, and with Values on, the walls are named beside their rows, clear of
    /// every figure, at either placement.
    #[cfg(feature = "widget")]
    #[test]
    fn options_rows_keep_their_colours_and_walls_clear_their_figures() {
        use InputValue::{Choice, Toggle};
        use egui::{Rect, Shape};
        let (chain, dates) = chain_with_volumes(&[
            (100.0, [20.0, 10.0]),
            (105.0, [1_499.0, 40.0]),
            (110.0, [5.0, 1_234_567.0]),
        ]);
        // The row at 110 holds 1,234,572 of 1,236,141, more than the 70% value area.
        let tone = |[r, g, b, a]: [u8; 4]| {
            Color32::from_rgba_unmultiplied(r, g, b, a).gamma_multiply(0.35)
        };
        let blue = tone([41, 98, 255, 180]);
        let red = tone([239, 83, 80, 180]);
        let teal = tone([38, 166, 154, 180]);
        let [r, g, b, _] = EXPIRY_COLORS[0].to_srgba_unmultiplied();
        let expiry = tone([r, g, b, 180]);
        for (sides, fills) in [
            (0, vec![blue, red]),
            (1, vec![teal, red]),
            (2, vec![blue]),
            (3, vec![expiry]),
            (4, vec![red]),
        ] {
            for left in [false, true] {
                for strikes in [[Some(105.0), Some(110.0)], [Some(110.0); 2]] {
                    let case = format!("sides {sides}, left {left}, walls {strikes:?}");
                    let mut study =
                        OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", dates.clone())
                            .unwrap();
                    for (id, value) in [
                        ("volume_sides", Choice(sides)),
                        ("placement", Choice(usize::from(left))),
                        ("show_values", Toggle(true)),
                    ] {
                        study.set_input(id, value).unwrap();
                    }
                    study.calculate(&[Bar::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 1.0)]);
                    let plot = study.volume_profiles(0..1).remove(0);
                    let wall_colors = [3, 4].map(|line| study.plot_style(line).color);
                    let mut profile = (*plot.profile).clone();
                    profile.walls = Some(OptionWalls {
                        strikes,
                        colors: wall_colors.map(|color| color.to_srgba_unmultiplied()),
                    });
                    let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
                        crate::painting::volume_profile::paint_profile(
                            ui.painter(),
                            &profile,
                            plot.config,
                            plot.color,
                            100.0..400.0,
                            crate::painting::volume_profile::ProfileStyle::Study {
                                right_aligned: plot.right_aligned,
                                strip: false,
                                walls_visible: [true; 2],
                                width: plot.width,
                                volume_decimals: None,
                            },
                            &|price| (1200.0 - 10.0 * price) as f32,
                        );
                    });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let rows: Vec<_> = shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Rect(row) if row.fill.a() < 255 => Some(row.fill),
                            _ => None,
                        })
                        .collect();
                    assert!(!rows.is_empty(), "{case}");
                    assert!(
                        rows.iter().all(|fill| fills.contains(fill)),
                        "{case}: {rows:?}"
                    );
                    assert!(
                        !shapes.iter().any(|shape| matches!(
                            &shape.shape,
                            Shape::LineSegment { .. } | Shape::Path(_)
                        )),
                        "{case}: no line across the profile"
                    );
                    let (walls, figures): (Vec<_>, Vec<_>) = shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Text(text) => Some((
                                wall_colors.contains(&text.fallback_color),
                                Rect::from_min_size(text.pos, text.galley.size()),
                            )),
                            _ => None,
                        })
                        .partition(|(wall, _)| *wall);
                    assert_eq!(walls.len(), 2, "{case}");
                    for (_, wall) in &walls {
                        assert!(
                            figures.iter().all(|(_, figure)| !wall.intersects(*figure)),
                            "{case}: wall {wall:?} clears {figures:?}"
                        );
                    }
                    assert!(
                        !walls[0].1.intersects(walls[1].1),
                        "{case}: walls clear each other"
                    );
                }
            }
        }
    }

    /// An options profile's rows start at the right or the left edge of the plot, the widest
    /// taking the chosen share of the strip of 30% of the plot, with calls and puts side by side,
    /// their difference or their sum, by expiration in segments that add up to the row and are
    /// named in a key. Net gamma bars take half that share on each side of the strip's centre.
    /// Values writes each volume row's figures beside it.
    #[cfg(feature = "widget")]
    #[test]
    fn options_rows_paint_at_their_place_width_and_expiry() {
        use crate::scales::{PriceMap, PriceRange, PriceTransform};
        use InputValue::{Choice, Number, Toggle};
        let (chain, dates) = chain_by_expiry();
        let bars = vec![Bar::new(Utc::now(), 100.0, 101.0, 99.0, 100.0, 1.0); 2];
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 200.0));
        // The row at 100 is 100 tall, room for a key.
        let range = PriceRange::new(99.0, 101.0);
        let map = PriceMap::new(PriceTransform::default(), range, 0.0, 200.0, false);
        let coords = crate::chart::coords::ChartMapping::new(rect, 30.0, 0, 1, 0.0, map);
        // The strip is 240 wide, so the widest row takes 216 at 27% of the plot and 80 at 10%.
        // At the nearest expiry calls are 20 and puts 10; through both, 25 and 15, or 30 at the
        // first expiry and 10 at the second.
        for (measure, sides, through, left, width, rows) in [
            (0, 0, false, false, 27.0, vec![656.0..800.0, 584.0..656.0]),
            (0, 0, false, true, 27.0, vec![0.0..144.0, 144.0..216.0]),
            (
                0,
                0,
                false,
                true,
                10.0,
                vec![0.0..160.0 / 3.0, 160.0 / 3.0..80.0],
            ),
            (0, 1, false, false, 27.0, vec![728.0..800.0]),
            (0, 2, false, false, 27.0, vec![584.0..800.0]),
            (0, 2, true, false, 27.0, vec![584.0..800.0]),
            (0, 3, true, false, 27.0, vec![638.0..800.0, 584.0..638.0]),
            // Net gamma of +1000, the largest, from the strip's centre at 120.
            (2, 1, false, true, 10.0, vec![120.0..160.0]),
        ] {
            let case = format!(
                "measure {measure}, sides {sides}, through {through}, left {left}, width {width}"
            );
            let mut profile =
                OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", dates.clone()).unwrap();
            let sides_id = if measure == 2 {
                "gamma_sides"
            } else {
                "volume_sides"
            };
            for (id, value) in [
                ("measure", Choice(measure)),
                (sides_id, Choice(sides)),
                ("expiry", Choice(if through { 3 } else { 0 })),
                ("placement", Choice(usize::from(left))),
                ("profile_width", Number(width)),
            ] {
                profile.set_input(id, value).unwrap();
            }
            let mut study = crate::studies::ConfiguredStudy::new(Box::new(profile));
            study.calculate(&bars);
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
            // Rows are translucent; the key's swatches are not.
            let drawn: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill.a() < 255 => Some(rect.rect.x_range()),
                    _ => None,
                })
                .collect();
            assert_eq!(drawn.len(), rows.len(), "{case}: {drawn:?}");
            for (drawn, row) in drawn.iter().zip(&rows) {
                assert!(
                    (drawn.min - row.start).abs() < 1e-3 && (drawn.max - row.end).abs() < 1e-3,
                    "{case}: {drawn:?}"
                );
            }
            let texts: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            let named = dates.iter().all(|date| texts.contains(&date.to_string()));
            assert_eq!(named, sides == 3, "{case}: {texts:?}");
        }
        // An options profile without an edge strip stands beside bars 2 to 4. At spacing
        // 30 its span is 90 wide.
        let mut profile =
            OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", dates.clone()).unwrap();
        profile.set_input("volume_sides", Choice(2)).unwrap();
        let mut plot = profile.volume_profiles(0..1).remove(0);
        plot.bars = 2..5;
        plot.strip = None;
        let coords = crate::chart::coords::ChartMapping::new(rect, 30.0, 0, 9, 0.0, map);
        let span = crate::chart::rendering::volume_profile::profile_span(&plot, &coords);
        assert_eq!(span, coords.idx_to_x(2) - 15.0..coords.idx_to_x(4) + 15.0);
        // Calls × puts as whole counts or with K and M from the lowest strike up, or puts
        // divided by calls, which a strike without calls lacks; nothing beside gamma exposure.
        // Rows are 50 tall.
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(95.0, 115.0),
            0.0,
            200.0,
            false,
        );
        let coords = crate::chart::coords::ChartMapping::new(rect, 30.0, 0, 1, 0.0, map);
        let (strikes, listed) = chain_with_a_strike_without_calls();
        let (counts, count_dates) = chain_with_volumes(&[
            (100.0, [20.0, 10.0]),
            (105.0, [1_499.0, 40.0]),
            (110.0, [5.0, 1_234_567.0]),
        ]);
        for (chain, dates, edit, written) in [
            (
                counts,
                count_dates,
                ("volume_sides", Choice(0)),
                vec!["20 × 10", "1.5K × 40", "5 × 1.23M"],
            ),
            (
                strikes.clone(),
                listed.clone(),
                ("volume_sides", Choice(0)),
                vec!["10 × 5", "0 × 7", "4 × 8"],
            ),
            (
                strikes,
                listed,
                ("volume_sides", Choice(4)),
                vec!["0.50", "2.00"],
            ),
            (chain, dates, ("measure", Choice(2)), Vec::new()),
        ] {
            let mut profile = OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates).unwrap();
            for (id, value) in [edit.clone(), ("show_values", Toggle(true))] {
                profile.set_input(id, value).unwrap();
            }
            let mut study = crate::studies::ConfiguredStudy::new(Box::new(profile));
            study.calculate(&bars);
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
            let texts: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            assert_eq!(texts, written, "{edit:?}");
        }
    }

    /// Under Through date, Maximum expirations sums the first listed expiries that have not
    /// expired, up to the date; at 0 it sums every listed expiry up to the date, as before.
    #[test]
    fn maximum_expirations_keep_the_first_listed_that_have_not_expired() {
        let dates = [(11, 6), (11, 13), (11, 20), (11, 27), (12, 4)]
            .map(|(month, day)| NaiveDate::from_ymd_opt(2099, month, day).unwrap());
        let historical = NaiveDate::from_ymd_opt(2099, 11, 10)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        // Call volume 1, 2, 4, 8 and 16 in the five weeks; a historical chain on 10 November
        // has 6 November expired.
        for (maximum, through, as_of, up) in [
            (0, 4, None, 31.0),
            (2, 4, None, 3.0),
            (2, 1, None, 3.0),
            (2, 0, None, 1.0),
            (9, 4, None, 31.0),
            (2, 4, Some(historical), 6.0),
            (0, 4, Some(historical), 31.0),
        ] {
            let feed = ChainFeed::default();
            let mut query = OptionChainQuery::new("SAMPLE", dates[0]..=dates[4]);
            query.as_of = as_of;
            let chain = OptionChain::new(&feed, query, || {}).unwrap();
            let time = as_of.map_or_else(Utc::now, |time| time - chrono::TimeDelta::hours(1));
            let rows = dates
                .iter()
                .zip([1.0, 2.0, 4.0, 8.0, 16.0])
                .map(|(&expiry, volume)| {
                    let symbol = expiry.to_string();
                    let mut row = OptionQuote::new(symbol, expiry, 100.0, OptionKind::Call, time);
                    row.volume = Some(volume);
                    row
                })
                .collect();
            feed.0.lock().unwrap().as_ref().unwrap().push(rows).unwrap();
            let mut study =
                OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates.to_vec()).unwrap();
            for (id, value) in [
                ("expiry", InputValue::Choice(6)),
                ("through_date", InputValue::Text(dates[through].to_string())),
                ("max_expiries", InputValue::Length(maximum)),
            ] {
                study.set_input(id, value).unwrap();
            }
            assert_eq!(
                study.volume_profiles(0..1)[0].profile.up(),
                [up],
                "maximum {maximum}, through {}, as of {as_of:?}",
                dates[through]
            );
        }
    }

    /// Upcoming expirations stand at their dates in the time scale's whitespace, with shared
    /// rows, independent walls and labels, and grouping by calendar period.
    #[test]
    fn expirations_draw_beside_their_dates() {
        use crate::model::Timeframe;
        use InputValue::{Choice, Length, Toggle};
        let dates = [(11, 6), (11, 13), (11, 20), (11, 27), (12, 4)]
            .map(|(month, day)| NaiveDate::from_ymd_opt(2099, month, day).unwrap());
        let historical = dates[0]
            .with_day(10)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        let feed = ChainFeed::default();
        let mut query = OptionChainQuery::new("SAMPLE", dates[0]..=dates[4]);
        query.as_of = Some(historical);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let rows = dates
            .iter()
            .zip([1.0, 2.0, 4.0, 8.0, 16.0])
            .map(|(&expiry, volume)| {
                let mut row = OptionQuote::new(
                    expiry.to_string(),
                    expiry,
                    100.0,
                    OptionKind::Call,
                    historical,
                );
                row.volume = Some(volume);
                row
            })
            .collect();
        feed.0.lock().unwrap().as_ref().unwrap().push(rows).unwrap();
        let bars: Vec<_> = (1..=10)
            .map(|day| {
                Bar::new(
                    dates[0]
                        .with_day(day)
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc(),
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    1.0,
                )
            })
            .collect();
        // From November 1 at index 0: November 13, 20, 27 and December 4 map to
        // indices 12, 19, 26 and 33. Month grouping sums November's 2 + 4 + 8 = 14.
        for (left, grouping, maximum, visible, expected) in [
            (
                false,
                0,
                10,
                0..60,
                vec![
                    (12..19, 2.0, 1),
                    (19..26, 4.0, 2),
                    (26..33, 8.0, 3),
                    (33..40, 16.0, 4),
                ],
            ),
            (
                true,
                0,
                10,
                0..60,
                vec![
                    (6..13, 2.0, 1),
                    (13..20, 4.0, 2),
                    (20..27, 8.0, 3),
                    (27..34, 16.0, 4),
                ],
            ),
            (false, 0, 2, 0..60, vec![(12..19, 2.0, 1), (19..26, 4.0, 2)]),
            (
                false,
                2,
                10,
                0..60,
                vec![(26..33, 14.0, 3), (33..40, 16.0, 4)],
            ),
            (
                false,
                1,
                10,
                0..60,
                vec![
                    (12..19, 2.0, 1),
                    (19..26, 4.0, 2),
                    (26..33, 8.0, 3),
                    (33..40, 16.0, 4),
                ],
            ),
            (
                false,
                0,
                10,
                0..20,
                vec![(12..19, 2.0, 1), (19..26, 4.0, 2)],
            ),
            (false, 0, 1, 0..60, vec![(12..32, 2.0, 1)]),
        ] {
            let mut study =
                OptionsVolumeProfile::from_chain(chain.clone(), "SAMPLE", dates.to_vec()).unwrap();
            study.set_timeframe(Timeframe::Day1);
            for (id, value) in [
                ("layout", Choice(1)),
                ("placement", Choice(usize::from(left))),
                ("grouping", Choice(grouping)),
                ("expirations", Length(maximum)),
                ("call_wall", Toggle(true)),
            ] {
                study.set_input(id, value).unwrap();
            }
            assert!(study.volume_profiles(0..60).is_empty(), "no bars");
            study.calculate(&bars);
            let plots = study.volume_profiles(visible.clone());
            assert_eq!(
                plots.len(),
                expected.len(),
                "{left}, {grouping}, {maximum}, {visible:?}"
            );
            for (plot, (span, volume, position)) in plots.iter().zip(expected) {
                assert_eq!(plot.bars, span);
                assert_eq!(plot.right_aligned, left);
                assert_eq!(plot.strip, None);
                assert_eq!(plot.width, 0.9);
                assert_eq!(plot.profile.up(), [volume]);
                assert_eq!(
                    plot.profile.walls.as_ref().unwrap().strikes,
                    [Some(100.0), None]
                );
                assert_eq!(
                    plot.profile.expiry_tag,
                    Some((
                        dates[position],
                        EXPIRY_COLORS[position].to_srgba_unmultiplied()
                    ))
                );
            }
            #[cfg(feature = "widget")]
            for sides in [0, 4] {
                // Calls with no puts have a zero ratio row, but still a call wall.
                let mut displayed = study.clone();
                displayed.set_input("volume_sides", Choice(sides)).unwrap();
                for plot in displayed.volume_profiles(0..60) {
                    let mut output =
                        egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                            crate::painting::volume_profile::paint_profile(
                                ui.painter(),
                                &plot.profile,
                                plot.config,
                                plot.color,
                                100.0..200.0,
                                crate::painting::volume_profile::ProfileStyle::Study {
                                    right_aligned: left,
                                    strip: false,
                                    walls_visible: [true; 2],
                                    width: 0.9,
                                    volume_decimals: None,
                                },
                                &|price| price as f32,
                            );
                        });
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    let label = shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == "Call wall" => {
                                Some(text)
                            }
                            _ => None,
                        })
                        .expect("the profile's own wall is labelled");
                    let label_end = if left {
                        label.pos.x + label.galley.size().x
                    } else {
                        label.pos.x
                    };
                    // The widest bar reaches 90; the zero ratio has none. The label clears it by 4.
                    let expected = match (left, sides) {
                        (true, 0) => 106.0,
                        (false, 0) => 194.0,
                        (true, _) => 196.0,
                        (false, _) => 104.0,
                    };
                    assert!((label_end - expected).abs() < 1e-3);
                    assert_eq!(
                        label.fallback_color,
                        study.plot_style(3).color,
                        "labelled in the call wall colour"
                    );
                    assert!(!shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == "Put wall")));
                    assert!(
                        !shapes
                            .iter()
                            .any(|shape| matches!(&shape.shape, egui::Shape::LineSegment { .. }))
                    );
                }
            }
            #[cfg(feature = "widget")]
            {
                use crate::scales::{PriceRange, PriceTransform};
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 200.0));
                let map = crate::scales::PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(95.0, 105.0),
                    0.0,
                    200.0,
                    false,
                );
                let coords = crate::chart::coords::ChartMapping::new(rect, 10.0, 0, 60, 0.0, map);
                let mut configured = crate::studies::ConfiguredStudy::new(Box::new(study.clone()));
                for shown in [false, true] {
                    let mut settings = configured.settings();
                    settings.plots[3].visible = shown;
                    configured.apply_settings(&[], settings).unwrap();
                    let output = egui::Context::default().run_ui(Default::default(), |ui| {
                        crate::chart::rendering::IndicatorRenderer::render_study(
                            &crate::chart::rendering::RenderContext::new(ui.painter(), rect),
                            &configured,
                            &bars,
                            0,
                            &map,
                            &coords,
                        );
                    });
                    let labelled = output.shapes.iter().any(|shape| {
                        matches!(
                            &shape.shape,
                            egui::Shape::Text(text) if text.galley.text() == "Call wall"
                        )
                    });
                    output.drop_without_applying_deltas();
                    assert_eq!(labelled, shown, "the wall follows its plot's visibility");
                }
                let color = Color32::from_rgb(210, 80, 40);
                let mut settings = configured.settings();
                assert_ne!(settings.plots[3].color, color);
                settings.plots[3].color = color;
                configured.apply_settings(&[], settings).unwrap();
                let recolored = configured.volume_profiles(visible.clone());
                assert_eq!(recolored.len(), plots.len());
                for plot in recolored {
                    assert_eq!(
                        plot.profile.walls.as_ref().unwrap().colors[0],
                        color.to_srgba_unmultiplied(),
                        "dated walls follow the edited call wall colour"
                    );
                }
            }
            assert_eq!(study.plot_value(3, 0), None);
            assert!(!study.legend_visible(3));
            study.set_input("expiry_labels", Toggle(false)).unwrap();
            assert!(
                study
                    .volume_profiles(0..60)
                    .iter()
                    .all(|plot| plot.profile.expiry_tag.is_none())
            );
            // History loaded to the left shifts every placement by one bar.
            let mut longer = bars.clone();
            longer.insert(
                0,
                Bar::new(
                    bars[0].time - chrono::TimeDelta::days(1),
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    1.0,
                ),
            );
            study.calculate(&longer);
            let shifted = study.volume_profiles(0..60);
            assert_eq!(shifted[0].bars.start, plots[0].bars.start + 1);
        }
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", dates.to_vec()).unwrap();
        study.calculate(&bars);
        let edge = study.volume_profiles(0..60).remove(0);
        assert_eq!(edge.bars, 0..60);
        assert_eq!(edge.strip, Some(0.3));
        // With one monthly bar on November 1, November 13 rounds to index 0; November
        // 20, 27 and December 4 round to index 1. Merge 4 + 8 + 16 there.
        study.set_timeframe(Timeframe::Month1);
        study.set_input("layout", Choice(1)).unwrap();
        study.calculate(&[bars[0].clone()]);
        let merged = study.volume_profiles(0..60);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].profile.up(), [2.0]);
        assert_eq!(merged[1].profile.up(), [28.0]);
    }

    /// Strikes around price draws the listed strikes at the price the expected move uses and the
    /// given number nearest below and above it, and follows that price, while the walls and the
    /// put/call ratio still count every selected strike.
    #[test]
    fn strikes_around_price_follow_the_underlying() {
        let feed = ChainFeed::default();
        let expiry = Utc::now().date_naive().succ_opt().unwrap();
        let query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let sink = feed.0.lock().unwrap().clone().unwrap();
        // Call and put volume 9 and 1 at 50, 1 and 1 at 90, 2 and 1 at 100, 1 and 1 at 110 and
        // 1 and 8 at 150, one contract open of each, in rows 10 apart.
        let rows = |underlying: Option<f64>| -> Vec<OptionQuote> {
            [50.0, 90.0, 100.0, 110.0, 150.0]
                .into_iter()
                .zip([[9.0, 1.0], [1.0, 1.0], [2.0, 1.0], [1.0, 1.0], [1.0, 8.0]])
                .flat_map(|(strike, volumes)| {
                    [OptionKind::Call, OptionKind::Put]
                        .into_iter()
                        .zip(volumes)
                        .map(move |(kind, volume)| {
                            let symbol = format!("{strike}-{kind:?}");
                            let mut row =
                                OptionQuote::new(symbol, expiry, strike, kind, Utc::now());
                            row.volume = Some(volume);
                            row.open_interest = Some(1.0);
                            row.gamma = Some(0.05);
                            row.multiplier = Some(100.0);
                            row.underlying_price = underlying;
                            row
                        })
                })
                .collect()
        };
        sink.push(rows(None)).unwrap();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        for id in ["call_wall", "put_wall"] {
            study.set_input(id, InputValue::Toggle(true)).unwrap();
        }
        let every = [9.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        // Without a supplied underlying price, the last close stands in; a new close alone moves
        // the rows.
        for (around, close, bounds, up) in [
            (0, Some(100.0), [45.0, 155.0], &every[..]),
            (1, Some(100.0), [85.0, 115.0], &[1.0, 2.0, 1.0][..]),
            (
                1,
                Some(112.0),
                [105.0, 155.0],
                &[1.0, 0.0, 0.0, 0.0, 1.0][..],
            ),
            // A limit past the listed strikes draws every strike.
            (usize::MAX, Some(100.0), [45.0, 155.0], &every[..]),
            (1, None, [45.0, 155.0], &every[..]),
        ] {
            if study.inputs().iter().any(|input| {
                input.id == "strikes_around_price" && input.value != InputValue::Length(around)
            }) {
                study
                    .set_input("strikes_around_price", InputValue::Length(around))
                    .unwrap();
            }
            let bars: Vec<_> = close
                .map(|close| Bar::new(Utc::now(), close, close, close, close, 1.0))
                .into_iter()
                .collect();
            study.calculate(&bars);
            let profile = study.volume_profiles(0..1).remove(0).profile;
            let case = format!("{around} around {close:?}");
            assert_eq!([profile.low(), profile.high()], bounds, "{case}");
            assert_eq!(profile.up(), up, "{case}");
            if close.is_some() {
                let walls = [study.plot_value(3, 0), study.plot_value(4, 0)];
                assert_eq!(walls, [Some(50.0), Some(150.0)], "{case}");
                assert_eq!(study.plot_value(0, 0), Some(12.0 / 14.0), "{case}");
            }
        }
        // The current price line marks the price the strikes are drawn around, the supplied
        // underlying price before the last close, until its switch hides it.
        study.calculate(&[Bar::new(Utc::now(), 112.0, 112.0, 112.0, 112.0, 1.0)]);
        assert_eq!(study.plot_value(8, 0), Some(112.0));
        sink.push(rows(Some(100.0))).unwrap();
        assert_eq!(study.plot_value(8, 0), Some(100.0));
        assert!(study.legend_visible(8));
        study
            .set_input("current_price", InputValue::Toggle(false))
            .unwrap();
        assert_eq!(study.plot_value(8, 0), None);
        assert!(!study.legend_visible(8));
        // Gamma exposure rows follow the underlying price the quotes supply.
        sink.push(rows(Some(100.0))).unwrap();
        study.set_input("measure", InputValue::Choice(2)).unwrap();
        for (underlying, strikes) in [
            (100.0, vec![90.0, 100.0, 110.0]),
            (112.0, vec![110.0, 150.0]),
        ] {
            sink.push(rows(Some(underlying))).unwrap();
            let plot = study.volume_profiles(0..1).remove(0);
            let drawn: Vec<_> = plot
                .profile
                .exposure
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|row| row.strike)
                .collect();
            assert_eq!(drawn, strikes, "underlying {underlying}");
        }
        // A new profile draws 10 strikes on each side of the price: 5 to 25 of the strikes 1 to
        // 30 around 15, in rows 1 high. A larger saved limit restores as the most it takes.
        let feed = ChainFeed::default();
        let query = OptionChainQuery::new("SAMPLE", expiry..=expiry);
        let chain = OptionChain::new(&feed, query, || {}).unwrap();
        let quotes = (1..=30)
            .map(|strike| {
                let strike = f64::from(strike);
                let symbol = strike.to_string();
                let mut row =
                    OptionQuote::new(symbol, expiry, strike, OptionKind::Call, Utc::now());
                row.volume = Some(1.0);
                row.underlying_price = Some(15.0);
                row
            })
            .collect();
        feed.0
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .push(quotes)
            .unwrap();
        let mut study = OptionsVolumeProfile::from_chain(chain, "SAMPLE", vec![expiry]).unwrap();
        let profile = study.volume_profiles(0..1).remove(0).profile;
        assert_eq!([profile.low(), profile.high()], [4.5, 25.5]);
        study
            .set_input("strikes_around_price", InputValue::Length(5_000))
            .unwrap();
        assert!(study.inputs().iter().any(|input| {
            input.id == "strikes_around_price" && input.value == InputValue::Length(1_000)
        }));
    }
}

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
    supplied: Option<Arc<OptionExposure>>,
    exposure: Option<Arc<ExposureProfile>>,
    walls: [Option<f64>; 2],
    net: Option<f64>,
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

/// Call and put volume, open interest or signed gamma exposure by strike.
/// Hosts supply complete snapshots through the volume-only constructor or [`Self::from_chain`].
/// Clones share the subscription; dropping its last clone makes the feed's sink inactive.
#[derive(Clone)]
pub struct OptionsVolumeProfile {
    source: ProfileSource,
    expiries: Expiries,
    through_date: NaiveDate,
    open_interest: bool,
    gamma: bool,
    gamma_split: bool,
    exposure: Option<OptionExposureHandle>,
    exposure_levels: [bool; 3],
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
            gamma: false,
            gamma_split: false,
            exposure: None,
            exposure_levels: [false; 3],
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
    /// Volume and interest need every selected contract's figure; missing values are never zero.
    /// Max pain needs every selected contract's open interest and multiplier. It minimises
    /// their combined intrinsic payout over listed strikes, choosing the lower strike on ties.
    /// Rows are as tall as the largest common divisor of the selected strikes, so each strike
    /// is a row centre, and coarser only where the 10,000-row limit requires.
    /// Gamma uses [`OptionExposure::gamma`], with net bars by default and optional call and
    /// put walls. Missing contracts are counted in the readout. Attach a provider observation
    /// with [`Self::with_exposure`]; its signs and net values take precedence, even if empty.
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
            gamma: false,
            gamma_split: false,
            exposure: None,
            exposure_levels: [false; 3],
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
        self.gamma
            || (!cache.volume_available && !cache.interest_available && cache.exposure.is_some())
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
                let supplied = self
                    .exposure
                    .as_ref()
                    .and_then(OptionExposureHandle::snapshot);
                let same_exposure = match (&supplied, &cache.supplied) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                };
                if Arc::ptr_eq(&rows, &cache.chain_rows) && cache.day == Some(day) && same_exposure
                {
                    return;
                }
                let nearest = expiries.iter().copied().find(|date| *date >= day);
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
                    let rows = aggregate_exposure(&snapshot.rows)?;
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
                            .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.total_cmp(&a.1)))
                            .map(|(_, strike)| strike)
                    })
                });
                cache.supplied = supplied;
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

// Expiries share strike bars, but an unknown side in any expiry keeps that sum unknown.
fn aggregate_exposure(rows: &[OptionExposureRow]) -> Option<Vec<StrikeExposure>> {
    let mut ordered: Vec<_> = rows.iter().collect();
    ordered.sort_by(|a, b| a.strike.total_cmp(&b.strike));
    let mut strikes: Vec<StrikeExposure> = Vec::new();
    for row in ordered {
        if let Some(previous) = strikes.last_mut()
            && previous.strike == row.strike
        {
            previous.calls = previous.calls.zip(row.calls).map(|(a, b)| a + b);
            previous.puts = previous.puts.zip(row.puts).map(|(a, b)| a + b);
            previous.net = previous.net.zip(row.net).map(|(a, b)| a + b);
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
            ]
        } else {
            Vec::new()
        }
    }
    fn plot_style(&self, line: usize) -> PlotStyle {
        PlotStyle {
            visible: line == 1 || (3..=5).contains(&line),
            color: self.colors[usize::from(line == 1 || line == 4)],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Line,
        }
    }
    fn legend_visible(&self, line: usize) -> bool {
        self.refresh();
        if self.gamma_selected(&self.cache.borrow()) {
            line == 2 || (3..=5).contains(&line) && self.exposure_levels[line - 3]
        } else {
            line == 0 || (line == 1 && self.max_pain)
        }
    }
    fn plot_value(&self, line: usize, bar: usize) -> Option<f64> {
        if !self.visible || !self.matching_instrument || bar >= self.values.len() {
            return None;
        }
        self.refresh();
        let cache = self.cache.borrow();
        if self.gamma_selected(&cache) {
            return match line {
                2 => cache.net,
                3 | 4 if self.exposure_levels[line - 3] => cache.walls[line - 3],
                5 if self.exposure_levels[2] => cache
                    .exposure
                    .as_ref()
                    .and_then(|profile| profile.snapshot.zero_gamma),
                _ => None,
            };
        }
        match line {
            0 => cache.ratio,
            1 if self.max_pain => cache.pain,
            _ => None,
        }
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
            || self.gamma
        {
            measures.push("Gamma exposure".into());
        }
        if !measures.is_empty() {
            let selected = if gamma {
                measures.len() - 1
            } else {
                usize::from(
                    self.open_interest && cache.volume_available && cache.interest_available,
                )
            };
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
                            InputValue::Toggle(self.exposure_levels[index]),
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
                self.gamma = choices[choice] == "Gamma exposure";
            }
            ("sides", InputValue::Choice(choice)) => {
                self.gamma_split = choice == 0;
                self.config.display = if choice == 0 {
                    ProfileDisplay::UpDown
                } else {
                    ProfileDisplay::Delta
                }
            }
            ("max_pain", InputValue::Toggle(enabled)) => self.max_pain = enabled,
            ("call_wall", InputValue::Toggle(enabled)) => self.exposure_levels[0] = enabled,
            ("put_wall", InputValue::Toggle(enabled)) => self.exposure_levels[1] = enabled,
            ("zero_gamma", InputValue::Toggle(enabled)) => self.exposure_levels[2] = enabled,
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
                    let exposure = plot.profile.exposure().unwrap();
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
                            .exposure()
                            .unwrap()
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
                        .exposure()
                        .unwrap()
                        .rows[0]
                        .net,
                    Some(13.0)
                );
                assert_eq!(study.plot_value(5, 0), Some(0.0));
                snapshot.rows.clear();
                snapshot.zero_gamma = None;
                exposure_sink.push(snapshot).unwrap();
            } else {
                let mut later = rows.clone();
                later.iter_mut().for_each(|row| row.expiry = next);
                rows.extend(later);
                sink.push(rows).unwrap();
                study.set_input("expiry", InputValue::Choice(3)).unwrap();
                let plot = study.volume_profiles(0..1).remove(0);
                assert_eq!(plot.profile.exposure().unwrap().rows.len(), 2);
                let strikes = &plot.profile.exposure.as_ref().unwrap().rows;
                assert_eq!(strikes.len(), 1);
                assert!((strikes[0].net.unwrap() - 10.0).abs() < 1e-10);
                study.set_input("expiry", InputValue::Choice(2)).unwrap();
                assert!((study.plot_value(2, 0).unwrap() - 5.0).abs() < 1e-10);
                sink.push(Vec::new()).unwrap();
            }
            assert!(study.volume_profiles(0..1).is_empty());
            assert_eq!(study.plot_value(3, 0), None);
            assert_eq!(study.plot_value(4, 0), None);
            assert_eq!(study.plot_value(5, 0), None);
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

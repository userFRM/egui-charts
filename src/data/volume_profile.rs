//! Price-row volume distributions shared by studies and drawings.

use crate::model::Bar;
use serde::{Deserialize, Serialize};
use std::ops::{Range, RangeInclusive};
use std::sync::Arc;

/// One price-row profile placed over a half-open span of chart bars.
#[derive(Clone, Debug)]
pub struct StudyProfile {
    /// Full source span for bar profiles, including offscreen bars in fixed periods,
    /// or the visible span for an options overlay.
    pub bars: Range<usize>,
    /// Immutable row volumes and price boundaries.
    pub profile: Arc<VolumeProfile>,
    /// Row layout, value area and display settings.
    pub config: VolumeProfileConfig,
    /// Colour of total volume and value-area lines.
    pub color: [u8; 4],
    /// Draw at the right of the viewport instead of beside the first source bar.
    pub right_aligned: bool,
}

/// Volume shown in each price row of a profile built from bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProfileDisplay {
    /// Total volume, with the point of control and value area highlighted.
    #[default]
    Total,
    /// Up-bar and down-bar volume in adjacent segments.
    UpDown,
    /// Absolute up minus down volume, coloured by its sign.
    Delta,
}

fn profile_up_color() -> [u8; 4] {
    [38, 166, 154, 180]
}
fn profile_down_color() -> [u8; 4] {
    [239, 83, 80, 180]
}

/// Row layout and display settings shared by volume profile studies and drawings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VolumeProfileFields")]
pub struct VolumeProfileConfig {
    row_size: Option<f64>,
    rows: usize,
    value_area: f64,
    /// Volume displayed in each row. Totals still determine POC and value area.
    pub display: ProfileDisplay,
    /// Colour of rising bars, including an unchanged close, in split and delta modes.
    pub up_color: [u8; 4],
    /// Colour of falling bars in split and delta modes.
    pub down_color: [u8; 4],
}

/// Stored settings of a volume profile, checked by [`VolumeProfileConfig::new`] on load.
#[derive(Deserialize)]
struct VolumeProfileFields {
    #[serde(default)]
    row_size: Option<f64>,
    rows: usize,
    value_area: f64,
    #[serde(default)]
    display: ProfileDisplay,
    #[serde(default = "profile_up_color")]
    up_color: [u8; 4],
    #[serde(default = "profile_down_color")]
    down_color: [u8; 4],
}

impl TryFrom<VolumeProfileFields> for VolumeProfileConfig {
    type Error = &'static str;

    fn try_from(fields: VolumeProfileFields) -> Result<Self, Self::Error> {
        let mut config = Self::new(fields.rows, fields.value_area)
            .ok_or("volume profile settings out of bounds")?;
        config.display = fields.display;
        config.up_color = fields.up_color;
        config.down_color = fields.down_color;
        config
            .with_row_size(fields.row_size)
            .ok_or("volume profile row height must be finite and positive")
    }
}

impl VolumeProfileConfig {
    /// Price rows a profile may have.
    pub const ROWS: RangeInclusive<usize> = 1..=1000;

    /// A profile of `rows` equal price rows whose value area holds the
    /// `value_area` share of its volume (above 0, at most 1); `None` when
    /// either is out of bounds.
    pub fn new(rows: usize, value_area: f64) -> Option<Self> {
        (Self::ROWS.contains(&rows) && value_area > 0.0 && value_area <= 1.0).then_some(Self {
            rows,
            value_area,
            ..Self::default()
        })
    }

    /// Fix each row to a price height, aligned to multiples of that height.
    /// `None` restores equal rows over the span. Rejects nonpositive or nonfinite heights.
    /// Profiles spanning more than 10,000 fixed rows are not drawn.
    pub fn with_row_size(mut self, height: Option<f64>) -> Option<Self> {
        if height.is_some_and(|h| !h.is_finite() || h <= 0.0) {
            return None;
        }
        self.row_size = height;
        Some(self)
    }

    /// Fixed price height per row, or equal rows over the span when absent.
    pub fn row_size(&self) -> Option<f64> {
        self.row_size
    }

    /// Number of price rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Share of the volume inside the value area.
    pub fn value_area(&self) -> f64 {
        self.value_area
    }
}

impl Default for VolumeProfileConfig {
    /// 24 rows and a value area of 70% of the volume.
    fn default() -> Self {
        Self {
            row_size: None,
            rows: 24,
            value_area: 0.7,
            display: ProfileDisplay::Total,
            up_color: profile_up_color(),
            down_color: profile_down_color(),
        }
    }
}

/// Volume traded at each price row of bars or host-supplied option strikes.
#[derive(Clone, Debug, PartialEq)]
pub struct VolumeProfile {
    /// Price at the bottom of row 0.
    pub(crate) low: f64,
    /// Highest price of the bars.
    pub(crate) high: f64,
    pub(crate) row_height: f64,
    fixed_height: bool,
    /// Volume per row, lowest prices first.
    pub(crate) volume: Vec<f64>,
    pub(crate) up: Vec<f64>,
    pub(crate) down: Vec<f64>,
}

fn valid_bar(bar: &Bar) -> bool {
    [bar.open, bar.high, bar.low, bar.close, bar.volume]
        .iter()
        .all(|v| v.is_finite())
        && (bar.high - bar.low).is_finite()
        && bar.high >= bar.low
        && bar.volume >= 0.0
}

impl VolumeProfile {
    /// Group host options turnover into rows centered on multiples of `row_size`.
    /// A row at strike K includes [K - row_size/2, K + row_size/2); a boundary
    /// goes to the higher row. Calls occupy [`Self::up`], puts [`Self::down`].
    /// Duplicate strikes are summed. Empty, invalid, overflowing or unrepresentable
    /// snapshots, including grids exceeding 10,000 rows, produce no profile.
    pub fn from_options(data: &[crate::data::OptionsVolume], row_size: f64) -> Option<Self> {
        if data.is_empty()
            || !row_size.is_finite()
            || row_size <= 0.0
            || data.iter().any(|v| {
                !v.strike.is_finite()
                    || !v.calls.is_finite()
                    || !v.puts.is_finite()
                    || v.calls < 0.0
                    || v.puts < 0.0
            })
        {
            return None;
        }
        let key = |strike| grid_floor(strike / row_size + 0.5);
        let first = data
            .iter()
            .map(|v| key(v.strike))
            .fold(f64::INFINITY, f64::min);
        let last = data
            .iter()
            .map(|v| key(v.strike))
            .fold(f64::NEG_INFINITY, f64::max);
        let count = last - first + 1.0;
        if !first.is_finite()
            || !last.is_finite()
            || first.abs() >= 2_f64.powi(52)
            || last.abs() >= 2_f64.powi(52)
            || !(1.0..=10_000.0).contains(&count)
        {
            return None;
        }
        let low = (first - 0.5) * row_size;
        let high = low + count * row_size;
        if !low.is_finite() || !high.is_finite() || high <= low {
            return None;
        }
        let mut profile = Self {
            low,
            high,
            row_height: row_size,
            fixed_height: false,
            volume: vec![0.0; count as usize],
            up: vec![0.0; count as usize],
            down: vec![0.0; count as usize],
        };
        let mut total = 0.0;
        for v in data {
            let row = (key(v.strike) - first) as usize;
            profile.up[row] += v.calls;
            profile.down[row] += v.puts;
            profile.volume[row] = profile.up[row] + profile.down[row];
            total += v.calls + v.puts;
        }
        total.is_finite().then_some(profile)
    }

    /// Distribute finite, nonnegative-volume bars over their high-low ranges.
    /// Invalid bars are ignored. Returns no profile for an empty or unrepresentable grid.
    pub fn from_bars(bars: &[Bar], config: VolumeProfileConfig) -> Option<Self> {
        Self::of(bars, config.rows(), config.row_size())
    }
    /// Bottom edge of the first row.
    pub fn low(&self) -> f64 {
        self.low
    }
    /// Top edge of the last row.
    pub fn high(&self) -> f64 {
        self.low + self.volume.len() as f64 * self.row_height
    }
    /// Price height of a row.
    pub fn row_height(&self) -> f64 {
        self.row_height
    }
    /// Total volume per row, lowest price first.
    pub fn volume(&self) -> &[f64] {
        &self.volume
    }
    /// Up-bar volume per row (unchanged closes count as up), or calls for options.
    pub fn up(&self) -> &[f64] {
        &self.up
    }
    /// Down-bar volume per row, or puts for options.
    pub fn down(&self) -> &[f64] {
        &self.down
    }

    /// Spreads each bar's volume evenly over its high-low range across
    /// `rows` equal price rows (a bar without range puts it in the row of its
    /// price). A flat span uses one narrow row; `None` for an unrepresentable grid.
    pub(crate) fn of(bars: &[Bar], rows: usize, row_size: Option<f64>) -> Option<Self> {
        let low = bars
            .iter()
            .filter(|b| valid_bar(b))
            .map(|b| b.low)
            .fold(f64::INFINITY, f64::min);
        let high = bars
            .iter()
            .filter(|b| valid_bar(b))
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max);
        if rows == 0 || rows > 10_000 {
            return None;
        }
        let (low, high, row_height, rows) = if let Some(height) = row_size {
            let (low, high, rows) = fixed_grid(low, high, height)?;
            (low, high, height, rows)
        } else if low == high && low.is_finite() {
            let height = (low.abs() * 1e-6).max(1e-6);
            (low - height * 0.5, high + height * 0.5, height, 1)
        } else {
            (low, high, (high - low) / rows as f64, rows)
        };
        if !low.is_finite() || !high.is_finite() || !row_height.is_finite() || row_height <= 0.0 {
            return None;
        }
        let mut profile = Self {
            low,
            high,
            row_height,
            fixed_height: row_size.is_some(),
            volume: vec![0.0; rows],
            up: vec![0.0; rows],
            down: vec![0.0; rows],
        };
        bars.iter().for_each(|bar| profile.add(bar));
        Some(profile)
    }

    /// Add one completed bar without redistributing earlier bars. Fixed-height grids grow
    /// at the edges; equal-row grids return `None` when the bar moves a boundary.
    /// A changed forming bar must be added to the profile before that bar, not added twice.
    pub fn with_bar(&self, bar: &Bar) -> Option<Self> {
        let mut profile = self.clone();
        if !valid_bar(bar) {
            return Some(profile);
        }
        if self.fixed_height {
            let (low, high, _) = fixed_grid(bar.low, bar.high, self.row_height)?;
            let low = low.min(self.low);
            let high = high.max(self.high);
            let count = ((high - low) / self.row_height).round();
            if count > 10_000.0 {
                return None;
            }
            let prepend = ((self.low - low) / self.row_height).round() as usize;
            for rows in [&mut profile.volume, &mut profile.up, &mut profile.down] {
                let old_len = rows.len();
                rows.resize(count as usize, 0.0);
                if prepend > 0 {
                    rows.copy_within(..old_len, prepend);
                    rows[..prepend].fill(0.0);
                }
            }
            profile.low = low;
            profile.high = high;
        } else if bar.low < self.low || bar.high > self.high {
            return None;
        }
        profile.add(bar);
        Some(profile)
    }

    /// Adds the volume of `bar` to the rows that its high-low range covers,
    /// in proportion to the overlap.
    fn add(&mut self, bar: &Bar) {
        if !valid_bar(bar) {
            return;
        }
        let (low, row_height) = (self.low, self.row_height);
        let direction = if bar.close >= bar.open {
            &mut self.up
        } else {
            &mut self.down
        };
        let top_row = self.volume.len() - 1;
        let row_of = |price: f64| (grid_floor((price - low) / row_height) as usize).min(top_row);
        if bar.high > bar.low {
            let (first, last) = (row_of(bar.low), row_of(bar.high));
            for (row, row_volume) in self
                .volume
                .iter_mut()
                .enumerate()
                .take(last + 1)
                .skip(first)
            {
                let row_low = low + row as f64 * row_height;
                let overlap = bar.high.min(row_low + row_height) - bar.low.max(row_low);
                let amount = bar.volume * (overlap.max(0.0) / (bar.high - bar.low));
                *row_volume += amount;
                direction[row] += amount;
            }
        } else {
            self.volume[row_of(bar.low)] += bar.volume;
            direction[row_of(bar.low)] += bar.volume;
        }
    }

    /// The row with the most volume (point of control), and the lowest and
    /// highest row of the value area, which holds `share` of the volume;
    /// `None` when no volume traded.
    pub fn poc_and_value_area(&self, share: f64) -> Option<(usize, (usize, usize))> {
        if !share.is_finite() || share <= 0.0 || share > 1.0 {
            return None;
        }
        let volume = &self.volume;
        let total: f64 = volume.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        let poc = (0..volume.len()).max_by(|&a, &b| volume[a].total_cmp(&volume[b]))?;
        Some((poc, value_area(volume, poc, total * share)))
    }
}

// Decimal tick heights can put an exact boundary one rounding step below its integer row.
fn grid_floor(value: f64) -> f64 {
    let nearest = value.round();
    if (value - nearest).abs() <= 4.0 * f64::EPSILON * value.abs() {
        nearest
    } else {
        value.floor()
    }
}

/// A zero-aligned grid, including the row of a zero-range bar on an edge.
fn fixed_grid(low: f64, high: f64, height: f64) -> Option<(f64, f64, usize)> {
    if !height.is_finite() || height <= 0.0 {
        return None;
    }
    let first = grid_floor(low / height);
    let last = grid_floor(high / height);
    let count = last - first + 1.0;
    if !first.is_finite()
        || !last.is_finite()
        || first.abs() >= 2_f64.powi(52)
        || last.abs() >= 2_f64.powi(52)
        || !(1.0..=10_000.0).contains(&count)
    {
        return None;
    }
    let bounds = (first * height, (last + 1.0) * height);
    (bounds.0.is_finite() && bounds.1.is_finite() && bounds.1 > bounds.0).then_some((
        bounds.0,
        bounds.1,
        count as usize,
    ))
}

/// Rows around the POC that hold `target` volume. Market Profile rule: grow
/// toward whichever of the next two rows above or the next two rows below
/// holds more volume, one row at a time.
fn value_area(volume: &[f64], poc: usize, target: f64) -> (usize, usize) {
    let (mut low, mut high, mut sum) = (poc, poc, volume[poc]);
    while sum < target && (low > 0 || high + 1 < volume.len()) {
        let above: f64 = volume[high + 1..].iter().take(2).sum();
        let below: f64 = volume[..low].iter().rev().take(2).sum();
        let up = high + 1 < volume.len() && (low == 0 || above >= below);
        for _ in 0..2 {
            if up && high + 1 < volume.len() {
                high += 1;
                sum += volume[high];
            } else if !up && low > 0 {
                low -= 1;
                sum += volume[low];
            }
            if sum >= target {
                break;
            }
        }
    }
    (low, high)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_bars() -> Vec<Bar> {
        [
            (10.0, 12.0, 9.0, 11.0, 100.0),
            (11.0, 13.0, 10.0, 12.0, 200.0),
            (12.0, 12.5, 11.0, 11.0, 150.0),
            (11.5, 14.0, 11.0, 13.5, 300.0),
            (13.5, 15.0, 13.0, 14.0, 250.0),
            (14.0, 14.5, 12.0, 12.5, 400.0),
            (13.0, 13.0, 13.0, 13.0, 50.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c, v))| {
            let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
            Bar::new(time, o, h, l, c, v)
        })
        .collect()
    }
    fn assert_close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "{got} != {want}");
    }
    /// Reference: rows of 1.5 from 9.0, each bar's volume split by how much of
    /// its range falls in the row, the zero-range bar whole in the row of 13.0.
    /// For 70% the value area takes the two rows below the POC (433.3) over
    /// the one row above it (397.5); comparing single rows would take the row
    /// above.
    #[test]
    fn volume_profile_matches_reference() {
        let profile = VolumeProfile::of(&sample_bars(), 4, None).unwrap();
        for (price, representable) in [(13.0, true), (f64::MAX, false), (-f64::MAX, false)] {
            let flat = Bar::new(
                chrono::DateTime::UNIX_EPOCH,
                price,
                price,
                price,
                price,
                50.0,
            );
            let flat_profile = VolumeProfile::of(&[flat], 4, None);
            assert_eq!(flat_profile.is_some(), representable, "flat price {price}");
            if let Some(flat_profile) = flat_profile {
                assert_eq!(flat_profile.volume(), [50.0]);
                assert_eq!(flat_profile.up(), [50.0]);
            }
        }
        assert_close(profile.low, 9.0);
        assert_close(profile.row_height, 1.5);
        for (got, want) in
            profile
                .volume
                .iter()
                .zip([83.33333333333334, 350.0, 619.1666666666667, 397.5])
        {
            assert_close(*got, want);
        }
        // The down bars are 150 over 11..12.5 and 400 over 12..14.5.
        for (row, (up, down)) in [
            (250.0 / 3.0, 0.0),
            (250.0, 100.0),
            (3950.0 / 12.0, 290.0),
            (237.5, 160.0),
        ]
        .into_iter()
        .enumerate()
        {
            assert_close(profile.up[row], up);
            assert_close(profile.down[row], down);
            assert_close(profile.up[row] + profile.down[row], profile.volume[row]);
        }
        for (share, value_area) in [(0.5, (1, 2)), (0.7, (0, 2)), (1.0, (0, 3))] {
            assert_eq!(
                profile.poc_and_value_area(share),
                Some((2, value_area)),
                "share {share}"
            );
        }
    }

    /// Existing rows keep their prices and volume when a fixed grid grows at either edge.
    #[test]
    fn fixed_profile_grows_without_rebinning() {
        let time = chrono::DateTime::UNIX_EPOCH;
        let first = Bar::new(time, -0.5, 1.0, -1.0, 0.5, 20.0);
        let mut bars = vec![first];
        let mut profile = VolumeProfile::of(&bars, 24, Some(1.0)).unwrap();
        assert_eq!(profile.volume, [10.0, 10.0, 0.0]);
        for (low, high, volume, expected) in [
            (-2.0, -1.0, 6.0, vec![6.0, 10.0, 10.0, 0.0]),
            (1.0, 2.0, 8.0, vec![6.0, 10.0, 10.0, 8.0, 0.0]),
            (2.0, 2.0, 3.0, vec![6.0, 10.0, 10.0, 8.0, 3.0]),
        ] {
            let bar = Bar::new(time, low, high, low, high, volume);
            // Extension must succeed: a fallback rebuild would conceal the regression.
            profile = profile.with_bar(&bar).expect("extend fixed grid");
            bars.push(bar);
            assert_eq!(profile.volume, expected);
            assert_eq!(
                Some(profile.clone()),
                VolumeProfile::of(&bars, 24, Some(1.0))
            );
        }
        assert!(VolumeProfile::of(&bars, 24, Some(1e-12)).is_none());
        let boundary = Bar::new(time, 0.3, 0.3, 0.3, 0.3, 7.0);
        let decimal = VolumeProfile::of(&[boundary], 24, Some(0.1)).unwrap();
        assert_close(decimal.low(), 0.3);
        assert_eq!(decimal.volume(), [7.0]);
    }

    /// Four-unit rows centered at 100, 104 and 108 collect [98,102), [102,106)
    /// and [106,110). Totals are 90+50=140, 60+140=200 and 75+25=100.
    #[test]
    fn options_profile_groups_strikes_and_scales_by_largest_total() {
        use crate::data::OptionsVolume;
        let data: Vec<_> = [
            (99.0, 20.0, 10.0),
            (100.0, 10.0, 5.0),
            (100.0, 20.0, 15.0),
            (101.0, 40.0, 20.0),
            (102.0, 10.0, 40.0),
            (103.0, 20.0, 50.0),
            (104.0, 30.0, 50.0),
            (107.0, 75.0, 25.0),
        ]
        .into_iter()
        .map(|(strike, calls, puts)| OptionsVolume {
            strike,
            calls,
            puts,
        })
        .collect();
        let profile = VolumeProfile::from_options(&data, 4.0).unwrap();
        assert_eq!(
            (profile.low(), profile.high(), profile.row_height()),
            (98.0, 110.0, 4.0)
        );
        let max = profile.volume().iter().copied().fold(0.0, f64::max);
        for (row, (calls, puts, share)) in
            [(90.0, 50.0, 0.7), (60.0, 140.0, 1.0), (75.0, 25.0, 0.5)]
                .into_iter()
                .enumerate()
        {
            assert_close(profile.up()[row], calls);
            assert_close(profile.down()[row], puts);
            assert_close(profile.volume()[row] / max, share);
        }
        #[cfg(feature = "widget")]
        for right_aligned in [false, true] {
            for direction in [-1.0, 1.0] {
                let config = VolumeProfileConfig {
                    display: ProfileDisplay::UpDown,
                    ..VolumeProfileConfig::default()
                };
                let rgba =
                    |c: [u8; 4]| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    let painter = egui::Painter::new(
                        ui.ctx().clone(),
                        egui::LayerId::background(),
                        egui::Rect::EVERYTHING,
                    );
                    crate::painting::volume_profile::paint_profile(
                        &painter,
                        &profile,
                        config,
                        [0, 0, 255, 255],
                        100.0..300.0,
                        right_aligned,
                        &|price| price as f32 * direction,
                    );
                });
                let rows: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(r)
                            if r.fill == rgba(config.up_color)
                                || r.fill == rgba(config.down_color) =>
                        {
                            Some(r)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(rows.len(), 6);
                // Of 180 available points, the rows use 126, 180 and 90; each segment
                // uses its own contract volume divided by the largest combined total, 200.
                for (row, (call_width, put_width, strike)) in [
                    (81.0, 45.0, 100.0),
                    (54.0, 126.0, 104.0),
                    (67.5, 22.5, 108.0),
                ]
                .into_iter()
                .enumerate()
                {
                    let (calls, puts) = (rows[2 * row], rows[2 * row + 1]);
                    assert!((calls.rect.width() - call_width).abs() < 1e-4);
                    assert!((puts.rect.width() - put_width).abs() < 1e-4);
                    assert!((calls.rect.top() - (strike * direction - 2.0)).abs() < 1e-4);
                    assert_eq!(calls.fill, rgba(config.up_color));
                    assert_eq!(puts.fill, rgba(config.down_color));
                    if right_aligned {
                        assert!((calls.rect.right() - 300.0).abs() < 1e-4);
                        assert!((puts.rect.right() - calls.rect.left()).abs() < 1e-4);
                    } else {
                        assert!((calls.rect.left() - 100.0).abs() < 1e-4);
                        assert!((puts.rect.left() - calls.rect.right()).abs() < 1e-4);
                    }
                }
                output.drop_without_applying_deltas();
            }
        }
        // Exact and decimal strikes stay at row centers; half-row boundaries go up.
        for (strike, size, center) in [
            (100.0, 1.0, 100.0),
            (0.3, 0.1, 0.3),
            (0.35, 0.1, 0.4),
            (-0.15, 0.1, -0.1),
        ] {
            let point = OptionsVolume {
                strike,
                calls: 3.0,
                puts: 7.0,
            };
            let p = VolumeProfile::from_options(&[point], size).unwrap();
            assert_close(p.low() + p.row_height() / 2.0, center);
            assert_eq!(p.volume(), [10.0]);
        }
        for bad in [f64::NAN, f64::INFINITY, -1.0] {
            let invalid = OptionsVolume {
                strike: 100.0,
                calls: bad,
                puts: 0.0,
            };
            assert!(VolumeProfile::from_options(&[invalid], 1.0).is_none());
            assert!(VolumeProfile::from_options(&data, bad).is_none());
        }
        assert!(VolumeProfile::from_options(&data, 0.0).is_none());
        assert!(VolumeProfile::from_options(&data, 1e-12).is_none());
        assert!(VolumeProfile::from_options(&[], 4.0).is_none());
        assert!(
            VolumeProfile::from_options(
                &[-1e308, 1e308].map(|strike| OptionsVolume {
                    strike,
                    calls: 1.0,
                    puts: 0.0,
                }),
                1e308,
            )
            .is_none()
        );
        assert!(
            VolumeProfile::from_options(
                &[OptionsVolume {
                    strike: 100.0,
                    calls: f64::MAX,
                    puts: f64::MAX
                }],
                1.0
            )
            .is_none()
        );
    }
}

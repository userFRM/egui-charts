//! Chart data export and renderer-independent image frames.

use super::Chart;
use crate::localization as l10n;
use crate::scales::StudyFormat;
use crate::theme::ThemeExt;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;

/// Pixels per point of a PNG, clipboard or frame export. A higher density draws the same layout
/// with more pixels, for high-density screens, documents and print; SVG ignores it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PixelDensity {
    /// One pixel per point, the default.
    #[default]
    One,
    /// Two pixels per point.
    Two,
    /// Three pixels per point.
    Three,
}

impl PixelDensity {
    /// Every density, lowest first, as the export dialog offers them.
    pub const ALL: [Self; 3] = [Self::One, Self::Two, Self::Three];

    /// The number of pixels per point: 1, 2 or 3.
    pub fn pixels_per_point(self) -> u32 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

/// Image options shared by the export dialog and offscreen rendering.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImageExportOptions {
    /// Width and height of the layout in points. A PNG, clipboard or frame export has
    /// [`pixel_density`](Self::pixel_density) times as many pixels on each side; an SVG document
    /// has this size. Defaults to 1440 by 900.
    pub size: [u32; 2],
    /// Include the symbol above the chart.
    pub include_title: bool,
    /// Include the export time above the chart, written as the chart writes times, with its
    /// date format, clock and locale, with seconds on every interval unless the time scale hides
    /// them, followed by the name of the chart's time zone.
    pub include_timestamp: bool,
    /// Draw the symbol watermark and every host watermark set with [`Chart::set_watermark`]
    /// or [`Chart::set_pane_watermark`], as they show on screen, independently of the title and
    /// timestamp.
    pub include_watermark: bool,
    /// Omit the chart and header backgrounds, retaining plot and drawing colours.
    pub transparent_background: bool,
    /// Draw the crosshair where the pointer last placed it over the plots, or where
    /// [`Chart::set_crosshair_position`], [`Chart::set_series_crosshair_position`] or a
    /// synchronized chart placed it since.
    pub include_crosshair: bool,
    /// Draw [`PrimitivePass::Above`](super::PrimitivePass::Above) content above the plots and
    /// persistent drawings. Defaults to `true`, independently of `include_crosshair`.
    /// Frozen exports omit pointer hover and selection decorations with either setting.
    pub include_foreground: bool,
    /// Pixels of the textures the host binds itself, by the id its images use, so
    /// `Chart::export_png` and [`Chart::export_svg`] can draw them with `png-export`.
    /// Without an entry, an image of such a texture fails the export. Supplied textures use
    /// linear filtering and clamp to their edges. Prefer [`egui::TextureId::User`]: ids that
    /// collide with the offscreen frame's own textures return an error. [`ChartImage`] frames
    /// include the supplied pixels too.
    pub textures: HashMap<egui::TextureId, Arc<egui::ColorImage>>,
    /// Pixels per point of PNG and frame exports; SVG ignores it. The size limits of
    /// [`ExportError::InvalidSize`] apply to the pixels. Defaults to [`PixelDensity::One`].
    pub pixel_density: PixelDensity,
}

impl Default for ImageExportOptions {
    fn default() -> Self {
        Self {
            size: [1440, 900],
            include_title: true,
            include_timestamp: true,
            include_watermark: true,
            transparent_background: false,
            include_crosshair: false,
            include_foreground: true,
            textures: HashMap::new(),
            pixel_density: PixelDensity::One,
        }
    }
}

/// A complete offscreen frame for an egui renderer at [`pixels_per_point`](Self::pixels_per_point).
/// Upload `textures_delta.set`, draw `primitives` over transparent black into `size` pixels with
/// that many pixels per point, as an egui renderer's screen description takes them, then release
/// the textures. The host owns GPU readback, PNG encoding and file/clipboard delivery.
/// This uses its own texture namespace; render with a dedicated egui renderer. Hosts must also
/// bind any user textures referenced by their image sources whose pixels were not supplied in
/// [`ImageExportOptions::textures`] before drawing.
#[must_use = "Render the frame and apply its texture deltas, or clear them before dropping"]
#[derive(Debug)]
#[non_exhaustive]
pub struct ChartImage {
    /// Exact output dimensions in pixels: the options' size times the pixel density.
    pub size: [u32; 2],
    /// Tessellated chart, header, study panes and requested layout widgets.
    pub primitives: Vec<egui::ClippedPrimitive>,
    /// Textures referenced by the primitives.
    pub textures_delta: egui::TexturesDelta,
    /// Pixels per point the primitives were laid out and tessellated for: 1, 2 or 3, from
    /// [`ImageExportOptions::pixel_density`]. Primitive positions are in points.
    pub pixels_per_point: f32,
}

/// What [`Chart::data_table_with`] and `Chart::export_csv_with` (with `csv`) list. The default
/// lists what [`Chart::data_table`] and `Chart::export_csv` do: every loaded bar with its time and
/// main series values, the visible added series and every visible plot of the visible studies, as
/// plain numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataExportOptions {
    /// The earliest row time listed, inclusive. `None`, the default, starts at the first bar. On a
    /// [horizontal scale](Chart::set_horizontal_scale) other than time, a key.
    pub from: Option<DateTime<Utc>>,
    /// The latest row time listed, inclusive. `None`, the default, ends at the last row. A `to`
    /// before `from` lists no rows.
    pub to: Option<DateTime<Utc>>,
    /// List the time column first. Defaults to `true`.
    pub include_time: bool,
    /// List the main series' open, high, low, close and volume. Defaults to `true`.
    pub include_main_series: bool,
    /// Also list studies and study plots hidden on the chart, including studies hidden with
    /// [`Chart::set_indicators_hidden`]. Defaults to `false`.
    pub include_hidden_studies: bool,
    /// Also list the rows past the last bar on which a displaced plot draws a value, such as the
    /// last 8 bars of Williams Alligator's jaw. Their times continue the bars' spacing as
    /// [`Chart::index_to_time`] extrapolates it, and the main series' cells are empty. Defaults
    /// to `false`.
    pub include_values_past_last_bar: bool,
    /// The studies to list, by id, in chart order. `None`, the default, lists every study. Ids
    /// that name no study are ignored. A hidden study listed here still needs
    /// [`include_hidden_studies`](Self::include_hidden_studies).
    pub studies: Option<Vec<super::SeriesId>>,
    /// How values are written. Defaults to [`DataValues::Plain`].
    pub values: DataValues,
}

impl Default for DataExportOptions {
    fn default() -> Self {
        Self {
            from: None,
            to: None,
            include_time: true,
            include_main_series: true,
            include_hidden_studies: false,
            include_values_past_last_bar: false,
            studies: None,
            values: DataValues::Plain,
        }
    }
}

/// How [`DataExportOptions`] writes values. Times are written the same way in every mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DataValues {
    /// Plain numbers with the decimals each column takes, as `Chart::export_csv` writes them.
    #[default]
    Plain,
    /// As the data window writes them: prices of the main series, the added series and price
    /// studies in their formatter, else with the chart's thousands separator.
    Displayed,
    /// Every finite value unrounded, in the shortest text that reads back as the same number.
    Raw,
}

/// What a column of a [`DataTable`] holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DataColumn {
    /// The time of each row, or the key of a horizontal scale other than time.
    Time,
    /// A value of a series. Of the main series ([`SeriesId::MAIN`](super::SeriesId::MAIN)),
    /// `plot` 0 to 4 is the open, high, low, close and volume; of an added series, `plot` 0 is its
    /// close.
    Series {
        /// The series.
        id: super::SeriesId,
        /// The value of the series, numbered as above.
        plot: usize,
    },
    /// Output `plot` of study `id`, numbered as `Indicator::plot_value` numbers it.
    Study {
        /// The study.
        id: super::SeriesId,
        /// The study output.
        plot: usize,
    },
}

/// A column of a [`DataTable`]. A study column carries the original id and title of its study
/// and its plot; the title keeps a choice off a study of another title that a restore adds under
/// the id the choice was made for. An added series column carries its title and its place among
/// the visible series of that title, which a restore, adding the series again under new ids,
/// keeps.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ColumnId {
    Main(usize),
    Series(String, usize),
    Study(usize, String, usize),
}

/// The loaded bars, the closes of visible added series and visible study plots as text, aligned
/// by chart time: what `Chart::export_csv` (with `csv`) writes and the chart data table lists.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DataTable {
    /// Column titles: time, open, high, low, close and volume, then one per visible added series,
    /// titled as its legend shows it, and one per visible study plot, of those the
    /// [`DataExportOptions`] list.
    pub headers: Vec<String>,
    /// What each column holds, one per header.
    pub columns: Vec<DataColumn>,
    /// One row per loaded bar in the range, oldest first, then the rows past the last bar that
    /// [`DataExportOptions::include_values_past_last_bar`] asks for. A missing or non-finite value
    /// is an empty cell.
    pub rows: Vec<Vec<String>>,
}

impl DataTable {
    /// CSV text of the `columns` (indices into [`Self::headers`]) in the given order.
    #[cfg(feature = "csv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "csv")))]
    pub fn csv(
        &self,
        columns: &[usize],
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer.write_record(columns.iter().map(|&column| &self.headers[column]))?;
        for row in &self.rows {
            writer.write_record(columns.iter().map(|&column| &row[column]))?;
        }
        Ok(String::from_utf8(writer.into_inner()?)?)
    }

    /// Tab-separated text of the `columns` with a header line, for pasting into a spreadsheet.
    pub fn tsv(&self, columns: &[usize]) -> String {
        let line = |cells: &mut dyn Iterator<Item = &String>| {
            cells.map(String::as_str).collect::<Vec<_>>().join("\t")
        };
        let mut lines = vec![line(
            &mut columns.iter().map(|&column| &self.headers[column]),
        )];
        lines.extend(
            self.rows
                .iter()
                .map(|row| line(&mut columns.iter().map(|&column| &row[column]))),
        );
        lines.join("\n")
    }
}

/// Sets the host's textures in `delta` beside the frame's own, for the encoders that draw them.
fn add_host_textures(
    delta: &mut egui::TexturesDelta,
    options: &ImageExportOptions,
) -> Result<(), String> {
    for (&id, image) in &options.textures {
        if delta.set.contains_key(&id) {
            return Err(format!(
                "Host texture {id:?} conflicts with an offscreen texture; use TextureId::User"
            ));
        }
        let [width, height] = image.size;
        if width == 0
            || height == 0
            || width > u32::MAX as usize
            || height > u32::MAX as usize
            || width.checked_mul(height) != Some(image.pixels.len())
        {
            return Err(format!(
                "Host texture {id:?} has invalid dimensions or pixels"
            ));
        }
        delta.push(
            id,
            egui::epaint::ImageDelta::full(
                egui::ImageData::Color(Arc::clone(image)),
                egui::TextureOptions::LINEAR,
            ),
        );
    }
    Ok(())
}

/// Decimals that write each finite price of `prices` as the number it holds, ignoring binary
/// noise past 15 significant digits: at least two and at most twelve.
fn held_decimals(prices: impl IntoIterator<Item = f64>) -> usize {
    prices
        .into_iter()
        .filter(|price| price.is_finite())
        .map(crate::scales::decimals)
        .fold(2, usize::max)
        .min(12)
}

/// Why [`Chart::export_image`], `Chart::export_png` (with `png-export`) or
/// [`Chart::export_svg`] made no image.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExportError {
    /// A side of the output is outside 100 to 7680 pixels, or the output has more than
    /// 33,177,600 pixels. PNG, clipboard and frame exports have the options' size times the pixel
    /// density in pixels; SVG has the size itself.
    InvalidSize {
        /// The output width in pixels, saturating at `u32::MAX`.
        width: u32,
        /// The output height in pixels, saturating at `u32::MAX`.
        height: u32,
    },
    /// The egui context has not run its first frame, so it has no fonts to lay the chart out.
    NoFrame,
    /// The frame holds content the format cannot take over unchanged, such as a host paint
    /// callback or a texture the host binds itself; the text names it.
    Unsupported(String),
    /// The PNG encoder failed; the text is its message.
    Encode(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSize { width, height } => write!(
                f,
                "Image size {width} by {height} must be 100 to 7680 pixels per side and at most 33,177,600 pixels"
            ),
            Self::NoFrame => {
                f.write_str("Run the egui context's first frame before exporting an image")
            }
            Self::Unsupported(reason) => f.write_str(reason),
            Self::Encode(message) => write!(f, "PNG encoding failed: {message}"),
        }
    }
}
impl std::error::Error for ExportError {}

impl Chart {
    /// `time` with the UTC offset of `zone`, the zone [`Self::bar_date_timezone`] gives.
    fn export_time(time: DateTime<Utc>, zone: &crate::scales::TimezoneMode) -> String {
        match zone {
            crate::scales::TimezoneMode::Local => time.with_timezone(&chrono::Local).to_rfc3339(),
            zone => time
                .with_timezone(&zone.to_tz().unwrap_or(crate::model::TimezoneId::utc()))
                .to_rfc3339(),
        }
    }

    /// The export time above an image: the date and clock with seconds as the chart writes
    /// times, also on daily bars, in its display zone and followed by the zone's name, as the
    /// bottom toolbar's clock names it.
    fn header_time(&self, time: DateTime<Utc>) -> String {
        let zone = self.display_timezone();
        let name = match &zone {
            crate::scales::TimezoneMode::Local => crate::model::TimezoneId::fixed_offset(
                time.with_timezone(&chrono::Local)
                    .offset()
                    .local_minus_utc(),
            )
            .unwrap_or_default(),
            zone => zone.to_tz().unwrap_or_default(),
        };
        let clock = self.time_formatter_in(zone).format_crosshair(time, 1);
        format!("{clock} {name}")
    }

    /// Exports all loaded source bars, the close of each visible added series and visible study
    /// plots as UTF-8 CSV. An added series fills the rows whose time matches one of its bars and
    /// leaves the others empty.
    /// Recalculates copies of the studies so streaming updates and pending refreshes are included
    /// without altering the live chart. Price-based charts export source-bar calculations, aligned
    /// by source index and plot displacement, rather than mixing element values with source bars.
    /// Times include the UTC offset of the zone that dates the bars, including DST changes and
    /// subsecond timestamps: the display zone, or UTC for bars of a day or longer that
    /// [`format_time`](Self::format_time) reads in UTC, so each row keeps the date the chart gives
    /// its bar. On a [horizontal scale](Self::set_horizontal_scale) other than time, keys read as
    /// the crosshair label does.
    /// Prices use the instrument's decimals, and the main series the precision chosen for the
    /// price scale in their place. Without `price_decimals` or a valid `tick_size`, price studies
    /// take the main series' precision, and without a chosen one the main series and price
    /// studies write the decimals the main series' prices hold, and an added series those its
    /// closes hold, at least two and at most twelve, so a price the chart shows does not read as
    /// another number; an added series keeps at least four significant digits. Volumes take the
    /// instrument's volume decimals, at most twelve as volume studies take them, and without them
    /// the shortest text that holds their first 15 significant digits; they are never abbreviated.
    /// Study overrides and non-price units retain their precision. Missing/non-finite values are
    /// empty cells. This lists the defaults of [`DataExportOptions`]; see `Self::export_csv_with`
    /// to choose rows, columns and values.
    #[cfg(feature = "csv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "csv")))]
    pub fn export_csv(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.export_csv_with(&DataExportOptions::default())
    }

    /// [`Self::export_csv`] of the rows, columns and values `options` chooses, as
    /// [`Self::data_table_with`] lists them.
    #[cfg(feature = "csv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "csv")))]
    pub fn export_csv_with(
        &self,
        options: &DataExportOptions,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let table = self.data_table_with(options);
        table.csv(&(0..table.headers.len()).collect::<Vec<_>>())
    }

    /// The cells of `Self::export_csv` (with `csv`) as a table, with the same bars, added series
    /// closes, study plots, times and precision. During replay it holds the bars the replay
    /// cutoff has completed, and on price-based chart types still the source bars. Loads
    /// nothing: only loaded bars are listed. This lists the defaults of [`DataExportOptions`];
    /// see [`Self::data_table_with`] to choose rows, columns and values.
    pub fn data_table(&self) -> DataTable {
        self.data_table_with(&DataExportOptions::default())
    }

    /// [`Self::data_table`] of the rows, columns and values `options` chooses: the rows whose
    /// time is in `from..=to`, the time and main series columns only when asked, the studies
    /// listed by id, hidden studies and plots when asked, the rows past the last bar on which a
    /// displaced plot draws a value when asked, and values plain, as displayed or raw. Each
    /// value keeps the decimals it takes in the full table, and [`DataTable::columns`] names
    /// what each column holds, so two studies of the same title are told apart by id.
    ///
    /// ```
    /// use egui_charts::model::{Bar, BarData};
    /// use egui_charts::studies::{CustomIndicator, IndicatorValue};
    /// use egui_charts::widget::{Chart, DataColumn, DataExportOptions, DataValues};
    ///
    /// let start = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    /// let bars = (0..3)
    ///     .map(|i| Bar::new(start + chrono::Duration::hours(i), 10.0, 10.5, 9.5, 10.25, 1.0))
    ///     .collect();
    /// let mut chart = Chart::new(BarData::from_bars(bars));
    /// let ahead = chart.add_indicator(Box::new(
    ///     CustomIndicator::new("Ahead", |bars| {
    ///         bars.iter().map(|bar| IndicatorValue::Single(bar.close)).collect()
    ///     })
    ///     .with_offsets(vec![2]),
    /// ));
    /// let mut options = DataExportOptions::default();
    /// options.include_main_series = false;
    /// options.include_values_past_last_bar = true;
    /// options.values = DataValues::Raw;
    /// let table = chart.data_table_with(&options);
    /// assert_eq!(table.columns, [DataColumn::Time, DataColumn::Study { id: ahead, plot: 0 }]);
    /// // The offset draws the closes of bars 1 and 2 on the two rows past the last bar.
    /// assert_eq!(table.rows.len(), 5);
    /// assert_eq!(table.rows[4][1], "10.25");
    /// ```
    pub fn data_table_with(&self, options: &DataExportOptions) -> DataTable {
        self.data_table_with_columns(options).0
    }

    /// Build cells and the identity of each column from the same calculated studies. With
    /// [`DataValues::Displayed`], the cells of the main series, the added series and the price
    /// studies read as the data window writes them: in the series' or the study's formatter, else
    /// with their thousands grouped; other cells, and every cell without it, stay plain numbers.
    pub(crate) fn data_table_with_columns(
        &self,
        options: &DataExportOptions,
    ) -> (DataTable, Vec<ColumnId>) {
        let readable = options.values == DataValues::Displayed;
        let bars = &self.state.data().bars;
        let instrument_precision = self
            .instrument()
            .and_then(crate::scales::instrument_price_precision)
            .map(usize::from);
        // The main series' prices take the chart's readout decimals. Without metadata, prices
        // keep the decimals they hold, so none reads as another number.
        let price_precision = self
            .readout_price_decimals()
            .map(usize::from)
            .unwrap_or_else(|| {
                held_decimals(
                    bars.iter()
                        .flat_map(|bar| [bar.open, bar.high, bar.low, bar.close]),
                )
            });
        // The main series' volumes take the instrument's volume decimals, capped as volume studies
        // cap them so both columns agree; without them each keeps the digits it holds.
        let volume_precision = self
            .instrument()
            .and_then(|instrument| instrument.volume_decimals)
            .map(|decimals| usize::from(decimals).min(12));
        let mut studies: Vec<_> = self
            .indicators()
            .map(|(_, study)| study.clone_box())
            .collect();
        let mut columns = Vec::new();
        // Each visible added series with its legend title, which falls back to "Series" as the
        // pane menu does, its place among series of that title and the decimals of its closes.
        let mut series: Vec<(&[crate::model::Bar], String, usize, usize, super::SeriesId)> =
            Vec::new();
        let mut series_formatters = Vec::new();
        for item in self.price_series.iter().filter(|item| item.visible) {
            series_formatters.push(
                readable
                    .then(|| self.series_price_formatter(item.id))
                    .flatten(),
            );
            let title = match item.options.name.as_str() {
                "" => "Series".to_owned(),
                name => name.to_owned(),
            };
            let place = series.iter().filter(|other| other.1 == title).count();
            // A series of another scale keeps the decimals its closes hold without metadata, and
            // four significant digits of its smallest close.
            let precision = item
                .bars
                .iter()
                .filter(|bar| !bar.is_whitespace() && bar.close.is_finite() && bar.close != 0.0)
                .map(|bar| (3.0 - bar.close.abs().log10().floor()).clamp(0.0, 12.0) as usize)
                .fold(
                    instrument_precision
                        .unwrap_or_else(|| held_decimals(item.bars.iter().map(|bar| bar.close))),
                    usize::max,
                );
            series.push((item.bars.as_slice(), title, place, precision, item.id));
        }
        let main_columns = [
            (options.include_time, ["time"].as_slice(), 0),
            (
                options.include_main_series,
                ["open", "high", "low", "close", "volume"].as_slice(),
                1,
            ),
        ];
        let mut header: Vec<String> = main_columns
            .iter()
            .filter(|(included, ..)| *included)
            .flat_map(|(_, names, _)| names.iter().map(|name| (*name).to_owned()))
            .collect();
        header.extend(series.iter().map(|item| item.1.clone()));
        let clock = self.bar_clock();
        for study in &mut studies {
            study.set_instrument(self.instrument());
            study.set_chart_symbol(&self.symbol);
            study.set_time_scale_events(&self.time_scale_events);
            if let Some(timeframe) = self.timeframe {
                study.set_timeframe(timeframe);
            }
            study.set_bar_clock(&clock);
            self.study_data.set_replay_cut(
                study.as_mut(),
                &self.symbol,
                self.timeframe,
                self.replay_time(),
                self.replay_newest_closed(),
            );
        }
        let requests = studies
            .iter()
            .flat_map(|study| study.data_requests())
            .collect();
        let sources =
            self.study_data
                .data(requests, &self.symbol, self.timeframe, self.replay_time());
        super::studies::calculate(&mut studies, &self.study_ids, bars, &sources);
        for (index, study) in studies.iter_mut().enumerate().filter(|(index, study)| {
            options
                .studies
                .as_ref()
                .is_none_or(|ids| ids.contains(&self.study_ids[*index]))
                && (options.include_hidden_studies || self.shows_study(study.as_ref()))
        }) {
            let names = study.line_names();
            for line in (0..study.line_count())
                .filter(|line| options.include_hidden_studies || study.plot_style(*line).visible)
            {
                header.push(format!(
                    "{}: {}",
                    study.name(),
                    names.get(line).map_or("", String::as_str)
                ));
                let precision = study.precision().unwrap_or(match study.value_format() {
                    StudyFormat::Price => price_precision,
                    StudyFormat::Fixed(decimals) => decimals.min(12),
                    _ => 2,
                });
                // A price study reads as the data window writes it: in its formatter, else grouped.
                let price_units = readable && study.value_format() == StudyFormat::Price;
                let own_text = price_units
                    && self
                        .study_formatter(self.study_ids[index], StudyFormat::Price)
                        .is_some();
                columns.push((index, line, precision, price_units, own_text));
            }
        }
        let number = |value: f64, precision: usize| match options.values {
            _ if !value.is_finite() => String::new(),
            DataValues::Raw => value.to_string(),
            _ => format!("{value:.precision$}"),
        };
        // Built-in labels write '.' as the decimal point, so '.' and digits group nothing.
        let separator = self
            .config
            .thousands_sep
            .filter(|separator| *separator != '.' && !separator.is_ascii_digit());
        let price =
            |value: f64,
             precision: usize,
             formatter: Option<&std::sync::Arc<dyn crate::scales::PriceFormatter>>| {
                match formatter {
                    _ if !readable || !value.is_finite() => number(value, precision),
                    Some(formatter) => formatter.format(value),
                    None => crate::scales::group_thousands(number(value, precision), separator),
                }
            };
        let main_formatter = readable
            .then(|| self.series_price_formatter(super::SeriesId::MAIN))
            .flatten();
        // Rows date the bars as the chart labels them: a calendar bar keeps its trading date.
        let zone = self.bar_date_timezone();
        // The rows past the last bar on which a listed displaced plot draws a value. Offsets reach
        // `isize::MAX`, so a plot's candidates are bounded by the values it holds.
        let mut past = std::collections::BTreeSet::new();
        if options.include_values_past_last_bar {
            for &(study, line, ..) in &columns {
                let study = &studies[study];
                let offset = usize::try_from(study.plot_offset(line)).unwrap_or(0);
                let Some(end) = study
                    .values()
                    .len()
                    .checked_add(offset)
                    .filter(|_| offset > 0)
                else {
                    continue;
                };
                past.extend((bars.len().max(offset)..end).filter(|&row| {
                    study
                        .plot_value(line, row)
                        .is_some_and(|value| value.is_finite())
                }));
            }
        }
        let (timeframe, timezone) = (self.calendar_timeframe(), self.calendar_timezone());
        let past = past.into_iter().filter_map(|row| {
            crate::model::time_at_with_timeframe(bars, row as f64, timeframe, timezone.as_ref())
                .map(|time| (row, time, None))
        });
        let listed = bars
            .iter()
            .enumerate()
            .map(|(index, bar)| (index, bar.time, Some(bar)))
            .chain(past)
            .filter(|&(_, time, _)| {
                options.from.is_none_or(|from| time >= from)
                    && options.to.is_none_or(|to| time <= to)
            });
        let mut rows = Vec::with_capacity(bars.len());
        for (index, time, bar) in listed {
            let mut row = Vec::with_capacity(header.len());
            if options.include_time {
                row.push(if self.horizontal_scale.is_some() {
                    self.format_time(time)
                } else {
                    Self::export_time(time, &zone)
                });
            }
            match bar {
                Some(bar) if options.include_main_series => {
                    row.extend(
                        [bar.open, bar.high, bar.low, bar.close]
                            .map(|value| price(value, price_precision, main_formatter.as_ref())),
                    );
                    row.push(number(
                        bar.volume,
                        volume_precision.unwrap_or_else(|| crate::scales::decimals(bar.volume)),
                    ));
                }
                None if options.include_main_series => {
                    row.extend(std::iter::repeat_n(String::new(), 5));
                }
                _ => {}
            }
            row.extend(series.iter().zip(&series_formatters).map(
                |(&(bars, _, _, precision, _), formatter)| {
                    bars.binary_search_by_key(&time, |other| other.time)
                        .ok()
                        .map(|at| &bars[at])
                        .filter(|other| !other.is_whitespace())
                        .map_or_else(String::new, |other| {
                            price(other.close, precision, formatter.as_ref())
                        })
                },
            ));
            row.extend(
                columns
                    .iter()
                    .map(|&(study, line, precision, price_units, own_text)| {
                        studies[study]
                            .plot_value(line, index)
                            .map_or_else(String::new, |value| match (price_units, own_text) {
                                (true, true) if value.is_finite() => {
                                    studies[study].format_plot_value(line, value)
                                }
                                (true, _) => price(value, precision, None),
                                _ => number(value, precision),
                            })
                    }),
            );
            rows.push(row);
        }
        let main = main_columns
            .iter()
            .filter(|(included, ..)| *included)
            .flat_map(|&(_, names, first)| first..first + names.len());
        let data_columns = main
            .clone()
            .map(|column| match column {
                0 => DataColumn::Time,
                column => DataColumn::Series {
                    id: super::SeriesId::MAIN,
                    plot: column - 1,
                },
            })
            .chain(series.iter().map(|item| DataColumn::Series {
                id: item.4,
                plot: 0,
            }))
            .chain(columns.iter().map(|&(study, line, ..)| DataColumn::Study {
                id: self.study_ids[study],
                plot: line,
            }))
            .collect();
        let column_ids = main
            .map(ColumnId::Main)
            .chain(
                series
                    .iter()
                    .map(|item| ColumnId::Series(item.1.clone(), item.2)),
            )
            .chain(columns.iter().map(|&(study, line, ..)| {
                let id = self.study_ids[study].0;
                #[cfg(feature = "drawings")]
                let id = self.study_column_ids.get(&id).copied().unwrap_or(id);
                ColumnId::Study(id, studies[study].name().to_owned(), line)
            }))
            .collect();
        (
            DataTable {
                headers: header,
                columns: data_columns,
                rows,
            },
            column_ids,
        )
    }

    /// Renders an isolated image frame using the same chart, studies, primitives, drawings and
    /// scales as the widget. Does not poll feeds, change live viewport state, call the lifecycle
    /// hooks of primitives or capture host panels/dialogs. A pane primitive attached since the
    /// last frame is left out, as it draws only after its `attached` hook.
    /// `ctx` supplies the host's fonts and style after its first frame. The image lays out
    /// `options.size` points at [`ImageExportOptions::pixel_density`]; its pixels, the size times
    /// the density, are 100 to 7680 on each axis, with at most 33,177,600 in all.
    /// Pass the returned frame to an egui renderer; see [`ChartImage`] for delivery responsibilities.
    ///
    /// # Errors
    /// Returns [`ExportError::InvalidSize`] for dimensions outside the limits,
    /// [`ExportError::NoFrame`] before the first frame, or [`ExportError::Unsupported`]
    /// for invalid host textures.
    pub fn export_image(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
    ) -> Result<ChartImage, ExportError> {
        self.export_layout_image(ctx, options, |ui, chart| {
            chart.show(ui);
        })
    }

    /// Like [`Self::export_image`] for a layout of library widgets: `layout` draws everything
    /// inside the image, the toolbars, panels and trading overlays around the chart, and shows the
    /// chart it is given, a frozen copy of this one, with [`Chart::show`] or
    /// [`Chart::show_footprint`]. It runs once for each of several layout passes, on a context
    /// of its own with no pointer, so its widgets show as at rest. The title and timestamp, if
    /// requested, sit above it.
    ///
    /// ```no_run
    /// # fn capture(chart: &egui_charts::Chart, ctx: &egui::Context) -> Result<(), egui_charts::widget::ExportError> {
    /// let mut options = egui_charts::widget::ImageExportOptions::default();
    /// options.pixel_density = egui_charts::widget::PixelDensity::Two;
    /// let image = chart.export_layout_image(ctx, &options, |ui, snapshot| {
    ///     ui.horizontal(|ui| {
    ///         ui.label(snapshot.symbol());
    ///         ui.label("1m");
    ///     });
    ///     snapshot.show(ui);
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn export_layout_image(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<ChartImage, ExportError> {
        let pixels_per_point = options.pixel_density.pixels_per_point();
        let (shapes, textures_delta, image_ctx) =
            self.export_shapes(ctx, options, pixels_per_point, layout)?;
        Ok(ChartImage {
            size: options.size.map(|side| side * pixels_per_point),
            primitives: image_ctx.tessellate(shapes, pixels_per_point as f32),
            textures_delta,
            pixels_per_point: pixels_per_point as f32,
        })
    }

    /// The shapes of the last pass of the offscreen frame laid out at `pixels_per_point`, the
    /// textures it sets and the context whose fonts laid it out.
    fn export_shapes(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        pixels_per_point: u32,
        mut layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<
        (
            Vec<egui::epaint::ClippedShape>,
            egui::TexturesDelta,
            egui::Context,
        ),
        ExportError,
    > {
        let pixels = options
            .size
            .map(|side| u64::from(side) * u64::from(pixels_per_point));
        // The area is only multiplied once both sides are at most 7680, so it cannot overflow.
        if pixels.iter().any(|&side| !(100..=7680).contains(&side))
            || pixels[0] * pixels[1] > 33_177_600
        {
            return Err(ExportError::InvalidSize {
                width: u32::try_from(pixels[0]).unwrap_or(u32::MAX),
                height: u32::try_from(pixels[1]).unwrap_or(u32::MAX),
            });
        }
        let size = options.size;
        if ctx.cumulative_pass_nr() == 0 {
            return Err(ExportError::NoFrame);
        }
        let mut chart = self.export_snapshot(ctx, options);
        let theme = chart
            .theme_override
            .clone()
            .unwrap_or_else(|| ctx.active_theme());
        let image_ctx = egui::Context::default();
        image_ctx.set_global_style(ctx.global_style());
        crate::theme::apply_to_egui(&image_ctx, &theme);
        image_ctx.set_fonts(ctx.fonts(|fonts| fonts.definitions().clone()));
        let timestamp = options
            .include_timestamp
            .then(|| self.header_time(Utc::now()));
        let title = options.include_title.then(|| self.symbol().to_owned());
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32),
            )),
            ..Default::default()
        };
        // Text is laid out and its glyphs rasterized at the density, as on a screen with it.
        if let Some(viewport) = input.viewports.get_mut(&egui::ViewportId::ROOT) {
            viewport.native_pixels_per_point = Some(pixels_per_point as f32);
        }
        let mut textures_delta = egui::TexturesDelta::default();
        // Resolve pane layout, axis measurement and each pending element study before the final frame.
        let passes = chart.studies.indicators().len() + 3;
        let mut shapes = Vec::new();
        for pass in 0..passes {
            let output = image_ctx.run_ui(
                egui::RawInput {
                    time: Some(pass as f64 * 3600.0),
                    ..input.clone()
                },
                |ui| {
                    if !options.transparent_background {
                        ui.painter()
                            .rect_filled(ui.max_rect(), 0.0, chart.config.background_color);
                    }
                    ui.set_min_size(ui.available_size());
                    if title.is_some() || timestamp.is_some() {
                        ui.horizontal(|ui| {
                            if let Some(title) = &title {
                                ui.label(l10n::text(ui.ctx(), title));
                            }
                            if let Some(timestamp) = &timestamp {
                                ui.label(timestamp);
                            }
                        });
                    }
                    layout(ui, &mut chart);
                },
            );
            textures_delta.append(output.textures_delta);
            shapes = output.shapes;
        }
        if let Err(error) = add_host_textures(&mut textures_delta, options) {
            textures_delta.clear();
            return Err(ExportError::Unsupported(error));
        }
        Ok((shapes, textures_delta, image_ctx))
    }

    /// Renders the image of [`Self::export_image`] on the CPU and encodes it as PNG, so the host
    /// needs no renderer or GPU readback. Fails with the reason for content it cannot draw, host
    /// paint callbacks and images of textures the host binds itself, such as a legend badge from a
    /// user texture, unless [`ImageExportOptions::textures`] holds their pixels, instead of
    /// leaving it out.
    ///
    /// # Errors
    /// Returns the errors of [`Self::export_image`], [`ExportError::Unsupported`] for content
    /// that cannot be drawn, or [`ExportError::Encode`] if the PNG encoder fails.
    #[cfg(feature = "png-export")]
    #[cfg_attr(docsrs, doc(cfg(feature = "png-export")))]
    pub fn export_png(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
    ) -> Result<Vec<u8>, ExportError> {
        self.export_layout_png(ctx, options, |ui, chart| {
            chart.show(ui);
        })
    }

    /// Like [`Self::export_png`] for a layout of library widgets, drawn by `layout` as
    /// [`Self::export_layout_image`] describes.
    #[cfg(feature = "png-export")]
    #[cfg_attr(docsrs, doc(cfg(feature = "png-export")))]
    pub fn export_layout_png(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<Vec<u8>, ExportError> {
        let mut image = self.export_layout_image(ctx, options, layout)?;
        let pixels = super::raster::rasterize(
            image.size,
            image.pixels_per_point,
            &image.primitives,
            &image.textures_delta,
        );
        image.textures_delta.clear();
        super::raster::png(image.size, &pixels.map_err(ExportError::Unsupported)?)
            .map_err(ExportError::Encode)
    }

    /// Renders the frame of [`Self::export_image`] as an SVG document of the options' size, with
    /// the same title, timestamp, watermark and background. Text stays text, in the font family
    /// class of the on-screen font, and the frame's textures, such as legend icons, are embedded
    /// as PNG with `png-export` enabled, with the host's [`ImageExportOptions::textures`].
    /// Fails with the reason for content SVG cannot take over unchanged, such as host paint
    /// callbacks and textures the host binds itself without supplying their pixels, instead of
    /// leaving it out. [`ImageExportOptions::pixel_density`] does not apply: the document is the
    /// options' size.
    ///
    /// # Errors
    /// Returns the errors of [`Self::export_image`] or [`ExportError::Unsupported`] for content
    /// the format cannot take over unchanged, including image textures when `png-export` is off.
    pub fn export_svg(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
    ) -> Result<String, ExportError> {
        self.export_layout_svg(ctx, options, |ui, chart| {
            chart.show(ui);
        })
    }

    /// Like [`Self::export_svg`] for a layout of library widgets, drawn by `layout` as
    /// [`Self::export_layout_image`] describes.
    pub fn export_layout_svg(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<String, ExportError> {
        let (shapes, mut textures_delta, _) = self.export_shapes(ctx, options, 1, layout)?;
        let svg = super::svg::svg(options.size, &shapes, &textures_delta);
        textures_delta.clear();
        // shortcut: an encoder failure while embedding a texture arrives as Unsupported; split it when svg.rs returns ExportError.
        svg.map_err(ExportError::Unsupported)
    }

    /// A frozen chart for a multi-chart image layout, with the same background, title,
    /// watermark, foreground and crosshair choices as [`Self::export_image`]. Show it inside the callback
    /// of [`Self::export_layout_image`], `Self::export_layout_png` (with `png-export`) or
    /// [`Self::export_layout_svg`].
    /// No storage or live subscriptions are attached; the original chart stays unchanged.
    pub fn export_snapshot(&self, ctx: &egui::Context, options: &ImageExportOptions) -> Self {
        let mut chart = self.frozen_export_snapshot();
        chart.hide_foreground = !options.include_foreground;
        chart.config.skip_background = options.transparent_background;
        chart.config.show_symbol_labels &= options.include_title;
        if !options.include_watermark {
            chart.config.show_watermark = false;
            for pane in &mut chart.pane_layout.panes {
                pane.watermark = None;
            }
        }
        let mut theme = self
            .theme_override
            .clone()
            .unwrap_or_else(|| ctx.active_theme());
        if options.transparent_background {
            theme.chart.bg = egui::Color32::TRANSPARENT;
            // The fills that repeat the background behind text have nothing to repeat.
            chart.config.background_color = egui::Color32::TRANSPARENT;
            chart.config.background_style = crate::config::BackgroundStyle::Solid;
        }
        chart.theme_override = Some(theme);
        if let Some((time, price)) = self.last_crosshair.filter(|_| options.include_crosshair) {
            match price {
                Some(price) => match self.last_crosshair_series {
                    Some(series) => {
                        chart.set_series_crosshair_position(series, Some((time, price)))
                    }
                    None => chart.set_crosshair_position(Some((time, price))),
                },
                None => {
                    chart.set_synced_crosshair_time(Some(time));
                    chart.chart_options.crosshair.horz_line_visible = false;
                    chart.chart_options.crosshair.horz_label_visible = false;
                }
            }
        }
        chart
    }

    fn frozen_export_snapshot(&self) -> Self {
        let mut chart = Chart::with_config(Default::default(), self.config.clone());
        chart.state = self.state.clone();
        chart.chart_options = self.chart_options.clone();
        chart
            .chart_options
            .time_scale
            .lock_visible_time_range_on_resize = true;
        chart.prev_width = self.prev_width;
        chart.chart_options.time_scale.timezone = self.display_timezone();
        chart.chart_type = self.chart_type;
        chart.chart_type_params = self.chart_type_params.clone();
        #[cfg(feature = "studies")]
        {
            chart.trade_profiles = self.trade_profiles.clone();
        }
        chart.pending_box_sizes = self.pending_box_sizes;
        chart.main_series_visible = self.main_series_visible;
        chart.indicators_hidden = self.indicators_hidden;
        chart.main_series_present = self.main_series_present;
        chart.main_series_scale = self.main_series_scale;
        chart.main_series_style = self.main_series_style.clone();
        chart.price_series = self
            .price_series
            .iter()
            .map(|series| super::series::PriceSeries {
                id: series.id,
                options: series.options.clone(),
                visible: series.visible,
                fed: series.fed,
                filled: None,
                restored: false,
                bars: series.bars.clone(),
                revision: series.revision,
                last_price_pulse: std::cell::Cell::new(None),
                heikin_ashi: Default::default(),
                feed: series.feed.clone(),
                markers: series.markers.clone(),
                marker_options: series.marker_options,
                change_marks: None,
                lines: series.lines.clone(),
            })
            .collect();
        chart.series_scales = self.series_scales.clone();
        chart.symbol = self.symbol.clone();
        chart.timeframe = self.timeframe;
        chart.feed_link = self.feed_link.as_ref().map(super::feed::FeedLink::frozen);
        chart.legend_instrument = self.instrument().cloned();
        chart.legend_badge = self.legend_badge.clone();
        chart.legend_statuses = self.legend_statuses.clone();
        chart.quote = self.quote.clone();
        chart.session_quote = self.session_quote.clone();
        chart.session_provider = self.session_provider.clone();
        chart.horizontal_scale = self.horizontal_scale.clone();
        chart.locale = self.locale.clone();
        chart.date_format = self.date_format.clone();
        chart.time_formatter = self.time_formatter.clone();
        chart.price_formatter = self.price_formatter.clone();
        chart.percentage_formatter = self.percentage_formatter.clone();
        chart.volume_formatter = self.volume_formatter.clone();
        chart.series_formatters = self.series_formatters.clone();
        chart.study_formatters = self.study_formatters.clone();
        chart.extended_hours = self.extended_hours;
        chart.overnight_hours = self.overnight_hours;
        chart.theme_override = self.theme_override.clone();
        chart.inherited_theme = self.inherited_theme.clone();
        #[cfg(feature = "drawings")]
        {
            chart.drawings.registry = self.drawings.registry.clone();
            chart.drawings.drawings = self.drawings.drawings.clone();
            chart.drawings.curr_timeframe = self.drawings.curr_timeframe.clone();
            chart.drawings.set_hidden(self.drawings.is_hidden());
            chart.drawings.shared_filter = self.drawings.shared_filter;
        }
        chart.marks = self.marks.clone();
        chart.marker_options = self.marker_options;
        chart.price_lines = self.price_lines.clone();
        chart.events = self.events.clone();
        chart.time_scale_events = self.time_scale_events.clone();
        chart.event_visibility = self.event_visibility.clone();
        // Freeze loaded source marks without connecting the offscreen chart to the host.
        if let Some(source) = &self.mark_source {
            chart.marks = self
                .marks
                .iter()
                .chain(&source.page.bar_marks)
                .cloned()
                .collect::<Vec<_>>()
                .into();
            chart.events = self
                .events
                .iter()
                .chain(&source.page.time_scale_marks)
                .cloned()
                .collect::<Vec<_>>()
                .into();
            chart.time_scale_events = self
                .time_scale_events
                .iter()
                .chain(&source.page.events)
                .cloned()
                .collect::<Vec<_>>()
                .into();
            chart.frozen_source_events = source.page.events.len();
        }
        chart.desired_visible_bars = self.desired_visible_bars;
        chart.apply_visible_bars_once = self.apply_visible_bars_once;
        chart.pending_visible_range = self.pending_visible_range;
        chart.start_idx = self.start_idx;
        for (_, study) in self.indicators() {
            chart.insert_indicator(study.clone_for_export());
        }
        chart.study_ids = self.study_ids.clone();
        chart.study_inputs = self.study_inputs.clone();
        chart.study_autoscale = self.study_autoscale.clone();
        chart.next_series = self.next_series;
        // Copied after the studies, replacing the panes that adding them created.
        chart.pane_layout = self.pane_layout.clone();
        chart.pane_options = self.pane_options;
        #[cfg(feature = "studies")]
        {
            chart.volume_study = self.volume_study;
            chart.volume_setting = self.volume_setting;
        }
        chart.primitives = self.primitives.clone();
        chart.freeze_custom_points(self.replay_time());
        chart.next_primitive = self.next_primitive;
        chart.settle_primitives();
        chart.study_data = self.study_data.snapshot();
        chart.recalculate_studies();
        chart
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "csv")]
    use crate::model::Instrument;
    #[cfg(any(feature = "csv", feature = "drawings"))]
    use crate::model::{Bar, BarData, Timeframe};
    #[cfg(any(feature = "csv", feature = "drawings"))]
    use crate::studies::CustomIndicator;
    #[cfg(feature = "csv")]
    use crate::studies::IndicatorValue;
    use crate::widget::SeriesId;

    #[test]
    #[cfg(feature = "csv")]
    fn csv_aligns_fresh_visible_plots_and_formats_chart_times_and_prices() {
        let times: Vec<_> = [
            "2024-11-03T05:30:00.125Z",
            "2024-11-03T06:30:00.125Z",
            "2024-11-03T07:30:00.125Z",
        ]
        .map(|time| DateTime::parse_from_rfc3339(time).unwrap().to_utc())
        .into();
        let zones = [
            (
                crate::scales::TimezoneMode::Utc,
                [
                    "2024-11-03T05:30:00.125+00:00",
                    "2024-11-03T06:30:00.125+00:00",
                ],
            ),
            #[cfg(feature = "timezones")]
            (
                crate::scales::TimezoneMode::Timezone("America/New_York".parse().unwrap()),
                [
                    "2024-11-03T01:30:00.125-04:00",
                    "2024-11-03T01:30:00.125-05:00",
                ],
            ),
            #[cfg(feature = "timezones")]
            (
                crate::scales::TimezoneMode::Exchange(crate::model::TimezoneId::utc()),
                [
                    "2024-11-03T01:30:00.125-04:00",
                    "2024-11-03T01:30:00.125-05:00",
                ],
            ),
        ];
        for (zone, expected_times) in zones {
            let bars: Vec<_> = times
                .iter()
                .zip([10.0, 14.0, 18.0])
                .map(|(&time, close)| {
                    Bar::new(time, close, close + 1.0, close - 1.0, close, 1234.5)
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            chart.set_timeframe(Timeframe::Hour1);
            chart.chart_options.time_scale.timezone = zone;
            let mut instrument = Instrument::new("TEST");
            instrument.price_decimals = Some(2);
            instrument.volume_decimals = Some(3);
            instrument.time_zone = crate::model::TimezoneId::named("America/New_York");
            chart.set_instrument(Some(instrument.clone()));
            let study = chart.add_indicator(Box::new(
                CustomIndicator::new("Average, pair", |bars| {
                    bars.iter()
                        .enumerate()
                        .map(|(index, bar)| {
                            let mean = if index == 0 {
                                f64::NAN
                            } else {
                                (bars[index - 1].close + bar.close) / 2.0
                            };
                            IndicatorValue::Multiple(vec![mean, mean, bar.close * 2.0])
                        })
                        .collect()
                })
                .with_line_count(3)
                .with_line_names(vec!["mean".into(), "shifted".into(), "hidden".into()])
                .with_offsets(vec![0, -1, 0]),
            ));
            let settings_study = chart.indicator_mut(study).unwrap();
            let mut settings = settings_study.settings();
            settings.plots[2].visible = false;
            settings_study.apply_settings(&[], settings).unwrap();
            let hidden = chart
                .add_indicator(Box::new(CustomIndicator::new("Hidden study", |bars| {
                    vec![IndicatorValue::Single(999.0); bars.len()]
                })));
            chart.set_indicator_visible(hidden, false);
            chart.add_indicator(Box::new(
                CustomIndicator::new("Fixed", |bars| {
                    vec![IndicatorValue::Single(1.23456); bars.len()]
                })
                .with_value_format(StudyFormat::Fixed(4)),
            ));
            let override_id = chart
                .add_indicator(Box::new(CustomIndicator::new("Override", |bars| {
                    vec![IndicatorValue::Single(9.876); bars.len()]
                })));
            let indicator = chart.indicator_mut(override_id).unwrap();
            let mut settings = indicator.settings();
            settings.precision = Some(1);
            indicator.apply_settings(&[], settings).unwrap();
            // Volume study columns keep the instrument's decimals without abbreviating.
            chart.add_indicator(Box::new(
                CustomIndicator::new("Volume", |bars| {
                    bars.iter()
                        .map(|bar| IndicatorValue::Single(bar.volume))
                        .collect()
                })
                .with_value_format(StudyFormat::Volume),
            ));
            instrument.price_decimals = Some(3);
            chart.set_instrument(Some(instrument));
            let mut last = bars[2].clone();
            last.close = 22.0;
            last.high = 23.0;
            chart.update(SeriesId::MAIN, last);
            // A visible added series lists its closes at the bars it holds, by exact time; a
            // hidden one has no column.
            let peer = chart.add_series(crate::widget::SeriesOptions {
                name: "Peer".into(),
                ..Default::default()
            });
            chart.set_data(
                peer,
                vec![
                    Bar::new(times[0], 5.0, 6.0, 4.0, 5.5, 1.0),
                    Bar::new(times[2], 7.0, 8.0, 6.0, 7.25, 1.0),
                    Bar::new(
                        times[2] + chrono::Duration::hours(1),
                        7.0,
                        8.0,
                        6.0,
                        9.0,
                        1.0,
                    ),
                ],
            );
            let tiny = chart.add_series(crate::widget::SeriesOptions {
                name: "Tiny".into(),
                ..Default::default()
            });
            chart.set_data(
                tiny,
                vec![Bar::new(times[1], 0.0002, 0.0003, 0.0001, 0.000123, 1.0)],
            );
            let hidden_series = chart.add_series(crate::widget::SeriesOptions {
                name: "Off".into(),
                ..Default::default()
            });
            chart.set_data(
                hidden_series,
                vec![Bar::new(times[1], 1.0, 1.0, 1.0, 1.0, 1.0)],
            );
            chart.set_series_visible(hidden_series, false);
            let csv = chart.export_csv();
            fn assert_send_sync<T: Send + Sync>(_: &T) {}
            assert_send_sync(&csv);
            let csv = csv.unwrap();
            let mut reader = csv::Reader::from_reader(csv.as_bytes());
            assert_eq!(
                reader.headers().unwrap().iter().collect::<Vec<_>>(),
                [
                    "time",
                    "open",
                    "high",
                    "low",
                    "close",
                    "volume",
                    "Peer",
                    "Tiny",
                    "Average, pair: mean",
                    "Average, pair: shifted",
                    "Fixed: Fixed",
                    "Override: Override",
                    "Volume: Volume"
                ]
            );
            let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(rows.len(), 3);
            assert_eq!(&rows[0][0], expected_times[0]);
            assert_eq!(&rows[1][0], expected_times[1]);
            assert_eq!(&rows[0][1], "10.000");
            assert_eq!(&rows[2][4], "22.000");
            // The volume column writes the volume decimals, as the Volume study column (12) does.
            assert_eq!(&rows[0][5], "1234.500");
            // Two-bar means are 12 and (14 + 22) / 2 = 18; displacement -1 moves them left.
            assert_eq!(
                rows.iter().map(|row| &row[6]).collect::<Vec<_>>(),
                ["5.500", "", "7.250"]
            );
            // A series of another scale keeps its digits instead of rounding to the main decimals.
            assert_eq!(
                rows.iter().map(|row| &row[7]).collect::<Vec<_>>(),
                ["", "0.0001230", ""]
            );
            assert_eq!(
                rows.iter().map(|row| &row[8]).collect::<Vec<_>>(),
                ["", "12.000", "18.000"]
            );
            assert_eq!(
                rows.iter().map(|row| &row[9]).collect::<Vec<_>>(),
                ["12.000", "18.000", ""]
            );
            assert_eq!(&rows[0][10], "1.2346");
            assert_eq!(&rows[0][11], "9.9");
            assert_eq!(
                rows.iter().map(|row| &row[12]).collect::<Vec<_>>(),
                ["1234.500"; 3]
            );
            // Export must not refresh or mutate the chart's scheduled study results.
            assert!(chart.pending_from[0].is_some());
        }
        let empty = Chart::new(BarData::new()).export_csv().unwrap();
        assert_eq!(empty, "time,open,high,low,close,volume\n");
    }

    /// Without instrument decimals, exported prices keep the decimals they hold, so a sub-cent
    /// close does not read 0.00; an instrument's tick size still sets them, and a precision chosen
    /// for the price scale sets the main series'.
    #[test]
    #[cfg(feature = "csv")]
    fn csv_prices_keep_their_decimals_without_instrument_metadata() {
        type Row = (
            &'static str,
            Option<f64>,
            Option<u8>,
            [f64; 2],
            [&'static str; 3],
        );
        // Last row's close, added series close and price study cells.
        let cases: [Row; 5] = [
            (
                "sub-cent",
                None,
                None,
                [0.000123, 0.000162],
                ["0.000162", "0.0001620", "0.000162"],
            ),
            (
                "four decimals",
                None,
                None,
                [1.0812, 1.0851],
                ["1.0851", "1.0851", "1.0851"],
            ),
            (
                "binary noise",
                None,
                None,
                [0.1, 0.1 + 0.2],
                ["0.30", "0.3000", "0.30"],
            ),
            (
                "tick size",
                Some(0.01),
                None,
                [1.0812, 1.0851],
                ["1.09", "1.085", "1.09"],
            ),
            (
                "chosen",
                Some(0.01),
                Some(4),
                [1.0812, 1.0851],
                ["1.0851", "1.085", "1.09"],
            ),
        ];
        let start = DateTime::parse_from_rfc3339("2024-01-02T00:00:00Z")
            .unwrap()
            .to_utc();
        for (case, tick_size, chosen, closes, expected) in cases {
            let bars: Vec<_> = (0..)
                .zip(closes)
                .map(|(minute, close)| {
                    let time = start + chrono::Duration::minutes(minute);
                    Bar::new(time, close, close, close, close, 1.0)
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            if tick_size.is_some() {
                let mut instrument = Instrument::new("TEST");
                instrument.tick_size = tick_size;
                chart.set_instrument(Some(instrument));
            }
            chart.price_scale_mut().set_price_precision(chosen);
            let peer = chart.add_series(crate::widget::SeriesOptions {
                name: "Peer".into(),
                ..Default::default()
            });
            chart.set_data(peer, bars);
            chart.add_indicator(Box::new(CustomIndicator::new("Close", |bars| {
                bars.iter()
                    .map(|bar| IndicatorValue::Single(bar.close))
                    .collect()
            })));
            let csv = chart.export_csv().unwrap();
            let last = csv::Reader::from_reader(csv.as_bytes())
                .records()
                .last()
                .unwrap()
                .unwrap();
            assert_eq!([&last[4], &last[6], &last[7]], expected, "{case}");
        }
    }

    /// The volume column writes the main series' volumes with the instrument's volume decimals,
    /// at most twelve as a Volume study column of the row does, and without them the volume each
    /// bar holds without binary noise; the CSV export and the copied text take the same cells.
    #[test]
    #[cfg(feature = "csv")]
    fn data_table_writes_volumes_with_the_volume_decimals() {
        let (mut written, mut expected) = (Vec::new(), Vec::new());
        for (volume, decimals, text) in [
            (0.1 + 0.2, Some(2), "0.30"),
            (0.1 + 0.2, None, "0.3"),
            (1234.0, None, "1234"),
            (2500000.5, Some(2), "2500000.50"),
            (0.1 + 0.2, Some(15), "0.300000000000"),
        ] {
            let bar = Bar::new(DateTime::UNIX_EPOCH, 10.0, 10.0, 10.0, 10.0, volume);
            let mut chart = Chart::new(BarData::from_bars(vec![bar]));
            if let Some(decimals) = decimals {
                chart.set_instrument(Some(Instrument::new("TEST").with_volume_decimals(decimals)));
            }
            chart.add_indicator(Box::new(
                CustomIndicator::new("Volume", |bars| {
                    bars.iter()
                        .map(|bar| IndicatorValue::Single(bar.volume))
                        .collect()
                })
                .with_value_format(StudyFormat::Volume),
            ));
            let table = chart.data_table();
            let csv = chart.export_csv().unwrap();
            let record = csv::Reader::from_reader(csv.as_bytes())
                .records()
                .next()
                .unwrap()
                .unwrap();
            written.push((
                table.rows[0][5].clone(),
                record[5].to_owned(),
                table.tsv(&[5]).lines().nth(1).map(str::to_owned),
                // Without volume decimals the study keeps its own two.
                decimals.map(|_| table.rows[0][6].clone()),
            ));
            expected.push((
                text.to_owned(),
                text.to_owned(),
                Some(text.to_owned()),
                decimals.map(|_| text.to_owned()),
            ));
        }
        // Compared once, so that a failure lists every row.
        assert_eq!(written, expected);
    }

    /// An export's studies read the events the chart holds, as on screen, not the mark source page
    /// frozen into its event badges.
    #[cfg(feature = "studies")]
    #[test]
    fn exported_studies_do_not_restart_at_a_frozen_mark_source_page() {
        use crate::data::Reply;
        use crate::model::{EventKind, TimeScaleEvent};
        use crate::services::{Marks, MarksPage, MarksRequest};
        use crate::studies::{VolumeWeightedAvgPrice, VwapAnchor};
        use crate::widget::test_frames;
        use std::sync::Arc;

        struct Source(MarksPage);
        impl Marks for Source {
            fn marks(&self, _: MarksRequest, reply: Reply<MarksPage>) {
                reply.send(Ok(self.0.clone()));
            }
        }
        let ctx = egui::Context::default();
        let mut chart = test_frames::chart(&[1.0, 5.0, 9.0], &[2.0, 6.0, 10.0]);
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let time = chart.state.data().bars[70].time;
        let id = chart.add_indicator(Box::new(
            VolumeWeightedAvgPrice::new().with_anchor(VwapAnchor::Earnings),
        ));
        chart.set_mark_source(Some(Arc::new(Source(MarksPage {
            bar_marks: vec![],
            time_scale_marks: vec![],
            events: vec![TimeScaleEvent {
                id: "earnings".into(),
                time,
                kind: EventKind::Earnings,
                title: String::new(),
                details: String::new(),
            }],
        }))));
        chart.pump_marks(&ctx);

        let mut snapshot = chart.frozen_export_snapshot();
        assert_eq!(snapshot.time_scale_events.len(), 1);
        test_frames::frame(&ctx, &mut snapshot, 0.1, vec![]);
        let value = |chart: &Chart| {
            let (_, study) = chart.indicators().find(|(study, _)| *study == id).unwrap();
            (study.values().len(), study.values().value(0, 80))
        };
        // The chart holds no earnings event, so its VWAP has no value; a restart at the frozen
        // page's event on bar 70 would give bar 80 one.
        assert_eq!(value(&chart), (100, None));
        assert_eq!(value(&snapshot), value(&chart));
    }

    /// Copies retain a linked study's explicit price-pane placement and main scale.
    #[cfg(feature = "ui")]
    #[test]
    fn copies_keep_a_linked_study_where_the_user_put_it() {
        use crate::studies::{InputValue, MACD, RSI, SMA, StudyScale, StudySource};
        use crate::widget::{PaneId, test_frames};

        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.add_indicator(Box::new(RSI::new(14)));
        let macd = chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
        let sma = chart.add_indicator(Box::new(SMA::new(9)));
        chart
            .indicator_mut(sma)
            .unwrap()
            .set_input(
                "source",
                InputValue::Source(StudySource::Study {
                    id: macd.0,
                    plot: 0,
                }),
            )
            .unwrap();
        assert!(chart.move_indicator_to_pane(sma, SeriesId::MAIN));
        assert!(chart.set_indicator_scale(sma, StudyScale::Main));
        let mut restored = test_frames::chart(&[95.0], &[105.0]);
        restored.restore(&chart.snapshot().unwrap()).unwrap();
        let restored_id = restored.restored_series_id(sma.0).unwrap();
        for (copy, id) in [
            (chart.frozen_export_snapshot(), sma),
            (restored, restored_id),
        ] {
            let study = copy.indicators().find(|(study, _)| *study == id).unwrap().1;
            assert!(study.is_overlay());
            assert_eq!(study.scale(), StudyScale::Main);
            assert_eq!(copy.series_pane(id), Some(PaneId::MAIN));
        }
    }

    #[test]
    fn images_retain_loaded_marks_and_event_visibility_without_fetching() {
        use crate::data::Reply;
        use crate::model::{EventKind, EventVisibility, Marker, TimeScaleEvent};
        use crate::services::{Marks, MarksPage, MarksRequest};
        use crate::widget::test_frames;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        struct Source {
            page: MarksPage,
            calls: AtomicUsize,
        }
        impl Marks for Source {
            fn marks(&self, _: MarksRequest, reply: Reply<MarksPage>) {
                self.calls.fetch_add(1, Ordering::Relaxed);
                reply.send(Ok(self.page.clone()));
            }
        }
        let ctx = egui::Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let time = chart.state.shown()[70].time;
        let event = TimeScaleEvent {
            id: "earnings".into(),
            time,
            kind: EventKind::Earnings,
            title: "Host earnings".into(),
            details: String::new(),
        };
        chart.set_markers(SeriesId::MAIN, vec![Marker::new(time).with_text("Manual")]);
        chart.set_events(vec![Marker::event(time, "Manual axis")]);
        chart.set_time_scale_events(vec![event.clone()]);
        chart.set_event_visibility(EventVisibility {
            show_earnings: true,
            show_news: false,
            ..Default::default()
        });
        let source = Arc::new(Source {
            page: MarksPage {
                bar_marks: vec![Marker::new(time).with_text("Source bar")],
                time_scale_marks: vec![Marker::event(time, "Source axis")],
                events: vec![TimeScaleEvent {
                    kind: EventKind::News,
                    ..event
                }],
            },
            calls: AtomicUsize::new(0),
        });
        chart.set_mark_source(Some(source.clone()));
        chart.pump_marks(&ctx);
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);

        let mut snapshot = chart.frozen_export_snapshot();
        assert!(snapshot.mark_source.is_none());
        assert_eq!(snapshot.event_visibility(), chart.event_visibility());
        assert_eq!(snapshot.time_scale_events.len(), 2);
        let shapes = test_frames::shapes(&ctx, &mut snapshot, 0.1, vec![]);
        for label in ["Manual", "Manual axis", "Source bar", "Source axis", "E"] {
            assert!(
                shapes.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text) if text.galley.text() == label)),
                "missing {label}"
            );
        }
        assert!(!shapes.iter().any(|shape| matches!(shape,
            egui::Shape::Text(text) if text.galley.text() == "N")));
        let mut image = chart
            .export_image(&ctx, &ImageExportOptions::default())
            .unwrap();
        image.textures_delta.clear();
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
        assert_eq!(chart.marks.len(), 1);
        assert_eq!(chart.events.len(), 1);
        assert_eq!(chart.time_scale_events.len(), 1);
    }

    #[test]
    #[cfg(feature = "drawings")]
    fn images_apply_options_and_drawing_visibility_without_changing_the_live_view() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::new());
        chart.symbol = "EXPORT TEST".into();
        chart.add_indicator(Box::new(
            CustomIndicator::new("Pane", |_| vec![]).with_overlay(false),
        ));
        ctx.run_ui(Default::default(), |_| {})
            .drop_without_applying_deltas();
        let before = chart.get_time_scale_state();
        let vertices = |image: &ChartImage| {
            image
                .primitives
                .iter()
                .map(|primitive| match &primitive.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    _ => 0,
                })
                .sum::<usize>()
        };
        let base = ImageExportOptions {
            size: [480, 320],
            include_title: false,
            include_timestamp: false,
            include_watermark: true,
            transparent_background: false,
            include_crosshair: false,
            ..Default::default()
        };
        let mut opaque = chart.export_image(&ctx, &base).unwrap();
        let has_textures = !opaque.textures_delta.set.is_empty();
        opaque.textures_delta.clear();
        assert_eq!(opaque.size, [480, 320]);
        assert!(has_textures);
        for (title, timestamp) in [(true, false), (false, true)] {
            let mut image = chart
                .export_image(
                    &ctx,
                    &ImageExportOptions {
                        include_title: title,
                        include_timestamp: timestamp,
                        ..base.clone()
                    },
                )
                .unwrap();
            image.textures_delta.clear();
            assert!(
                vertices(&image) > vertices(&opaque),
                "missing requested header"
            );
        }
        let mut transparent = chart
            .export_image(
                &ctx,
                &ImageExportOptions {
                    transparent_background: true,
                    ..base.clone()
                },
            )
            .unwrap();
        transparent.textures_delta.clear();
        assert!(
            vertices(&transparent) < vertices(&opaque),
            "backgrounds were retained"
        );
        assert_eq!(chart.get_time_scale_state(), before);
        assert!(
            chart.last_rendered_price_rect.is_negative(),
            "live geometry changed"
        );
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType, TimeframeVisibility};
        let mut drawing = Drawing::new(1, DrawingToolType::HorizontalLine);
        drawing.completed = true;
        drawing.chart_points = vec![ChartPoint::new(0.0, 10.0)];
        drawing.color = [71, 89, 123, 255];
        drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        chart.drawings.drawings.push(drawing);
        chart.set_data(
            SeriesId::MAIN,
            vec![Bar::new(
                DateTime::from_timestamp(0, 0).unwrap(),
                10.0,
                11.0,
                9.0,
                10.0,
                1.0,
            )],
        );
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            chart.theme_override = Some(theme);
            for (timeframe, visible) in [(Timeframe::Min1, false), (Timeframe::Hour1, true)] {
                chart.set_timeframe(timeframe);
                let mut image = chart.export_image(&ctx, &base).unwrap();
                image.textures_delta.clear();
                assert_eq!(image.primitives.iter().any(|primitive| {
                    matches!(&primitive.primitive, egui::epaint::Primitive::Mesh(mesh)
                        if mesh.vertices.iter().any(|vertex| vertex.color == egui::Color32::from_rgb(71, 89, 123)))
                }), visible, "drawing visibility at {timeframe:?}");
            }
        }
        // The limits apply to the output pixels, the size times the density.
        for (size, pixel_density, expected) in [
            ([0, 320], PixelDensity::One, Some([0, 320])),
            ([480, u32::MAX], PixelDensity::One, Some([480, u32::MAX])),
            ([7680, 7680], PixelDensity::One, Some([7680, 7680])),
            ([50, 50], PixelDensity::Two, None),
            ([33, 200], PixelDensity::Three, Some([99, 600])),
            ([3841, 100], PixelDensity::Two, Some([7682, 200])),
            ([3840, 2161], PixelDensity::Two, Some([7680, 4322])),
            ([480, u32::MAX], PixelDensity::Three, Some([1440, u32::MAX])),
        ] {
            let image = chart.export_image(
                &ctx,
                &ImageExportOptions {
                    size,
                    pixel_density,
                    ..base.clone()
                },
            );
            assert_eq!(
                image.map(|mut image| {
                    image.textures_delta.clear();
                    image.size
                }),
                match expected {
                    Some([width, height]) => Err(ExportError::InvalidSize { width, height }),
                    None => Ok([100, 100]),
                },
                "{size:?} at {pixel_density:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "studies")]
    fn data_table_options_choose_rows_columns_and_values() {
        use crate::studies::{SMA, WilliamsAlligator};
        let t0 = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let hour = chrono::Duration::hours(1);
        let chart_with = |studies: Vec<Box<dyn crate::studies::Indicator>>| {
            let bars = (0..100)
                .map(|i| Bar::new(t0 + hour * i, 1234.5, 1235.5, 1233.5, 1234.5, 1.0))
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Hour1);
            chart.chart_options.time_scale.timezone = crate::scales::TimezoneMode::Utc;
            let ids: Vec<_> = studies
                .into_iter()
                .map(|study| chart.add_indicator(study))
                .collect();
            (chart, ids)
        };
        // Two identical SMA 9, a hidden SMA 20 and an Alligator whose lips are hidden: every
        // value is 1234.5, the median of each bar.
        let (mut chart, ids) = chart_with(vec![
            Box::new(SMA::new(9)),
            Box::new(SMA::new(9)),
            Box::new(SMA::new(20)),
            Box::new(WilliamsAlligator::default()),
        ]);
        let [a, b, h, w] = ids[..] else {
            unreachable!()
        };
        chart.set_indicator_visible(h, false);
        let alligator = chart.indicator_mut(w).unwrap();
        let mut settings = alligator.settings();
        settings.plots[2].visible = false;
        alligator.apply_settings(&[], settings).unwrap();

        let main = (0..5).map(|plot| DataColumn::Series {
            id: SeriesId::MAIN,
            plot,
        });
        let study = |id, plot| DataColumn::Study { id, plot };
        let shown: Vec<_> = std::iter::once(DataColumn::Time)
            .chain(main.clone())
            .chain([study(a, 0), study(b, 0), study(w, 0), study(w, 1)])
            .collect();
        let all: Vec<_> = std::iter::once(DataColumn::Time)
            .chain(main)
            .chain([
                study(a, 0),
                study(b, 0),
                study(h, 0),
                study(w, 0),
                study(w, 1),
                study(w, 2),
            ])
            .collect();
        let (t99, t100) = ("2024-01-05T03:00:00+00:00", "2024-01-05T04:00:00+00:00");
        let value = "1234.50";
        let cases = [
            (
                "default",
                DataExportOptions::default(),
                shown.clone(),
                100,
                vec![(
                    99,
                    vec![
                        t99, "1234.50", "1235.50", "1233.50", "1234.50", "1", value, value, value,
                        value,
                    ],
                )],
                Vec::new(),
            ),
            (
                "hidden",
                DataExportOptions {
                    include_hidden_studies: true,
                    ..Default::default()
                },
                all.clone(),
                100,
                Vec::new(),
                vec![(99, 8, value), (99, 11, value)],
            ),
            (
                "past",
                DataExportOptions {
                    include_values_past_last_bar: true,
                    ..Default::default()
                },
                shown.clone(),
                108,
                vec![(100, vec![t100, "", "", "", "", "", "", "", value, value])],
                vec![
                    (104, 9, value),
                    (105, 9, ""),
                    (105, 8, value),
                    (107, 0, "2024-01-05T11:00:00+00:00"),
                ],
            ),
            (
                "hidden and past",
                DataExportOptions {
                    include_hidden_studies: true,
                    include_values_past_last_bar: true,
                    ..Default::default()
                },
                all,
                108,
                Vec::new(),
                vec![(102, 11, value), (103, 11, ""), (103, 10, value)],
            ),
            (
                "one study without time or main series",
                DataExportOptions {
                    studies: Some(vec![b]),
                    include_time: false,
                    include_main_series: false,
                    ..Default::default()
                },
                vec![study(b, 0)],
                100,
                vec![(8, vec![value]), (99, vec![value])],
                vec![(7, 0, "")],
            ),
            (
                "range",
                DataExportOptions {
                    from: Some(t0 + hour * 10),
                    to: Some(t0 + hour * 19),
                    ..Default::default()
                },
                shown.clone(),
                10,
                Vec::new(),
                vec![
                    (0, 0, "2024-01-01T10:00:00+00:00"),
                    (9, 0, "2024-01-01T19:00:00+00:00"),
                ],
            ),
            (
                "range past the last bar",
                DataExportOptions {
                    include_values_past_last_bar: true,
                    from: Some(t0 + hour * 100),
                    to: Some(t0 + hour * 101),
                    ..Default::default()
                },
                shown.clone(),
                2,
                Vec::new(),
                vec![(0, 0, t100), (1, 0, "2024-01-05T05:00:00+00:00")],
            ),
            (
                "to before from",
                DataExportOptions {
                    from: Some(t0 + hour * 20),
                    to: Some(t0 + hour * 10),
                    ..Default::default()
                },
                shown.clone(),
                0,
                Vec::new(),
                Vec::new(),
            ),
            (
                "displayed",
                DataExportOptions {
                    values: DataValues::Displayed,
                    ..Default::default()
                },
                shown.clone(),
                100,
                vec![(
                    99,
                    vec![
                        t99, "1,234.50", "1,235.50", "1,233.50", "1,234.50", "1", "1,234.50",
                        "1,234.50", "1,234.50", "1,234.50",
                    ],
                )],
                Vec::new(),
            ),
            (
                "raw",
                DataExportOptions {
                    values: DataValues::Raw,
                    ..Default::default()
                },
                shown,
                100,
                vec![(
                    99,
                    vec![
                        t99, "1234.5", "1235.5", "1233.5", "1234.5", "1", "1234.5", "1234.5",
                        "1234.5", "1234.5",
                    ],
                )],
                Vec::new(),
            ),
        ];
        for (name, options, columns, rows, full, cells) in cases {
            let table = chart.data_table_with(&options);
            assert_eq!(table.columns, columns, "{name}");
            assert_eq!(table.headers.len(), columns.len(), "{name}");
            assert_eq!(table.rows.len(), rows, "{name}");
            for (row, expected) in full {
                assert_eq!(table.rows[row], expected, "{name}: row {row}");
            }
            for (row, column, expected) in cells {
                assert_eq!(table.rows[row][column], expected, "{name}: {row}, {column}");
            }
        }
        // The two SMA 9 share a header and are told apart by id.
        let table = chart.data_table();
        assert_eq!(table.headers[6], table.headers[7]);
        assert_eq!(table, chart.data_table_with(&DataExportOptions::default()));

        // A jaw displaced past any representable time lists no row, and its candidates stay as
        // few as its values.
        let (chart, _) = chart_with(vec![Box::new(WilliamsAlligator::default().with_offsets(
            isize::MAX as usize,
            5,
            3,
        ))]);
        let table = chart.data_table_with(&DataExportOptions {
            include_values_past_last_bar: true,
            ..Default::default()
        });
        assert_eq!(table.rows.len(), 105);
        assert!(table.rows.iter().all(|row| row[6].is_empty()));
    }

    #[test]
    fn table_text_holds_the_chosen_columns_with_their_missing_cells() {
        let table = DataTable {
            headers: vec!["time".into(), "close".into(), "SMA: 5".into()],
            columns: Vec::new(),
            rows: vec![
                vec!["t0".into(), "1.50".into(), String::new()],
                vec!["t1".into(), "2.50".into(), "2.00".into()],
            ],
        };
        #[cfg(feature = "csv")]
        assert_eq!(table.csv(&[0, 2]).unwrap(), "time,SMA: 5\nt0,\nt1,2.00\n");
        assert_eq!(table.tsv(&[2, 1]), "SMA: 5\tclose\n\t1.50\n2.00\t2.50");
    }

    #[test]
    #[cfg(feature = "ui")]
    fn svg_escapes_a_restored_axis_font_family() {
        use crate::widget::{ChartLayoutMode, SavedLayout, test_frames};
        use egui::FontFamily;

        for (name, escaped) in [
            (
                r#"R&D "Mono" <x> it's \ here"#,
                r#"R&amp;D &quot;Mono&quot; &lt;x&gt; it\'s \\ here"#,
            ),
            ("Line\nfeed\rreturn\ttab", r"Line\a feed\d return\9 tab"),
            ("Control\0\u{1}\u{b}\u{c}\u{1f}", r"Control\0 \1 \b \c \1f "),
            ("Unicode\u{fffe}\u{ffff}", r"Unicode\fffe \ffff "),
        ] {
            let ctx = egui::Context::default();
            let family = FontFamily::Name(name.into());
            let control = FontFamily::Name("Host Mono".into());
            let mut fonts = egui::FontDefinitions::default();
            fonts.families.insert(
                family.clone(),
                fonts.families[&FontFamily::Monospace].clone(),
            );
            fonts
                .families
                .insert(control.clone(), fonts.families[&family].clone());
            ctx.set_fonts(fonts);
            let mut chart = test_frames::chart(&[100.0], &[110.0]);
            test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
            let options = ImageExportOptions {
                size: [480, 320],
                include_title: false,
                include_timestamp: false,
                include_watermark: false,
                ..Default::default()
            };
            chart.config_mut().axis_font_family = family.clone();
            let json = SavedLayout {
                mode: ChartLayoutMode::Single,
                charts: vec![chart.snapshot().unwrap()],
                sizing: None,
                link_groups: None,
            }
            .to_json()
            .unwrap();
            let saved = SavedLayout::from_json(&json).unwrap();
            chart.config_mut().axis_font_family = FontFamily::Proportional;
            chart.restore(&saved.charts[0]).unwrap();
            assert_eq!(chart.config().axis_font_family, family);
            test_frames::frame(&ctx, &mut chart, 0.1, vec![]);

            chart.config_mut().axis_font_family = control;
            let text_count = chart
                .export_svg(&ctx, &options)
                .unwrap()
                .matches("<text ")
                .count();
            assert!(text_count > 0);
            chart.config_mut().axis_font_family = family;
            let svg = chart.export_svg(&ctx, &options).unwrap();
            resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
            assert!(svg.contains(&format!("font-family=\"'{escaped}', sans-serif\"")));
            assert_eq!(svg.matches("<text ").count(), text_count);
        }
    }

    #[test]
    #[cfg(feature = "ui")]
    fn svg_writes_a_restored_watermark_with_control_characters_as_xml() {
        use crate::widget::{ChartLayoutMode, SavedLayout, test_frames};

        let ctx = egui::Context::default();
        let options = ImageExportOptions {
            size: [480, 320],
            include_title: false,
            include_timestamp: false,
            include_watermark: true,
            ..Default::default()
        };
        for bad in ['\u{1}', '\u{b}', '\u{fffe}', '\r'] {
            let mut chart = test_frames::chart(&[10.0, 12.0], &[14.0, 13.0]);
            chart.config_mut().show_watermark = true;
            chart.config_mut().watermark_text = Some(format!("WATER{bad}MARK"));
            let json = SavedLayout {
                mode: ChartLayoutMode::Single,
                charts: vec![chart.snapshot().unwrap()],
                sizing: None,
                link_groups: None,
            }
            .to_json()
            .unwrap();
            let saved = SavedLayout::from_json(&json).unwrap();
            let mut chart = test_frames::chart(&[10.0, 12.0], &[14.0, 13.0]);
            chart.restore(&saved.charts[0]).unwrap();
            test_frames::frame(&ctx, &mut chart, 0.0, vec![]);

            let svg = chart.export_svg(&ctx, &options).unwrap();
            resvg::usvg::Tree::from_str(&svg, &Default::default())
                .unwrap_or_else(|e| panic!("U+{:04X}: {e}", bad as u32));
            assert!(!svg.contains(bad), "U+{:04X} was written raw", bad as u32);
            let kept = if bad == '\r' {
                "WATER&#13;MARK"
            } else {
                "WATERMARK"
            };
            assert!(svg.contains(kept), "U+{:04X}", bad as u32);
        }
    }

    #[test]
    #[cfg(feature = "png-export")]
    fn svg_images_apply_the_options_and_report_what_they_cannot_hold() {
        use crate::widget::test_frames::{chart, frame};
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0, 12.0], &[14.0, 13.0]);
        chart.symbol = "EXPORT TEST".into();
        chart.config.show_watermark = true;
        chart.config.watermark_text = Some("WATERMARK".into());
        frame(&ctx, &mut chart, 0.0, vec![]);
        // A colour of the chart's own, which no theme supplies.
        chart.config.background_color = egui::Color32::from_rgb(120, 10, 10);
        let base = ImageExportOptions {
            size: [480, 320],
            include_title: false,
            include_timestamp: false,
            include_watermark: true,
            transparent_background: false,
            include_crosshair: false,
            ..Default::default()
        };
        let svg = |options: &ImageExportOptions| chart.export_svg(&ctx, options).unwrap();
        let opaque = svg(&base);
        assert!(opaque.starts_with("<svg") && opaque.contains(r#"width="480" height="320""#));
        assert!(opaque.contains("WATERMARK"), "watermark was dropped");
        #[cfg(feature = "icons")]
        assert!(
            opaque.contains("data:image/png;base64,"),
            "frame textures were dropped"
        );
        assert!(!opaque.contains("EXPORT TEST"));
        let no_watermark = svg(&ImageExportOptions {
            include_watermark: false,
            include_title: true,
            ..base.clone()
        });
        assert!(!no_watermark.contains("WATERMARK") && no_watermark.contains("EXPORT TEST"));
        let [r, g, b, _] = chart.config.background_color.to_srgba_unmultiplied();
        let bg = format!(r#"fill="rgb({r},{g},{b})""#);
        assert!(opaque.contains(&bg));
        let transparent = svg(&ImageExportOptions {
            transparent_background: true,
            ..base.clone()
        });
        assert!(!transparent.contains(&bg), "background was retained");
        // The header writes the export time as the chart writes times, to the second on
        // daily bars too, in the chart's zone, which it names: #4063.
        for (zone, offset, use_24_hour, locale, timeframe, date_format, expected) in [
            (
                crate::scales::TimezoneMode::Utc,
                0,
                true,
                None,
                Some(crate::model::Timeframe::Min1),
                "%d.%m.%Y",
                "%d.%m.%Y %H:%M:%S UTC",
            ),
            (
                crate::scales::TimezoneMode::Timezone("-03:30".parse().unwrap()),
                -210,
                false,
                None,
                None,
                "%d.%m.%Y",
                "%d.%m.%Y %I:%M:%S %p -03:30",
            ),
            (
                crate::scales::TimezoneMode::Utc,
                0,
                true,
                Some("de"),
                Some(crate::model::Timeframe::Week1),
                "%a %d.%m.%Y",
                "%a %d.%m.%Y %H:%M:%S UTC",
            ),
        ] {
            chart.chart_options.time_scale.timezone = zone;
            chart.chart_options.time_scale.use_24_hour = use_24_hour;
            chart.set_locale(locale.map(str::to_owned));
            chart.timeframe = timeframe;
            chart.set_date_format(Some(date_format.to_owned()));
            let before = Utc::now();
            let header = chart
                .export_svg(
                    &ctx,
                    &ImageExportOptions {
                        include_timestamp: true,
                        ..base.clone()
                    },
                )
                .unwrap();
            let after = Utc::now();
            // Every second the export may have read, written by chrono in the row's zone.
            let texts: Vec<_> = (before.timestamp()..=after.timestamp())
                .map(|second| {
                    let wall = DateTime::from_timestamp(second, 0).unwrap()
                        + chrono::Duration::minutes(offset);
                    #[cfg(feature = "locales")]
                    if locale.is_some() {
                        return wall
                            .format_localized(expected, chrono::Locale::de_DE)
                            .to_string();
                    }
                    wall.format(expected).to_string()
                })
                .collect();
            assert!(
                texts
                    .iter()
                    .any(|text| header.contains(&format!(">{text}</text>"))),
                "{texts:?}"
            );
        }
        let callback = |ui: &mut egui::Ui, _: &mut Chart| {
            ui.painter().add(egui::PaintCallback {
                rect: ui.max_rect(),
                callback: std::sync::Arc::new(()),
            });
        };
        let host_texture = |ui: &mut egui::Ui, _: &mut Chart| {
            ui.painter().image(
                egui::TextureId::User(7),
                ui.max_rect(),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        };
        for (error, reason) in [
            (chart.export_layout_svg(&ctx, &base, callback), "callbacks"),
            (
                chart
                    .export_layout_png(&ctx, &base, callback)
                    .map(|_| String::new()),
                "callbacks",
            ),
            (
                chart.export_layout_svg(&ctx, &base, host_texture),
                "host textures",
            ),
            (
                chart
                    .export_layout_png(&ctx, &base, host_texture)
                    .map(|_| String::new()),
                "host textures",
            ),
        ] {
            let Err(ExportError::Unsupported(error)) = error else {
                panic!("{reason}")
            };
            assert!(error.contains(reason), "{error}");
        }
        let red = egui::Color32::from_rgb(200, 30, 30);
        let green = egui::Color32::from_rgb(30, 200, 30);
        let high_id = egui::TextureId::User(7 | 1 << 63);
        let supplied = ImageExportOptions {
            textures: HashMap::from([
                (
                    egui::TextureId::User(7),
                    Arc::new(egui::ColorImage::new([2, 1], vec![red, green])),
                ),
                (high_id, Arc::new(egui::ColorImage::filled([1, 1], green))),
            ]),
            ..base.clone()
        };
        let additive = ImageExportOptions {
            textures: HashMap::from([(
                egui::TextureId::User(7),
                Arc::new(egui::ColorImage::filled(
                    [1, 1],
                    egui::Color32::from_rgb_additive(200, 0, 0),
                )),
            )]),
            ..base.clone()
        };
        let error = chart
            .export_layout_svg(&ctx, &additive, host_texture)
            .unwrap_err();
        let ExportError::Unsupported(error) = error else {
            panic!("expected an unsupported texture")
        };
        assert!(error.contains("Additive texture User(7)"), "{error}");
        let layout = |ui: &mut egui::Ui, chart: &mut Chart| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(128.0, 32.0), egui::Sense::hover());
            ui.painter().image(
                egui::TextureId::User(7),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            ui.label("LAYOUT TOOLBAR");
            ui.painter().add(
                egui::epaint::TextShape::new(
                    egui::pos2(300.0, 20.0),
                    ui.painter().layout_no_wrap(
                        "TURNED LABEL".into(),
                        egui::FontId::proportional(12.0),
                        egui::Color32::WHITE,
                    ),
                    egui::Color32::WHITE,
                )
                .with_angle(-std::f32::consts::FRAC_PI_2),
            );
            for (id, x) in [(high_id, 160.0), (egui::TextureId::User(7), 208.0)] {
                ui.painter().image(
                    id,
                    egui::Rect::from_min_size(egui::pos2(x, 8.0), egui::vec2(32.0, 32.0)),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::from_rgba_premultiplied(128, 64, 32, 128),
                );
            }
            chart.show(ui);
        };
        let svg = chart.export_layout_svg(&ctx, &supplied, layout).unwrap();
        assert!(svg.contains("LAYOUT TOOLBAR") && svg.contains("WATERMARK"));
        // egui's clockwise -90 degrees is SVG's clockwise rotate(-90), about the text's position.
        let (_, group) = svg.split_once(r#"<g transform="rotate("#).unwrap();
        let (rotation, group) = group.split_once(r#")">"#).unwrap();
        let rotation: Vec<f32> = rotation.split(' ').map(|n| n.parse().unwrap()).collect();
        for (value, expected) in rotation.iter().zip([-90.0, 300.0, 20.0]) {
            assert!((value - expected).abs() < 1e-3, "{rotation:?}");
        }
        assert!(group.split_once("</g>").unwrap().0.contains("TURNED LABEL"));
        assert!(svg.contains(r#"width="2" height="1""#));
        for id in [7, 7 | 1u64 << 63] {
            assert!(svg.contains(&format!(r#"id="tu{id}_"#)));
        }
        #[cfg(feature = "icons")]
        {
            let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
            let mut groups = vec![tree.root()];
            let mut colors = Vec::new();
            while let Some(group) = groups.pop() {
                for node in group.children() {
                    match node {
                        resvg::usvg::Node::Group(group) => groups.push(group),
                        resvg::usvg::Node::Image(image) => {
                            let resvg::usvg::ImageKind::PNG(data) = image.kind() else {
                                panic!("expected an embedded PNG");
                            };
                            let mut reader =
                                png::Decoder::new(data.as_slice()).read_info().unwrap();
                            let mut pixels = vec![0; reader.output_buffer_size()];
                            reader.next_frame(&mut pixels).unwrap();
                            let [r, g, b, a] = pixels[..4].try_into().unwrap();
                            colors
                                .push(egui::Color32::from_rgba_unmultiplied(r, g, b, a).to_array());
                        }
                        _ => {}
                    }
                }
            }
            // Rounded products of source and tint channels / 255, with alpha 255 * 128 / 255.
            for expected in [[15, 50, 4, 128], [100, 8, 4, 128], red.to_array()] {
                assert!(
                    colors.contains(&expected),
                    "missing {expected:?} in {colors:?}"
                );
            }
        }
        let png = chart.export_layout_png(&ctx, &supplied, layout).unwrap();
        let mut reader = png::Decoder::new(png.as_slice()).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut pixels).unwrap();
        // Both samples lie in the edge-clamped halves of the two-texel image.
        assert_eq!(pixels[(20 * 480 + 20) * 4..][..4], red.to_array());
        assert_eq!(pixels[(20 * 480 + 120) * 4..][..4], green.to_array());
        for (id, size, pixels, reason) in [
            (
                egui::TextureId::User(7),
                [0, 1],
                vec![],
                "invalid dimensions",
            ),
            (
                egui::TextureId::User(7),
                [2, 1],
                vec![red],
                "invalid dimensions",
            ),
            (egui::TextureId::Managed(0), [1, 1], vec![red], "conflicts"),
        ] {
            let options = ImageExportOptions {
                textures: HashMap::from([(
                    id,
                    Arc::new(egui::ColorImage {
                        size,
                        source_size: egui::vec2(size[0] as f32, size[1] as f32),
                        pixels,
                    }),
                )]),
                ..base.clone()
            };
            for error in [
                chart.export_svg(&ctx, &options).unwrap_err(),
                chart.export_png(&ctx, &options).unwrap_err(),
            ] {
                let ExportError::Unsupported(error) = error else {
                    panic!("{reason}")
                };
                assert!(
                    error.contains(reason) && error.contains(&format!("{id:?}")),
                    "{error}"
                );
            }
        }
    }

    #[test]
    #[cfg(not(feature = "png-export"))]
    fn svg_exports_vectors_and_refuses_textures_without_png_export() {
        use crate::widget::test_frames::{chart, frame};
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0, 12.0], &[14.0, 13.0]);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let options = ImageExportOptions {
            size: [480, 320],
            include_timestamp: false,
            ..Default::default()
        };
        assert!(
            chart
                .export_layout_svg(&ctx, &options, |ui, _| {
                    ui.label("Vector text");
                })
                .unwrap()
                .contains("Vector text")
        );
        let id = egui::TextureId::User(7);
        let options = ImageExportOptions {
            textures: HashMap::from([(
                id,
                Arc::new(egui::ColorImage::filled([1, 1], egui::Color32::RED)),
            )]),
            ..options
        };
        assert_eq!(
            chart.export_layout_svg(&ctx, &options, |ui, _| {
                ui.image((id, egui::vec2(32.0, 32.0)));
            }),
            Err(ExportError::Unsupported(
                "SVG image texture User(7) requires the png-export feature".into()
            ))
        );
    }

    #[test]
    #[cfg(feature = "png-export")]
    fn image_exports_control_foreground_and_crosshair_independently() {
        use crate::chart::primitives::{Primitive, PrimitivePass, PrimitiveView};
        use crate::studies::{CustomIndicator, IndicatorValue};
        use crate::widget::test_frames::{chart, frame};
        use egui::{Color32, Event, Pos2};

        struct Layers([[Color32; 3]; 2]);
        impl Layers {
            fn paint(
                &self,
                painter: &egui::Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
                background: bool,
            ) {
                let at = match pass {
                    PrimitivePass::Below => 0,
                    PrimitivePass::Series => 1,
                    PrimitivePass::Above => 2,
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        Pos2::new(
                            view.mapping.rect.left() + 16.0 + at as f32 * 24.0,
                            view.mapping.rect.bottom() - if background { 32.0 } else { 16.0 },
                        ),
                        egui::vec2(12.0, 12.0),
                    ),
                    0.0,
                    self.0[background as usize][at],
                );
            }
        }
        impl Primitive for Layers {
            fn draw_background(
                &self,
                painter: &egui::Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
            ) {
                self.paint(painter, view, pass, true);
            }
            fn draw(&self, painter: &egui::Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
                self.paint(painter, view, pass, false);
            }
        }

        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        frame(
            &ctx,
            &mut chart,
            0.0,
            vec![Event::PointerMoved(Pos2::new(400.0, 200.0))],
        );
        frame(&ctx, &mut chart, 0.1, vec![Event::PointerGone]);
        let options = |include_foreground, include_crosshair| ImageExportOptions {
            size: [480, 320],
            include_title: false,
            include_timestamp: false,
            include_crosshair,
            include_foreground,
            ..Default::default()
        };
        let decode = |chart: &Chart, include_foreground, include_crosshair| {
            let png = chart
                .export_png(&ctx, &options(include_foreground, include_crosshair))
                .unwrap();
            let mut reader = png::Decoder::new(png.as_slice()).read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            assert_eq!(
                (info.width, info.height, info.color_type),
                (480, 320, png::ColorType::Rgba)
            );
            pixels.as_chunks::<4>().0.to_vec()
        };
        let (plain, crosshair) = (decode(&chart, true, false), decode(&chart, true, true));
        // Candle bodies keep their exact colour through rasterisation and encoding.
        let bullish = chart.config().bullish_color.to_srgba_unmultiplied();
        assert!(plain.iter().filter(|pixel| **pixel == bullish).count() > 1000);
        // The crosshair where the pointer left adds a vertical and a horizontal line.
        let changed = |x: usize, y: usize| plain[y * 480 + x] != crosshair[y * 480 + x];
        let column = (0..480).map(|x| (0..320).filter(|&y| changed(x, y)).count());
        let row = (0..320).map(|y| (0..480).filter(|&x| changed(x, y)).count());
        assert!(column.max() > Some(60) && row.max() > Some(100));

        chart.chart_options.crosshair.vert_label_background_color = egui::Color32::RED;
        chart.chart_options.crosshair.horz_label_background_color = egui::Color32::BLUE;
        let (time, _) = chart.last_crosshair.unwrap();
        for price in [Some(105.0), None] {
            chart.last_crosshair = Some((time, price));
            let crosshair = decode(&chart, true, true);
            assert!(crosshair.contains(&[255, 0, 0, 255]));
            assert_eq!(crosshair.contains(&[0, 0, 255, 255]), price.is_some());
        }

        let pane = chart.add_indicator(Box::new(
            CustomIndicator::new("Pane", |bars| {
                bars.iter()
                    .map(|bar| IndicatorValue::Single(bar.close))
                    .collect()
            })
            .with_overlay(false),
        ));
        let colors = [
            [
                [
                    Color32::from_rgb(1, 2, 3),
                    Color32::from_rgb(4, 5, 6),
                    Color32::from_rgb(7, 8, 9),
                ],
                [
                    Color32::from_rgb(21, 22, 23),
                    Color32::from_rgb(24, 25, 26),
                    Color32::from_rgb(27, 28, 29),
                ],
            ],
            [
                [
                    Color32::from_rgb(11, 12, 13),
                    Color32::from_rgb(14, 15, 16),
                    Color32::from_rgb(17, 18, 19),
                ],
                [
                    Color32::from_rgb(31, 32, 33),
                    Color32::from_rgb(34, 35, 36),
                    Color32::from_rgb(37, 38, 39),
                ],
            ],
        ];
        for (id, colors) in [SeriesId::MAIN, pane].into_iter().zip(colors) {
            assert!(chart.attach_primitive(id, Layers(colors)).is_some());
        }
        #[cfg(feature = "drawings")]
        {
            use crate::drawings::{Drawing, DrawingToolType};
            let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
            drawing.color = [45, 201, 173, 255];
            drawing.stroke_width = 3.0;
            let id = chart.create_drawing(drawing, &[(time, 103.0)]).unwrap();
            chart.drawings.select(id);
        }
        chart.selection.select(
            super::super::ChartElementId::Series(SeriesId::MAIN),
            Some(50),
        );
        chart.last_crosshair = Some((time, Some(105.0)));
        let mut images = Vec::new();
        for (foreground, crosshair) in [(false, false), (false, true), (true, false), (true, true)]
        {
            let options = options(foreground, crosshair);
            let pixels = decode(&chart, foreground, crosshair);
            let mut image = chart.export_image(&ctx, &options).unwrap();
            let raster = super::super::raster::rasterize(
                image.size,
                image.pixels_per_point,
                &image.primitives,
                &image.textures_delta,
            )
            .unwrap();
            image.textures_delta.clear();
            assert_eq!(
                raster
                    .iter()
                    .map(Color32::to_srgba_unmultiplied)
                    .collect::<Vec<_>>(),
                pixels
            );
            let svg = chart.export_svg(&ctx, &options).unwrap();
            for colors in colors.into_iter().flatten() {
                for (pass, color) in colors.into_iter().enumerate() {
                    let present = pass != 2 || foreground;
                    assert_eq!(
                        pixels.contains(&color.to_srgba_unmultiplied()),
                        present,
                        "{color:?}, foreground={foreground}"
                    );
                    let [r, g, b, _] = color.to_srgba_unmultiplied();
                    assert_eq!(svg.contains(&format!("rgb({r},{g},{b})")), present);
                }
            }
            for color in [Color32::RED, Color32::BLUE] {
                assert_eq!(pixels.contains(&color.to_srgba_unmultiplied()), crosshair);
                let [r, g, b, _] = color.to_srgba_unmultiplied();
                assert_eq!(svg.contains(&format!("rgb({r},{g},{b})")), crosshair);
            }
            assert!(pixels.contains(&bullish));
            #[cfg(feature = "drawings")]
            assert!(pixels.contains(&[45, 201, 173, 255]));
            images.push(pixels);
        }
        // Four 12-pixel swatches, with at most one feathered pixel along each edge.
        assert!(
            images[0]
                .iter()
                .zip(&images[2])
                .filter(|(a, b)| a != b)
                .count()
                <= 4 * 14 * 14
        );
        // Disjoint overlays change only their own pixels; the underlying frame stays identical.
        for at in 0..images[0].len() {
            let expected = if images[2][at] != images[0][at] {
                images[2][at]
            } else {
                images[1][at]
            };
            assert_eq!(images[3][at], expected, "pixel {at}");
        }
        assert!(ImageExportOptions::default().include_foreground);
        assert!(chart.selected_element().is_some());
        chart.clear_selection();
        #[cfg(feature = "drawings")]
        {
            assert!(chart.drawings.selected_ids().next().is_some());
            chart.drawings.deselect();
        }
        assert_eq!(decode(&chart, false, false), images[0]);
    }

    /// Guards #3596: a PNG at density d has d times the pixels on each side and draws the same
    /// layout, and an SVG ignores the density.
    #[test]
    #[cfg(feature = "png-export")]
    fn png_density_multiplies_pixels_and_keeps_the_layout() {
        let ctx = egui::Context::default();
        ctx.run_ui(Default::default(), |_| {})
            .drop_without_applying_deltas();
        // Every bar rises, so every candle body has the bullish colour.
        let chart = crate::widget::test_frames::chart(&[100.0, 104.0], &[110.0, 108.0]);
        let bullish = chart.config().bullish_color.to_srgba_unmultiplied();
        // The PNG's width and height and the bounding box of its candle bodies.
        let measure = |pixel_density| {
            let options = ImageExportOptions {
                size: [480, 320],
                include_title: false,
                include_timestamp: false,
                pixel_density,
                ..Default::default()
            };
            let png = chart.export_png(&ctx, &options).unwrap();
            let mut reader = png::Decoder::new(png.as_slice()).read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            let width = info.width as usize;
            let mut bbox = [usize::MAX, usize::MAX, 0, 0];
            for (at, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
                if *pixel == bullish {
                    let (x, y) = (at % width, at / width);
                    bbox = [
                        bbox[0].min(x),
                        bbox[1].min(y),
                        bbox[2].max(x + 1),
                        bbox[3].max(y + 1),
                    ];
                }
            }
            (info.width, info.height, bbox)
        };
        let measured = PixelDensity::ALL.map(measure);
        let one = measured[0].2;
        for (density, (width, height, bbox)) in PixelDensity::ALL.into_iter().zip(measured) {
            let d = density.pixels_per_point();
            assert_eq!((width, height), (480 * d, 320 * d), "{density:?}");
            // egui rounds text, and with it the axis and legend heights, to the density's pixel
            // grid, as on a screen of that density, which moves an edge by less than two points.
            let d = d as usize;
            for (edge, edge_at_one) in bbox.into_iter().zip(one) {
                assert!(
                    edge.abs_diff(edge_at_one * d) <= 2 * d,
                    "{density:?}: bodies at {bbox:?}, at one pixel per point {one:?}"
                );
            }
        }
        // 8000 by 4000 pixels would be too large, but an SVG document keeps the options' size.
        let svg = |pixel_density| {
            let options = ImageExportOptions {
                size: [4000, 2000],
                include_timestamp: false,
                pixel_density,
                ..Default::default()
            };
            chart.export_svg(&ctx, &options).unwrap()
        };
        assert_eq!(svg(PixelDensity::Two), svg(PixelDensity::One));
    }

    /// An export uses the same scales as the widget, so a main series that reserves 60 and 70
    /// points keeps them: on a new chart's 10% top and 8% bottom margins its high, 20, sits
    /// 0.1 h + 60 below the top of the plot and its low, 10, 0.08 h + 70 above the bottom.
    /// The style the host set on the main series is the one the snapshot reports.
    #[test]
    fn export_snapshot_keeps_the_main_series_style() {
        use crate::model::{AutoscaleInfo, AutoscaleMargins, AutoscaleProvider};

        let gaps = |chart: &Chart| {
            let map = chart.series_price_map(SeriesId::MAIN).unwrap();
            let (top, bottom) = map.extent();
            let height = bottom - top;
            (
                map.price_to_y(20.0) - top - (0.1 * height + 60.0),
                bottom - map.price_to_y(10.0) - (0.08 * height + 70.0),
            )
        };
        let ctx = egui::Context::default();
        let mut chart = crate::widget::test_frames::chart(&[10.0], &[20.0]);
        let style = chart.series_style_mut(SeriesId::MAIN).unwrap();
        style.price_line_visible = false;
        style.autoscale = Some(AutoscaleProvider::with_margins(|_, default| {
            Some(AutoscaleInfo {
                price_range: default?,
                margins: AutoscaleMargins {
                    top: 60.0,
                    bottom: 70.0,
                },
            })
        }));
        for step in 0..2 {
            crate::widget::test_frames::frame(&ctx, &mut chart, step as f64, vec![]);
        }
        let live = gaps(&chart);
        let mut snapshot = chart.export_snapshot(&ctx, &ImageExportOptions::default());
        let offscreen = egui::Context::default();
        for step in 0..2 {
            crate::widget::test_frames::frame(&offscreen, &mut snapshot, step as f64, vec![]);
        }
        let exported = gaps(&snapshot);
        let hidden = snapshot
            .series_style(SeriesId::MAIN)
            .map(|style| style.price_line_visible);
        assert!(
            live.0.abs() < 0.01 && live.1.abs() < 0.01,
            "live view reserves the request: {live:?}"
        );
        assert!(
            exported.0.abs() < 0.01 && exported.1.abs() < 0.01 && hidden == Some(false),
            "observed gaps {exported:?}, price line visible {hidden:?}"
        );
    }
}

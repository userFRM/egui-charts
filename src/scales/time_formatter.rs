//! Time formatting for X-axis labels.
//!
//! The [`TimeFormatter`] trait controls how timestamps are rendered as labels
//! on the time axis.  The library ships with [`DefaultTimeFormatter`],
//! [`LocaleTimeFormatter`], [`RelativeTimeFormatter`], and a
//! [`TimezoneAwareFormatter`] wrapper for automatic timezone conversion.
//!
//! Use [`TimeFormatterBuilder`] for a fluent construction API.

use super::timescale_marks::TickMarkType;
use crate::model::TimezoneId;
use chrono::{DateTime, Utc};
use std::sync::Arc;

#[cfg(test)]
use chrono::Datelike;

/// Display time and chart locale for one time-axis tick.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct TickMarkContext<'a> {
    /// Wall-clock fields in the chart's display zone, carried in a UTC value; behind UTC, bars of
    /// a day or longer given to a chart without a feed read in UTC unless they sit at the display
    /// zone's midnight. This is not a UTC instant and must not be converted to a time zone again.
    pub time: DateTime<Utc>,
    /// Calendar or clock precision selected for this tick.
    pub mark_type: TickMarkType,
    /// Active chart locale, such as `"de-DE"`; `None` uses English names.
    /// Unknown identifiers and builds without `locales` also use English names.
    pub locale: Option<&'a str>,
}

impl<'a> TickMarkContext<'a> {
    /// A tick at `time` of `mark_type` precision with English names: assign
    /// [`locale`](Self::locale) for another language.
    ///
    /// ```
    /// use chrono::{DateTime, Utc};
    /// use egui_charts::scales::{TickMarkContext, TickMarkType};
    ///
    /// let context = TickMarkContext::new(DateTime::<Utc>::UNIX_EPOCH, TickMarkType::Year);
    /// assert_eq!(context.locale, None);
    /// ```
    pub fn new(time: DateTime<Utc>, mark_type: TickMarkType) -> Self {
        Self {
            time,
            mark_type,
            locale: None,
        }
    }
}

/// Trait for custom time formatting
/// Allows users to customize how time labels are displayed on the axis
pub trait TimeFormatter: Send + Sync {
    /// Format a ts into a display string
    ///
    /// # Arguments
    /// * `time` - The ts to format
    /// * `mark_type` - The type of tick mark (Year, Month, etc.)
    ///
    /// # Returns
    /// Formatted string to display on the time axis
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String;

    /// Formats one axis tick with the active chart locale and display-time fields.
    ///
    /// Return `None` to use the chart's normal locale and clock settings for this tick.
    /// `Some(String::new())` keeps an empty label. The default calls [`format`](Self::format),
    /// preserving existing formatters. Builders and the chart call this after time-zone
    /// conversion; hidden intraday clocks still use date labels instead.
    /// This hook does not change [`format_crosshair`](Self::format_crosshair).
    fn format_tick(&self, context: TickMarkContext<'_>) -> Option<String> {
        Some(self.format(context.time, context.mark_type))
    }

    /// Clock-face time used to identify calendar boundaries. The returned UTC value
    /// holds local fields, not another instant.
    fn wall_time(&self, time: DateTime<Utc>) -> DateTime<Utc> {
        time
    }

    /// Converts local calendar fields from `wall_time` back to an instant.
    /// Repeated times use the first instant; skipped times use the clock jump.
    fn utc_time(&self, wall_time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        Some(wall_time)
    }

    /// Whether intraday marks and hover labels include the clock.
    fn time_visible(&self) -> bool {
        true
    }

    /// Formats the hovered bar, retaining this formatter's date and clock conventions.
    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        let mut label = self.format(time, TickMarkType::DayOfMonth);
        let year = self.format(time, TickMarkType::Year);
        if year != self.format(Utc::now(), TickMarkType::Year) {
            label = format!("{label}, {year}");
        }
        match bar_seconds {
            86_400.. => label,
            60.. => format!("{label} {}", self.format(time, TickMarkType::Time)),
            1.. => format!(
                "{label} {}",
                self.format(time, TickMarkType::TimeWithSeconds)
            ),
            _ => format!(
                "{label} {}",
                self.format(time, TickMarkType::TimeWithMilliseconds)
            ),
        }
    }

    /// Clone this formatter into a Box
    fn clone_box(&self) -> Box<dyn TimeFormatter>;
}

/// Default time formatter using standard formats
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DefaultTimeFormatter {
    /// Whether to use 24-hour format
    pub use_24_hour: bool,

    /// Whether to show seconds
    pub show_seconds: bool,

    /// `strftime` pattern for the date in crosshair and tooltip labels, such as
    /// `"%d.%m.%Y"`, followed by the clock for intraday bars. `None` uses the
    /// built-in dates; an invalid or empty pattern falls back to them.
    pub date_format: Option<String>,

    /// Whether crosshair and tooltip dates name the weekday.
    pub weekday_names: WeekdayNames,
}

impl Default for DefaultTimeFormatter {
    fn default() -> Self {
        Self {
            use_24_hour: true,
            show_seconds: true,
            date_format: None,
            weekday_names: WeekdayNames::Default,
        }
    }
}

impl DefaultTimeFormatter {
    fn style(&self) -> Style<'_> {
        Style {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: self.date_format.as_deref(),
            weekday_names: self.weekday_names,
            #[cfg(feature = "locales")]
            locale: None,
        }
    }

    fn into_box(self) -> Box<dyn TimeFormatter> {
        if self.date_format.is_none() {
            Box::new(ClockTimeFormatter {
                use_24_hour: self.use_24_hour,
                show_seconds: self.show_seconds,
                weekday_names: self.weekday_names,
            })
        } else {
            Box::new(self)
        }
    }
}

impl TimeFormatter for DefaultTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        self.style().format(time, mark_type)
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        self.style().format_crosshair(time, bar_seconds)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        self.clone().into_box()
    }
}

/// Default clock options without storage for an unused custom date pattern.
#[derive(Clone)]
struct ClockTimeFormatter {
    use_24_hour: bool,
    show_seconds: bool,
    weekday_names: WeekdayNames,
}

impl TimeFormatter for ClockTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        DefaultTimeFormatter {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: None,
            weekday_names: self.weekday_names,
        }
        .format(time, mark_type)
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        DefaultTimeFormatter {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: None,
            weekday_names: self.weekday_names,
        }
        .format_crosshair(time, bar_seconds)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Label layout shared by the default and locale formatters.
struct Style<'a> {
    use_24_hour: bool,
    show_seconds: bool,
    date_format: Option<&'a str>,
    weekday_names: WeekdayNames,
    /// Language of the names; `None` writes English ones.
    #[cfg(feature = "locales")]
    locale: Option<chrono::Locale>,
}

impl Style<'_> {
    /// Writes `time` with a `strftime` pattern, or `None` when the pattern is invalid.
    fn write(&self, time: DateTime<Utc>, pattern: &str) -> Option<String> {
        use std::fmt::Write;
        let mut text = String::new();
        #[cfg(feature = "locales")]
        if let Some(locale) = self.locale {
            write!(text, "{}", time.format_localized(pattern, locale)).ok()?;
            // Some locales pad their names, as in Japanese " 5月".
            return Some(text.split_whitespace().collect::<Vec<_>>().join(" "));
        }
        write!(text, "{}", time.format(pattern)).ok()?;
        Some(text)
    }

    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        let pattern = match (mark_type, self.use_24_hour) {
            (TickMarkType::Year, _) => "%Y",
            (TickMarkType::Month, _) => "%b",
            (TickMarkType::DayOfMonth, _) => "%d",
            (TickMarkType::TimeWithMilliseconds, true) if self.show_seconds => "%H:%M:%S%.3f",
            (TickMarkType::TimeWithMilliseconds, false) if self.show_seconds => "%I:%M:%S%.3f %p",
            (TickMarkType::TimeWithSeconds, true) if self.show_seconds => "%H:%M:%S",
            (TickMarkType::TimeWithSeconds, false) if self.show_seconds => "%I:%M:%S %p",
            // Without seconds, second marks show the minute clock.
            (_, true) => "%H:%M",
            (_, false) => "%I:%M %p",
        };
        // Some locales have no AM/PM marker; keep a 12-hour clock unambiguous.
        #[cfg(feature = "locales")]
        if !self.use_24_hour
            && let Some(locale) = self.locale
            && time.format_localized("%p", locale).to_string().is_empty()
        {
            let pattern = pattern.replace("%p", &time.format("%p").to_string());
            return self.write(time, &pattern).unwrap_or_default();
        }
        self.write(time, pattern).unwrap_or_default()
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        let daily = bar_seconds >= 86_400;
        let weekday = |default| match self.weekday_names {
            WeekdayNames::Default => default,
            WeekdayNames::Always => true,
            WeekdayNames::Never => false,
        };
        let date = self
            .date_format
            .filter(|pattern| !pattern.is_empty())
            .and_then(|pattern| {
                if weekday(false) && !pattern.contains("%a") && !pattern.contains("%A") {
                    self.write(time, &format!("%a {pattern}"))
                } else {
                    self.write(time, pattern)
                }
            });
        let custom_date = date.is_some();
        let pattern = match (daily, weekday(!daily)) {
            (true, false) => "%b %d",
            (true, true) => "%a %b %d",
            (false, true) => "%a %d %b '%y",
            (false, false) => "%d %b '%y",
        };
        #[cfg(feature = "locales")]
        let date = date.or_else(|| self.locale.and_then(|_| self.write(time, pattern)));
        // The built-in English date writes straight into the label without a temporary String.
        let default_date = time.format(pattern);
        let date: &dyn std::fmt::Display = match &date {
            Some(date) => date,
            None => &default_date,
        };
        if daily {
            return if custom_date
                || time.format("%Y").to_string() == Utc::now().format("%Y").to_string()
            {
                date.to_string()
            } else {
                format!("{date}, {}", time.format("%Y"))
            };
        }
        let kind = if bar_seconds >= 60 {
            TickMarkType::Time
        } else if bar_seconds >= 1 {
            TickMarkType::TimeWithSeconds
        } else {
            TickMarkType::TimeWithMilliseconds
        };
        let clock = self.format(time, kind);
        format!("{date} {clock}")
    }
}

/// Locale-aware time formatter: month and weekday names and the AM/PM marker
/// in the locale's language, laid out as [`DefaultTimeFormatter`] does.
///
/// The names need the `locales` cargo feature, on by default; without it they
/// are English, as with an unknown locale.
/// A 12-hour clock in a locale without an AM/PM marker, such as German, uses the English one.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LocaleTimeFormatter {
    /// Locale identifier: a language code, alone or with a region, such as `"de"`, `"ja-JP"` or
    /// `"de-AT"`, in any letter case and with `-` or `_` between the parts. A language alone, or
    /// with a region that has no names of its own, uses its most common region's names: `"de"`
    /// as `"de-DE"`, `"en"` as `"en-US"`, `"pt"` as `"pt-BR"`. Other forms, such as the script
    /// subtag in `"zh-Hant-TW"`, and languages without names use English names.
    pub locale: String,

    /// Whether to use 24-hour format
    pub use_24_hour: bool,

    /// Whether to show seconds
    pub show_seconds: bool,

    /// `strftime` pattern for the date in crosshair and tooltip labels; see
    /// [`DefaultTimeFormatter::date_format`].
    pub date_format: Option<String>,

    /// Whether crosshair and tooltip dates name the weekday.
    pub weekday_names: WeekdayNames,
}

impl LocaleTimeFormatter {
    /// A formatter for `locale`, in the forms [`locale`](Self::locale) accepts. The clock
    /// defaults to 12 hours for `"en"`, `"en-US"` and `"en-CA"` in any letter case, and to
    /// 24 hours otherwise.
    pub fn new(locale: impl Into<String>) -> Self {
        let locale_str = locale.into();
        let use_24_hour = Self::default_24hour_for_locale(&locale_str);
        Self {
            locale: locale_str,
            use_24_hour,
            show_seconds: true,
            date_format: None,
            weekday_names: WeekdayNames::Default,
        }
    }

    /// 12 hours for `en` (as `en-US`), `en-US` and `en-CA` in any case, 24 hours otherwise.
    fn default_24hour_for_locale(locale: &str) -> bool {
        !matches!(
            locale.replace('_', "-").to_ascii_lowercase().as_str(),
            "en" | "en-us" | "en-ca"
        )
    }

    fn style(&self) -> Style<'_> {
        Style {
            use_24_hour: self.use_24_hour,
            show_seconds: self.show_seconds,
            date_format: self.date_format.as_deref(),
            weekday_names: self.weekday_names,
            #[cfg(feature = "locales")]
            locale: Some(chrono_locale(&self.locale).unwrap_or(chrono::Locale::POSIX)),
        }
    }
}

impl TimeFormatter for LocaleTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        self.style().format(time, mark_type)
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        self.style().format_crosshair(time, bar_seconds)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// The locale of each language chrono names, at the language's most likely region in CLDR
/// (`ber`, which CLDR lacks, at its first region), for identifiers without a usable region.
// shortcut: lists the languages of chrono 0.4.45; a chrono update that adds one needs a row.
#[cfg(feature = "locales")]
const PRIMARY_LOCALES: &[&str] = &[
    "aa_ET", "af_ZA", "agr_PE", "ak_GH", "am_ET", "an_ES", "anp_IN", "ar_EG", "as_IN", "ast_ES",
    "ayc_PE", "az_AZ", "be_BY", "bem_ZM", "ber_DZ", "bg_BG", "bhb_IN", "bho_IN", "bi_VU", "bn_BD",
    "bo_CN", "br_FR", "brx_IN", "bs_BA", "byn_ER", "ca_ES", "ce_RU", "chr_US", "cmn_TW", "crh_UA",
    "cs_CZ", "csb_PL", "cv_RU", "cy_GB", "da_DK", "de_DE", "doi_IN", "dsb_DE", "dv_MV", "dz_BT",
    "el_GR", "en_US", "eo", "es_ES", "et_EE", "eu_ES", "fa_IR", "ff_SN", "fi_FI", "fil_PH",
    "fo_FO", "fr_FR", "fur_IT", "fy_NL", "ga_IE", "gd_GB", "gez_ET", "gl_ES", "gu_IN", "gv_GB",
    "ha_NG", "hak_TW", "he_IL", "hi_IN", "hif_FJ", "hne_IN", "hr_HR", "hsb_DE", "ht_HT", "hu_HU",
    "hy_AM", "ia_FR", "id_ID", "ig_NG", "ik_CA", "is_IS", "it_IT", "iu_CA", "ja_JP", "ka_GE",
    "kab_DZ", "kk_KZ", "kl_GL", "km_KH", "kn_IN", "ko_KR", "kok_IN", "ks_IN", "ku_TR", "kw_GB",
    "ky_KG", "lb_LU", "lg_UG", "li_NL", "lij_IT", "ln_CD", "lo_LA", "lt_LT", "lv_LV", "lzh_TW",
    "mag_IN", "mai_IN", "mfe_MU", "mg_MG", "mhr_RU", "mi_NZ", "miq_NI", "mjw_IN", "mk_MK", "ml_IN",
    "mn_MN", "mni_IN", "mnw_MM", "mr_IN", "ms_MY", "mt_MT", "my_MM", "nan_TW", "nb_NO", "nds_DE",
    "ne_NP", "nhn_MX", "niu_NU", "nl_NL", "nn_NO", "nr_ZA", "nso_ZA", "oc_FR", "om_ET", "or_IN",
    "os_RU", "pa_IN", "pap_CW", "pl_PL", "ps_AF", "pt_BR", "quz_PE", "raj_IN", "ro_RO", "ru_RU",
    "rw_RW", "sa_IN", "sah_RU", "sat_IN", "sc_IT", "sd_IN", "se_NO", "sgs_LT", "shn_MM", "shs_CA",
    "si_LK", "sid_ET", "sk_SK", "sl_SI", "sm_WS", "so_SO", "sq_AL", "sr_RS", "ss_ZA", "st_ZA",
    "sv_SE", "sw_TZ", "szl_PL", "ta_IN", "tcy_IN", "te_IN", "tg_TJ", "th_TH", "the_NP", "ti_ET",
    "tig_ER", "tk_TM", "tl_PH", "tn_ZA", "to_TO", "tpi_PG", "tr_TR", "ts_ZA", "tt_RU", "ug_CN",
    "uk_UA", "unm_US", "ur_PK", "uz_UZ", "ve_ZA", "vi_VN", "wa_BE", "wae_CH", "wal_ET", "wo_SN",
    "xh_ZA", "yi_US", "yo_NG", "yue_HK", "yuw_PG", "zh_CN", "zu_ZA",
];

/// Resolves `locale` to chrono's names: an identifier chrono knows as given, then the language and
/// region in any case, then the language alone through [`PRIMARY_LOCALES`]. Script subtags and
/// modifiers never fall back to another script, so they give `None` (English) when not found.
#[cfg(feature = "locales")]
fn chrono_locale(locale: &str) -> Option<chrono::Locale> {
    let locale = locale.replace('-', "_");
    if let Ok(found) = chrono::Locale::try_from(locale.as_str()) {
        return Some(found);
    }
    let (tag, modifier) = locale.split_at(locale.find('@').unwrap_or(locale.len()));
    let mut parts = tag.split('_');
    let language = parts.next()?.to_ascii_lowercase();
    let region = parts.next();
    if parts.next().is_some()
        || region.is_some_and(|part| {
            part.len() == 4 && part.bytes().all(|byte| byte.is_ascii_alphabetic())
        })
    {
        return None;
    }
    if let Some(region) = region {
        let id = format!("{language}_{}{modifier}", region.to_ascii_uppercase());
        if let Ok(found) = chrono::Locale::try_from(id.as_str()) {
            return Some(found);
        }
    }
    if !modifier.is_empty() {
        return None;
    }
    PRIMARY_LOCALES
        .iter()
        .find(|id| id.split('_').next() == Some(language.as_str()))
        .and_then(|id| chrono::Locale::try_from(*id).ok())
}

/// Custom time formatter using a closure
#[derive(Clone)]
pub struct CustomTimeFormatter {
    formatter: Arc<dyn Fn(DateTime<Utc>, TickMarkType) -> String + Send + Sync>,
}

impl std::fmt::Debug for CustomTimeFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomTimeFormatter")
            .finish_non_exhaustive()
    }
}

impl CustomTimeFormatter {
    /// Create a new custom formatter from a closure
    pub fn new<F>(formatter: F) -> Self
    where
        F: Fn(DateTime<Utc>, TickMarkType) -> String + Send + Sync + 'static,
    {
        Self {
            formatter: Arc::new(formatter),
        }
    }
}

impl TimeFormatter for CustomTimeFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        (self.formatter)(time, mark_type)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Keeps per-tick fallback separate from the host's crosshair formatting.
struct TickContextFormatter {
    formatter: Box<dyn TimeFormatter>,
    fallback: Box<dyn TimeFormatter>,
    locale: Option<String>,
}

impl TimeFormatter for TickContextFormatter {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        self.formatter
            .format_tick(TickMarkContext {
                time,
                mark_type,
                locale: self.locale.as_deref(),
            })
            .unwrap_or_else(|| self.fallback.format(time, mark_type))
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        self.formatter.format_crosshair(time, bar_seconds)
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(Self {
            formatter: self.formatter.clone_box(),
            fallback: self.fallback.clone_box(),
            locale: self.locale.clone(),
        })
    }
}

/// Relative time formatter (e.g., "2h ago", "yesterday")
#[derive(Debug, Clone)]
pub struct RelativeTimeFormatter {
    /// Reference time for relative calculations (usually "now")
    pub reference_time: DateTime<Utc>,
}

impl RelativeTimeFormatter {
    /// Create a new relative time formatter
    pub fn new(reference_time: DateTime<Utc>) -> Self {
        Self { reference_time }
    }

    /// Create a formatter relative to current time
    pub fn now() -> Self {
        Self {
            reference_time: Utc::now(),
        }
    }
}

impl TimeFormatter for RelativeTimeFormatter {
    fn format(&self, time: DateTime<Utc>, _mark_type: TickMarkType) -> String {
        let diff = self.reference_time.signed_duration_since(time);

        if diff.num_seconds() < 0 {
            return "future".to_string();
        }

        let seconds = diff.num_seconds();

        if seconds < 60 {
            format!("{seconds}s ago")
        } else if seconds < 3600 {
            format!("{}m ago", seconds / 60)
        } else if seconds < 86400 {
            format!("{}h ago", seconds / 3600)
        } else if seconds < 604800 {
            format!("{}d ago", seconds / 86400)
        } else if seconds < 2592000 {
            format!("{}w ago", seconds / 604800)
        } else if seconds < 31536000 {
            format!("{}mo ago", seconds / 2592000)
        } else {
            format!("{}y ago", seconds / 31536000)
        }
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Timezone display mode for time axis labels.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TimezoneMode {
    /// Display in UTC
    #[default]
    Utc,
    /// Display in user's local timezone
    Local,
    /// Display in specific timezone
    Timezone(TimezoneId),
    /// Display in exchange/venue timezone (user must specify via Timezone)
    Exchange(TimezoneId),
}

impl TimezoneMode {
    /// The wall time this zone shows at `time`; `Local` reads the system zone.
    #[cfg(feature = "widget")]
    pub(crate) fn wall_time(&self, time: DateTime<Utc>) -> chrono::NaiveDateTime {
        match self {
            Self::Local => crate::model::local_time(chrono::Local, time),
            _ => crate::model::local_time(self.to_tz().unwrap_or_default(), time),
        }
    }

    /// The first instant this zone shows `wall`; a skipped wall time opens at the clock jump.
    #[cfg(feature = "ui")]
    pub(crate) fn instant(&self, wall: chrono::NaiveDateTime) -> Option<DateTime<Utc>> {
        match self {
            Self::Local => crate::model::local_instant(chrono::Local, wall),
            _ => crate::model::local_instant(self.to_tz().unwrap_or_default(), wall),
        }
    }

    /// New York exchange time, with daylight-saving rules (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn nyse() -> Self {
        TimezoneMode::Exchange("America/New_York".parse().unwrap())
    }

    /// London exchange time, with daylight-saving rules (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn lse() -> Self {
        TimezoneMode::Exchange("Europe/London".parse().unwrap())
    }

    /// Tokyo exchange time (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn jse() -> Self {
        TimezoneMode::Exchange("Asia/Tokyo".parse().unwrap())
    }

    /// Hong Kong exchange time (`timezones` feature).
    #[cfg(feature = "timezones")]
    pub fn hkex() -> Self {
        TimezoneMode::Exchange("Asia/Hong_Kong".parse().unwrap())
    }

    /// Get the selected zone for conversion; UTC and system local time return `None`.
    pub fn to_tz(&self) -> Option<TimezoneId> {
        match self {
            TimezoneMode::Utc => None,   // Already UTC
            TimezoneMode::Local => None, // Will use system local
            TimezoneMode::Timezone(tz) | TimezoneMode::Exchange(tz) => Some(tz.clone()),
        }
    }
}

/// Whether crosshair, tooltip and forecast dates name the weekday. Time axis marks that show
/// a date in place of a hidden clock follow this choice too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum WeekdayNames {
    /// Follow the date format: built-in dates name the weekday on intraday bars, as in
    /// `Wed 13 Mar '24 14:30`, and omit it on daily bars, as in `Mar 13, 2024`.
    /// Custom patterns name it where they contain `%a` or `%A`.
    #[default]
    Default,
    /// Name it on every date: `Wed Mar 13, 2024` on daily bars. A custom pattern without
    /// `%a` or `%A` gets the short name before it, as in `Wed 13.03.2024`.
    Always,
    /// Omit it from built-in dates, as in `13 Mar '24 14:30`. Custom patterns are written
    /// as given, including a weekday they explicitly name.
    Never,
}

/// Timezone-aware formatter wrapper
/// Converts ts to target timezone before formatting
pub(crate) struct TimezoneAwareFormatter<T> {
    inner: Box<dyn TimeFormatter>,
    timezone: T,
    time_visible: bool,
}

impl<T: Clone> Clone for TimezoneAwareFormatter<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone_box(),
            timezone: self.timezone.clone(),
            time_visible: self.time_visible,
        }
    }
}

impl<T: chrono::TimeZone> TimezoneAwareFormatter<T> {
    /// Create a new timezone-aware formatter
    pub(crate) fn new(inner: Box<dyn TimeFormatter>, timezone: T, time_visible: bool) -> Self {
        Self {
            inner,
            timezone,
            time_visible,
        }
    }

    /// Convert UTC ts to target timezone
    fn convert_timezone(&self, utc_time: DateTime<Utc>) -> DateTime<Utc> {
        use crate::model::local_time;
        local_time(self.timezone.clone(), utc_time).and_utc()
    }
}

impl<T: chrono::TimeZone + Send + Sync + 'static> TimeFormatter for TimezoneAwareFormatter<T> {
    fn format(&self, time: DateTime<Utc>, mark_type: TickMarkType) -> String {
        let converted = self.convert_timezone(time);
        if !self.time_visible
            && matches!(
                mark_type,
                TickMarkType::Time
                    | TickMarkType::TimeWithSeconds
                    | TickMarkType::TimeWithMilliseconds
            )
        {
            self.inner.format_crosshair(converted, 86_400)
        } else {
            self.inner.format(converted, mark_type)
        }
    }

    fn time_visible(&self) -> bool {
        self.time_visible
    }

    fn wall_time(&self, time: DateTime<Utc>) -> DateTime<Utc> {
        self.convert_timezone(time)
    }

    fn utc_time(&self, time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        use crate::model::local_instant;
        local_instant(self.timezone.clone(), time.naive_utc())
    }

    fn format_crosshair(&self, time: DateTime<Utc>, bar_seconds: i64) -> String {
        self.inner.format_crosshair(
            self.convert_timezone(time),
            if self.time_visible {
                bar_seconds
            } else {
                86_400
            },
        )
    }

    fn clone_box(&self) -> Box<dyn TimeFormatter> {
        Box::new(self.clone())
    }
}

/// Builder for creating time formatters
pub struct TimeFormatterBuilder {
    use_24_hour: bool,
    show_seconds: bool,
    locale: Option<String>,
    date_format: Option<String>,
    weekday_names: WeekdayNames,
    formatter: Option<Box<dyn TimeFormatter>>,
    timezone: Option<TimezoneMode>,
    time_visible: bool,
}

impl std::fmt::Debug for TimeFormatterBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimeFormatterBuilder")
            .field("use_24_hour", &self.use_24_hour)
            .field("show_seconds", &self.show_seconds)
            .field("locale", &self.locale)
            .field("date_format", &self.date_format)
            .field("weekday_names", &self.weekday_names)
            .field("timezone", &self.timezone)
            .field("time_visible", &self.time_visible)
            .finish_non_exhaustive()
    }
}

impl TimeFormatterBuilder {
    /// Create a new formatter builder
    pub fn new() -> Self {
        Self {
            use_24_hour: true,
            show_seconds: true,
            locale: None,
            date_format: None,
            weekday_names: WeekdayNames::Default,
            formatter: None,
            timezone: None,
            time_visible: true,
        }
    }

    /// Set 24-hour format
    pub fn with_24_hour(mut self, use_24_hour: bool) -> Self {
        self.use_24_hour = use_24_hour;
        self
    }

    /// Set seconds display
    pub fn with_seconds(mut self, show_seconds: bool) -> Self {
        self.show_seconds = show_seconds;
        self
    }

    /// Show the clock in intraday marks and hover labels, or dates alone.
    pub fn with_time_visible(mut self, time_visible: bool) -> Self {
        self.time_visible = time_visible;
        self
    }

    /// Set locale
    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    /// Set the `strftime` pattern for crosshair and tooltip dates; see
    /// [`DefaultTimeFormatter::date_format`].
    pub fn with_date_format(mut self, date_format: impl Into<String>) -> Self {
        self.date_format = Some(date_format.into());
        self
    }

    /// Set whether crosshair and tooltip dates name the weekday.
    pub fn with_weekday_names(mut self, weekday_names: WeekdayNames) -> Self {
        self.weekday_names = weekday_names;
        self
    }

    /// Format with `formatter`, with per-tick access to the locale and default text.
    ///
    /// The time zone and clock visibility still apply: `formatter` receives
    /// wall-clock times in the chosen zone, and dates in place of intraday
    /// clocks when the time is hidden. Its own options replace the 24-hour,
    /// seconds, weekday names, locale and date format settings, unless
    /// [`format_tick`](TimeFormatter::format_tick) returns `None` for a tick.
    pub fn with_formatter(mut self, formatter: Box<dyn TimeFormatter>) -> Self {
        self.formatter = Some(formatter);
        self
    }

    /// Set timezone for conversion
    pub fn with_timezone(mut self, timezone: TimezoneMode) -> Self {
        self.timezone = Some(timezone);
        self
    }

    /// Build the formatter
    pub fn build(self) -> Box<dyn TimeFormatter> {
        let base: Box<dyn TimeFormatter> = if let Some(locale) = &self.locale {
            Box::new(LocaleTimeFormatter {
                locale: locale.clone(),
                use_24_hour: self.use_24_hour,
                show_seconds: self.show_seconds,
                date_format: self.date_format,
                weekday_names: self.weekday_names,
            })
        } else {
            DefaultTimeFormatter {
                use_24_hour: self.use_24_hour,
                show_seconds: self.show_seconds,
                date_format: self.date_format,
                weekday_names: self.weekday_names,
            }
            .into_box()
        };
        let base: Box<dyn TimeFormatter> = match self.formatter {
            Some(formatter) => Box::new(TickContextFormatter {
                formatter,
                fallback: base,
                locale: self.locale,
            }),
            None => base,
        };

        // UTC and system-local formatters need no storage for a selected zone.
        match self.timezone.unwrap_or_default() {
            TimezoneMode::Utc => {
                Box::new(TimezoneAwareFormatter::new(base, Utc, self.time_visible))
            }
            TimezoneMode::Local => Box::new(TimezoneAwareFormatter::new(
                base,
                chrono::Local,
                self.time_visible,
            )),
            TimezoneMode::Timezone(zone) | TimezoneMode::Exchange(zone) => {
                Box::new(TimezoneAwareFormatter::new(base, zone, self.time_visible))
            }
        }
    }
}

impl Default for TimeFormatterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_default_formatter() {
        let formatter = DefaultTimeFormatter::default().clone_box();
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap();

        assert_eq!(formatter.format(time, TickMarkType::Year), "2024");
        assert_eq!(formatter.format(time, TickMarkType::Month), "Jun");
        assert_eq!(formatter.format(time, TickMarkType::Time), "14:30");
        assert_eq!(
            formatter.format(time, TickMarkType::TimeWithSeconds),
            "14:30:45"
        );

        let twelve_hour = TimeFormatterBuilder::new()
            .with_24_hour(false)
            .with_seconds(false)
            .build();
        assert_eq!(twelve_hour.format(time, TickMarkType::Time), "02:30 PM");
    }

    #[test]
    fn fractional_crosshair_seconds_precede_the_meridiem() {
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 45).unwrap()
            + chrono::Duration::milliseconds(123);
        for (use_24_hour, seconds, fractional) in [
            (true, "14:30:45", "14:30:45.123"),
            (false, "02:30:45 PM", "02:30:45.123 PM"),
        ] {
            let formatter = DefaultTimeFormatter {
                use_24_hour,
                ..Default::default()
            };
            let custom = CustomTimeFormatter::new(move |time, kind| {
                DefaultTimeFormatter {
                    use_24_hour,
                    ..Default::default()
                }
                .format(time, kind)
            });
            // The style and the trait default share the clock's precision and hour cycle.
            for formatter in [&formatter as &dyn TimeFormatter, &custom] {
                for (bar_seconds, clock) in [(0, fractional), (1, seconds)] {
                    let label = formatter.format_crosshair(time, bar_seconds);
                    assert!(label.ends_with(clock), "{label}");
                }
            }
        }
        let hidden_seconds = DefaultTimeFormatter {
            use_24_hour: false,
            show_seconds: false,
            ..Default::default()
        };
        assert!(
            hidden_seconds
                .format_crosshair(time, 0)
                .ends_with("02:30 PM")
        );
    }

    #[test]
    fn locale_and_date_format_shape_labels() {
        // Wednesday 13 March 2024, 14:30:45 UTC.
        let time = Utc.with_ymd_and_hms(2024, 3, 13, 14, 30, 45).unwrap();
        // Without the `locales` feature every locale writes English names.
        let localized = |name, english| {
            if cfg!(feature = "locales") {
                name
            } else {
                english
            }
        };
        for (locale, expected) in [
            ("en-US", "02:30 PM"),
            ("en_US", "02:30 PM"),
            ("en-CA", "02:30 PM"),
            ("en_CA", "02:30 PM"),
            ("en-GB", "14:30"),
            ("en_GB", "14:30"),
            ("de-DE", "14:30"),
            ("de_DE", "14:30"),
            ("en", "02:30 PM"),
            ("EN-us", "02:30 PM"),
        ] {
            assert_eq!(
                LocaleTimeFormatter::new(locale).format(time, TickMarkType::Time),
                expected,
                "{locale}"
            );
        }
        // A language alone takes its most common region; language and region match in any case.
        let january = Utc.with_ymd_and_hms(2024, 1, 13, 14, 30, 45).unwrap();
        for (locale, time, expected) in [
            ("de", time, localized("Mär", "Mar")),
            ("DE", time, localized("Mär", "Mar")),
            ("fr", time, localized("mars", "Mar")),
            ("ja", time, localized("3月", "Mar")),
            ("es", time, localized("mar", "Mar")),
            ("de-AT", january, localized("Jän", "Jan")),
            ("DE-at", january, localized("Jän", "Jan")),
            ("de-XX", time, localized("Mär", "Mar")),
            ("sr-rs@latin", time, localized("mar", "Mar")),
            // A script never falls back to the language's default script, Cyrillic "мар".
            ("sr@latin", time, "Mar"),
            ("sr-Latn", time, "Mar"),
            ("sr-Latn-RS", time, "Mar"),
        ] {
            assert_eq!(
                LocaleTimeFormatter::new(locale).format(time, TickMarkType::Month),
                expected,
                "{locale}"
            );
        }
        #[cfg(feature = "locales")]
        for id in PRIMARY_LOCALES {
            assert!(chrono::Locale::try_from(*id).is_ok(), "{id}");
        }
        let german = LocaleTimeFormatter::new("de-DE");
        assert_eq!(
            german.format(time, TickMarkType::Month),
            localized("Mär", "Mar")
        );
        assert_eq!(
            german.format_crosshair(time, 60),
            localized("Mi 13 Mär '24 14:30", "Wed 13 Mar '24 14:30")
        );
        let japanese = LocaleTimeFormatter::new("ja_JP");
        assert_eq!(
            japanese.format(time, TickMarkType::Month),
            localized("3月", "Mar")
        );
        // Empty locale markers need a fallback, while existing translations stay intact.
        for (locale, expected) in [
            ("de-DE", "02:30 PM"),
            ("ja-JP", localized("02:30 午後", "02:30 PM")),
            ("en-GB", localized("02:30 pm", "02:30 PM")),
        ] {
            let twelve_hour = LocaleTimeFormatter {
                use_24_hour: false,
                ..LocaleTimeFormatter::new(locale)
            };
            assert_eq!(
                twelve_hour.format(time, TickMarkType::Time),
                expected,
                "{locale}"
            );
        }
        assert_eq!(
            LocaleTimeFormatter::new("xx-YY").format(time, TickMarkType::Month),
            "Mar"
        );
        let dated = |date_format: &str| {
            DefaultTimeFormatter {
                date_format: Some(date_format.into()),
                ..Default::default()
            }
            .clone_box()
        };
        assert_eq!(
            dated("%d.%m.%Y").format_crosshair(time, 60),
            "13.03.2024 14:30"
        );
        assert_eq!(
            dated("%d.%m.%Y").format_crosshair(time, 86_400),
            "13.03.2024"
        );
        // An invalid pattern keeps the built-in date rather than failing to format.
        assert_eq!(
            dated("%Q").format_crosshair(time, 60),
            "Wed 13 Mar '24 14:30"
        );
        assert_eq!(dated("").format_crosshair(time, 60), "Wed 13 Mar '24 14:30");
        assert_eq!(dated("").format_crosshair(time, 86_400), "Mar 13, 2024");
        let named = DefaultTimeFormatter {
            date_format: Some("%A %d.%m.%Y".into()),
            weekday_names: WeekdayNames::Always,
            ..Default::default()
        };
        assert_eq!(named.format_crosshair(time, 86_400), "Wednesday 13.03.2024");
        let current = time.with_year(Utc::now().year()).unwrap();
        let previous = current.with_year(current.year() - 1).unwrap();
        let default = DefaultTimeFormatter::default().clone_box();
        assert_eq!(default.format_crosshair(current, 86_400), "Mar 13");
        assert_eq!(
            default.format_crosshair(previous, 86_400),
            format!("Mar 13, {}", previous.year())
        );
    }

    #[test]
    fn test_relative_formatter() {
        let reference = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let formatter = RelativeTimeFormatter::new(reference);

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 28, 0).unwrap();
        assert_eq!(formatter.format(time, TickMarkType::Time), "2m ago");

        let time = Utc.with_ymd_and_hms(2024, 6, 15, 12, 30, 0).unwrap();
        assert_eq!(formatter.format(time, TickMarkType::Time), "2h ago");
    }

    #[test]
    fn builder_overrides_host_calendar_and_clock_visibility() {
        #[derive(Clone)]
        struct Host;
        impl TimeFormatter for Host {
            fn format(&self, time: DateTime<Utc>, _: TickMarkType) -> String {
                time.to_rfc3339()
            }
            fn wall_time(&self, time: DateTime<Utc>) -> DateTime<Utc> {
                time + chrono::Duration::hours(1)
            }
            fn utc_time(&self, _: DateTime<Utc>) -> Option<DateTime<Utc>> {
                None
            }
            fn time_visible(&self) -> bool {
                false
            }
            fn clone_box(&self) -> Box<dyn TimeFormatter> {
                Box::new(self.clone())
            }
        }
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let formatter = TimeFormatterBuilder::new()
            .with_formatter(Box::new(Host))
            .with_timezone(TimezoneMode::Utc)
            .build();
        for formatter in [formatter.clone_box(), formatter] {
            assert!(formatter.time_visible());
            assert_eq!(formatter.wall_time(time), time);
            assert_eq!(formatter.utc_time(time), Some(time));
            assert_eq!(
                formatter.format(time, TickMarkType::Time),
                time.to_rfc3339()
            );
        }
    }

    #[test]
    fn test_timezone_conversion_utc_and_fixed_offsets() {
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        for (name, label, wall) in [
            ("UTC", "14:30", "2024-06-15T14:30:00Z"),
            ("+05:45", "20:15", "2024-06-15T20:15:00Z"),
            ("-03:30", "11:00", "2024-06-15T11:00:00Z"),
            ("+14:00", "04:30", "2024-06-16T04:30:00Z"),
            ("-12:00", "02:30", "2024-06-15T02:30:00Z"),
            ("+00:19:32", "14:49", "2024-06-15T14:49:32Z"),
            ("+23:59:59", "14:29", "2024-06-16T14:29:59Z"),
            ("-23:59:59", "14:30", "2024-06-14T14:30:01Z"),
        ] {
            let mode = TimezoneMode::Timezone(name.parse().unwrap());
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!(r#"{{"Timezone":"{name}"}}"#));
            assert_eq!(serde_json::from_str::<TimezoneMode>(&json).unwrap(), mode);
            let formatter = TimeFormatterBuilder::new()
                .with_24_hour(true)
                .with_seconds(false)
                .with_timezone(mode)
                .build();
            assert_eq!(formatter.format(time, TickMarkType::Time), label, "{name}");
            assert_eq!(
                formatter.wall_time(time),
                wall.parse::<DateTime<Utc>>().unwrap(),
                "{name}"
            );
            assert_eq!(formatter.utc_time(formatter.wall_time(time)), Some(time));
        }
        for invalid in [
            "",
            "+24:00",
            "-24:00",
            "+01:60",
            "+01:00:60",
            "+01:00junk",
            "+１:00",
            "unknown",
        ] {
            assert!(invalid.parse::<TimezoneId>().is_err(), "{invalid}");
        }
        for seconds in [i32::MIN, -86_400, 86_400, i32::MAX] {
            assert!(TimezoneId::fixed_offset(seconds).is_none());
        }
        assert_eq!(
            TimezoneId::named("America/New_York").is_some(),
            cfg!(feature = "timezones")
        );
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn named_zones_keep_their_clock_through_dst_and_cloning() {
        use TickMarkType::{Time, TimeWithSeconds};
        // UTC offsets: New York -4 in summer, -5 in winter; London +1/0; Tokyo +9.
        for (zone, instant, kind, expected) in [
            (TimezoneMode::nyse(), "2024-06-15T14:30:00Z", Time, "10:30"),
            (
                TimezoneMode::Exchange("America/New_York".parse().unwrap()),
                "2024-03-10T06:00:00Z",
                TimeWithSeconds,
                "01:00:00",
            ),
            (
                TimezoneMode::Exchange("America/New_York".parse().unwrap()),
                "2024-03-10T08:00:00Z",
                TimeWithSeconds,
                "04:00:00",
            ),
            (
                TimezoneMode::Exchange("America/New_York".parse().unwrap()),
                "2024-11-03T05:00:00Z",
                TimeWithSeconds,
                "01:00:00",
            ),
            (
                TimezoneMode::Exchange("America/New_York".parse().unwrap()),
                "2024-11-03T07:00:00Z",
                TimeWithSeconds,
                "02:00:00",
            ),
            (TimezoneMode::lse(), "2024-06-15T14:30:00Z", Time, "15:30"),
            (TimezoneMode::lse(), "2024-01-15T14:30:00Z", Time, "14:30"),
            (TimezoneMode::jse(), "2024-06-15T14:30:00Z", Time, "23:30"),
        ] {
            let time = instant.parse::<DateTime<Utc>>().unwrap();
            let formatter = TimeFormatterBuilder::new()
                .with_24_hour(true)
                .with_timezone(zone.clone())
                .build();
            for formatter in [formatter.clone_box(), formatter] {
                assert_eq!(formatter.format(time, kind), expected, "{zone:?} {instant}");
            }
        }
        let time = Utc.with_ymd_and_hms(2024, 6, 15, 14, 30, 0).unwrap();
        let formatter = TimeFormatterBuilder::new()
            .with_timezone(TimezoneMode::nyse())
            .build();
        assert_eq!(
            formatter.format_crosshair(time, 3600),
            "Sat 15 Jun '24 10:30"
        );
        assert_eq!(
            formatter.format_crosshair(time + chrono::Duration::milliseconds(123), 0),
            "Sat 15 Jun '24 10:30:00.123"
        );
    }
}

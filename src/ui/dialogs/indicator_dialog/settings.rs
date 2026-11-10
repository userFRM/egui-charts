//! Settings for an existing study, staged until Apply.

use crate::localization as l10n;
use crate::model::LineStyle;
use crate::model::Timeframe;
use crate::studies::{Indicator, InputValue, PlotType, StudyInput, StudyScale, StudySettings};
use crate::theme::Theme;
use crate::ui_kit::{FormGrid, dialog::DialogFooter, modal::Modal};
use crate::widget::SeriesId;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Inputs,
    Style,
    Visibility,
}

/// A complete edit for the host to apply to the original study id.
pub struct StudyEdit {
    /// Stable id of the applied study.
    pub id: SeriesId,
    /// Calculation input ids and their new values.
    pub inputs: Vec<(String, InputValue)>,
    /// Per-plot styles and value tags, levels, precision and interval visibility.
    pub settings: StudySettings,
}

/// Inputs, Style and Visibility editor for an applied study. Closing or cancelling discards
/// staged values; no study or output history is cloned while the dialog is open.
#[derive(Default)]
pub struct IndicatorSettingsDialog {
    edit: Option<StudyEdit>,
    inputs: Vec<StudyInput>,
    title: String,
    line_names: Vec<String>,
    levels: Vec<(f64, &'static str)>,
    tab: Tab,
    current_interval: Option<Timeframe>,
    sources: Vec<(crate::studies::StudySource, String)>,
    overlay: bool,
    price_scale: bool,
}

impl crate::ui_kit::tab_bar::TabLabel for Tab {
    fn tab_label(&self) -> &str {
        match self {
            Self::Inputs => "Inputs",
            Self::Style => "Style",
            Self::Visibility => "Visibility",
        }
    }
}

/// A line pattern picker named for screen readers.
fn pattern_combo(ui: &mut egui::Ui, pattern: &mut LineStyle) {
    egui::ComboBox::from_id_salt("pattern")
        .selected_text(l10n::text(ui.ctx(), pattern.name()))
        .show_ui(ui, |ui| {
            for choice in LineStyle::all() {
                ui.selectable_value(pattern, *choice, l10n::text(ui.ctx(), choice.name()));
            }
        })
        .response
        .accessible_name(l10n::text(ui.ctx(), "Line pattern"));
}

fn width_picker(ui: &mut egui::Ui, width: &mut f32) {
    ui.add(
        egui::DragValue::new(width)
            .range(0.5..=10.0)
            .suffix(l10n::text(ui.ctx(), " px")),
    )
    .accessible_name(l10n::text(ui.ctx(), "Line width"));
}

impl IndicatorSettingsDialog {
    /// Open on the current values of the study identified by `id`.
    pub fn open(&mut self, id: SeriesId, study: &dyn Indicator, interval: Option<Timeframe>) {
        if !study.user_edit_enabled() {
            self.edit = None;
            return;
        }
        self.edit = Some(StudyEdit {
            id,
            inputs: Vec::new(),
            settings: study.settings(),
        });
        self.inputs = study.inputs();
        self.title = format!("{} settings", study.name());
        self.line_names = study.line_names();
        self.levels = study
            .pane()
            .levels
            .into_iter()
            .map(|(value, _, caption)| (value, caption))
            .collect();
        self.overlay = study.is_overlay();
        self.price_scale = study.scale() == StudyScale::Main;
        self.current_interval = interval;
        self.tab = Tab::Inputs;
        self.sources.clear();
    }

    /// Open on a chart study and offer its other studies as source inputs, excluding cycles.
    pub fn open_for_chart(&mut self, chart: &crate::widget::Chart, id: SeriesId) -> bool {
        let Some((_, study)) = chart.indicators().find(|(candidate, _)| *candidate == id) else {
            return false;
        };
        self.open(id, study, chart.timeframe());
        if !self.is_open() {
            return false;
        }
        let studies: Vec<_> = chart.indicators().collect();
        for (candidate, study) in &studies {
            let mut next = Some(candidate.0);
            for _ in 0..=studies.len() {
                let Some(current) = next else {
                    break;
                };
                if current == id.0 {
                    break;
                }
                next = studies
                    .iter()
                    .find(|(id, _)| id.0 == current)
                    .and_then(|(_, study)| match study.source() {
                        Some(crate::studies::StudySource::Study { id, .. }) => Some(id),
                        _ => None,
                    });
            }
            if next.is_some() {
                continue;
            }
            for (plot, name) in study.line_names().into_iter().enumerate() {
                self.sources.push((
                    crate::studies::StudySource::Study {
                        id: candidate.0,
                        plot,
                    },
                    format!("{}: {name}", study.name()),
                ));
            }
        }
        true
    }

    /// Whether an edit is open.
    pub fn is_open(&self) -> bool {
        self.edit.is_some()
    }

    /// Show an edit while its study still exists and permits editing. Revoking permission
    /// discards an already open edit. The host must recheck permission before applying it.
    pub fn show_for_chart(
        &mut self,
        ctx: &egui::Context,
        theme: &Theme,
        chart: &crate::widget::Chart,
    ) -> Option<StudyEdit> {
        if let Some(edit) = &self.edit
            && !chart.indicator_user_edit_enabled(edit.id)
        {
            self.edit = None;
        }
        self.show(ctx, theme)
    }

    /// Show the staged edit. Apply returns it once; the host calls
    /// [`Indicator::apply_settings`] on the original study, then lets the chart recalculate.
    pub fn show(&mut self, ctx: &egui::Context, _theme: &Theme) -> Option<StudyEdit> {
        let edit = self.edit.as_mut()?;
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Modal::new("indicator_settings", &self.title)
            .width(620.0)
            .height(560.0)
            .top(120.0)
            .show(ctx, &mut open, |ui| {
                crate::ui_kit::tab_bar::TabBar::new(
                    &[Tab::Inputs, Tab::Style, Tab::Visibility],
                    &mut self.tab,
                )
                .show(ui);
                ui.separator();
                egui::ScrollArea::both()
                    .max_height((ui.available_height() - 72.0).max(0.0))
                    .show(ui, |ui| match self.tab {
                        Tab::Inputs => {
                            if self.inputs.is_empty() {
                                ui.label(l10n::text(
                                    ui.ctx(),
                                    "This study has no editable inputs.",
                                ));
                            }
                            if !self.inputs.is_empty()
                                && ui
                                    .small_button(l10n::text(ui.ctx(), "Reset inputs"))
                                    .clicked()
                            {
                                for input in &mut self.inputs {
                                    input.value = input.default.clone();
                                }
                            }
                            let study_source = self.inputs.iter().any(|input| {
                                matches!(
                                    input.value,
                                    InputValue::Source(crate::studies::StudySource::Study { .. })
                                )
                            });
                            let shown: Vec<_> = self
                                .inputs
                                .iter()
                                .map(|input| input.is_shown(&self.inputs))
                                .collect();
                            FormGrid::new("study_inputs").show(ui, |ui| {
                                for (input, shown) in self.inputs.iter_mut().zip(shown) {
                                    if study_source
                                        && matches!(
                                            input.id.as_str(),
                                            "calculation_symbol"
                                                | "calculation_timeframe"
                                                | "wait_for_close"
                                        )
                                    {
                                        input.value = input.default.clone();
                                        continue;
                                    }
                                    if !shown {
                                        continue;
                                    }
                                    crate::ui::strategy_report::inputs::show_input(
                                        ui,
                                        input,
                                        &self.sources,
                                    );
                                }
                            });
                        }
                        Tab::Style => {
                            ui.checkbox(
                                &mut edit.settings.show_legend_values,
                                l10n::text(ui.ctx(), "Show values in status line"),
                            );
                            for (line, plot) in edit.settings.plots.iter_mut().enumerate() {
                                ui.push_id(line, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.checkbox(
                                            &mut plot.visible,
                                            l10n::text(
                                                ui.ctx(),
                                                self.line_names
                                                    .get(line)
                                                    .map_or("Plot", String::as_str),
                                            ),
                                        );
                                        ui.color_edit_button_srgba(&mut plot.color);
                                        width_picker(ui, &mut plot.width);
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        pattern_combo(ui, &mut plot.pattern);
                                        egui::ComboBox::from_id_salt("plot_type")
                                            .selected_text(l10n::text(
                                                ui.ctx(),
                                                plot.plot_type.name(),
                                            ))
                                            .show_ui(ui, |ui| {
                                                for kind in PlotType::ALL {
                                                    let label = l10n::text(ui.ctx(), kind.name());
                                                    // A saved choice stays listed and selected.
                                                    match kind
                                                        .incompatibility(
                                                            self.overlay,
                                                            self.price_scale,
                                                        )
                                                        .filter(|_| kind != plot.plot_type)
                                                    {
                                                        Some(reason) => {
                                                            ui.add_enabled(
                                                                false,
                                                                egui::Button::selectable(
                                                                    false, label,
                                                                ),
                                                            )
                                                            .on_disabled_hover_text(l10n::text(
                                                                ui.ctx(),
                                                                reason,
                                                            ));
                                                        }
                                                        None => {
                                                            ui.selectable_value(
                                                                &mut plot.plot_type,
                                                                kind,
                                                                label,
                                                            );
                                                        }
                                                    }
                                                }
                                            })
                                            .response
                                            .accessible_name(l10n::text(ui.ctx(), "Plot type"));
                                        let mut tagged =
                                            !edit.settings.untagged_plots.contains(&line);
                                        if ui
                                            .checkbox(
                                                &mut tagged,
                                                l10n::text(ui.ctx(), "Value on scale"),
                                            )
                                            .changed()
                                        {
                                            edit.settings
                                                .untagged_plots
                                                .retain(|&other| other != line);
                                            if !tagged {
                                                edit.settings.untagged_plots.push(line);
                                            }
                                        }
                                        let mut price_line =
                                            edit.settings.price_line_plots.contains(&line);
                                        if ui
                                            .checkbox(
                                                &mut price_line,
                                                l10n::text(ui.ctx(), "Latest value line"),
                                            )
                                            .changed()
                                        {
                                            edit.settings
                                                .price_line_plots
                                                .retain(|&other| other != line);
                                            if price_line {
                                                edit.settings.price_line_plots.push(line);
                                            }
                                        }
                                    });
                                    if let Some(variants) =
                                        edit.settings.plot_variants.get_mut(line)
                                    {
                                        for (index, variant) in variants.iter_mut().enumerate() {
                                            ui.push_id(("variant", index), |ui| {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label(l10n::text(ui.ctx(), &variant.name));
                                                    ui.color_edit_button_srgba(&mut variant.color);
                                                    width_picker(ui, &mut variant.width);
                                                });
                                            });
                                        }
                                    }
                                    ui.separator();
                                });
                            }
                            for (index, level) in edit.settings.levels.iter_mut().enumerate() {
                                let (drawn, caption) = self
                                    .levels
                                    .get(index)
                                    .copied()
                                    .unwrap_or((f64::NAN, "Level"));
                                ui.push_id(("level", index), |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.checkbox(
                                            &mut level.visible,
                                            l10n::text(ui.ctx(), caption),
                                        );
                                        let mut value = level.value.unwrap_or(drawn);
                                        if ui
                                            .add(egui::DragValue::new(&mut value).speed(0.1))
                                            .accessible_name(l10n::text(ui.ctx(), "Level value"))
                                            .changed()
                                        {
                                            level.value = Some(value);
                                        }
                                        ui.color_edit_button_srgba(&mut level.color)
                                            .accessible_name(l10n::text(ui.ctx(), "Level colour"));
                                        pattern_combo(ui, &mut level.pattern);
                                        width_picker(ui, &mut level.width);
                                        ui.checkbox(
                                            &mut level.price_line,
                                            l10n::text(ui.ctx(), "Latest value line"),
                                        );
                                    });
                                });
                            }
                            if edit.settings.levels.len() >= 2 {
                                ui.horizontal_wrapped(|ui| {
                                    ui.checkbox(
                                        &mut edit.settings.fill_between_levels,
                                        l10n::text(ui.ctx(), "Fill between levels"),
                                    );
                                    ui.color_edit_button_srgba(&mut edit.settings.level_fill_color)
                                        .accessible_name(l10n::text(ui.ctx(), "Fill colour"));
                                });
                            }
                            if !edit.settings.levels.is_empty() {
                                ui.separator();
                            }
                            ui.horizontal_wrapped(|ui| {
                                let mut enabled = edit.settings.background.is_some()
                                    && edit.settings.background_visible;
                                if ui
                                    .checkbox(&mut enabled, l10n::text(ui.ctx(), "Pane tint"))
                                    .changed()
                                {
                                    edit.settings.background_visible = enabled;
                                    if enabled && edit.settings.background.is_none() {
                                        edit.settings.background =
                                            Some(egui::Color32::from_rgba_unmultiplied(
                                                120, 123, 134, 16,
                                            ));
                                    }
                                }
                                if let Some(color) = &mut edit.settings.background {
                                    ui.color_edit_button_srgba(color)
                                        .accessible_name(l10n::text(ui.ctx(), "Pane tint colour"));
                                }
                            });
                            for (index, fill) in edit.settings.fills.iter_mut().enumerate() {
                                ui.push_id(("fill", index), |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.checkbox(
                                            &mut fill.visible,
                                            format!(
                                                "{} {}",
                                                l10n::text(ui.ctx(), "Band"),
                                                index + 1
                                            ),
                                        );
                                        ui.color_edit_button_srgba(&mut fill.color)
                                            .accessible_name(l10n::text(ui.ctx(), "Band colour"));
                                        let mut separate = fill.inverted_color.is_some();
                                        if ui
                                            .checkbox(
                                                &mut separate,
                                                l10n::text(ui.ctx(), "Alternate below"),
                                            )
                                            .changed()
                                        {
                                            fill.inverted_color = separate.then_some(fill.color);
                                        }
                                        if let Some(color) = &mut fill.inverted_color {
                                            ui.color_edit_button_srgba(color).accessible_name(
                                                l10n::text(ui.ctx(), "Band colour below"),
                                            );
                                        }
                                    });
                                });
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(l10n::text(ui.ctx(), "Precision"));
                                let mut automatic = edit.settings.precision.is_none();
                                if ui
                                    .checkbox(&mut automatic, l10n::text(ui.ctx(), "Automatic"))
                                    .changed()
                                {
                                    edit.settings.precision =
                                        if automatic { None } else { Some(2) };
                                }
                                if let Some(precision) = &mut edit.settings.precision {
                                    ui.add(
                                        egui::DragValue::new(precision)
                                            .range(0..=12)
                                            .suffix(l10n::text(ui.ctx(), " decimals")),
                                    )
                                    .accessible_name(l10n::text(ui.ctx(), "Precision"));
                                }
                            });
                        }
                        Tab::Visibility => {
                            ui.label(l10n::text(ui.ctx(), "Show this study on these intervals:"));
                            let mut intervals = Timeframe::all();
                            for interval in self
                                .current_interval
                                .iter()
                                .chain(&edit.settings.hidden_intervals)
                            {
                                if !intervals.contains(interval) {
                                    intervals.push(*interval);
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                for interval in intervals {
                                    let mut visible =
                                        !edit.settings.hidden_intervals.contains(&interval);
                                    if ui
                                        .checkbox(
                                            &mut visible,
                                            l10n::text(ui.ctx(), interval.to_string()),
                                        )
                                        .changed()
                                    {
                                        edit.settings.hidden_intervals.retain(|tf| *tf != interval);
                                        if !visible {
                                            edit.settings.hidden_intervals.push(interval);
                                        }
                                    }
                                }
                            });
                        }
                    });
                let valid = self.inputs.iter().all(|input| input.accepts(&input.value))
                    && edit
                        .settings
                        .levels
                        .iter()
                        .all(|level| level.value.is_none_or(f64::is_finite));
                (apply, cancel) = DialogFooter::new("Apply")
                    .primary_enabled(valid)
                    .secondary("Cancel")
                    .show(ui);
            });
        if !open || cancel {
            self.edit = None;
            return None;
        }
        if apply {
            let mut edit = self.edit.take()?;
            edit.inputs = self
                .inputs
                .iter()
                .map(|input| (input.id.clone(), input.value.clone()))
                .collect();
            return Some(edit);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{ConfiguredStudy, SMA};

    #[test]
    fn smi_value_lines_apply_cancel_save_and_restore_independently() {
        use crate::scales::{PriceMap, PriceRange, PriceTransform};
        use crate::studies::{StochasticMomentumIndex, StudyTemplate};
        use egui_kittest::{Harness, kittest::Queryable};

        let mut study = ConfiguredStudy::new(Box::new(StochasticMomentumIndex::new(5, 2, 2, 3)));
        let mut bars: Vec<_> = (0..6)
            .map(|i| {
                crate::model::Bar::new(
                    chrono::DateTime::from_timestamp(i * 60, 0).unwrap(),
                    100.0 + i as f64,
                    102.0 + i as f64,
                    98.0 + i as f64,
                    101.0 + i as f64,
                    1000.0,
                )
            })
            .collect();
        bars[5].close = 104.0;
        study.calculate(&bars);
        let defaults = study.settings();
        let before = study.snapshot_settings();
        assert!(defaults.price_line_plots.is_empty());
        assert!(defaults.levels.iter().all(|level| {
            !serde_json::to_value(level).unwrap()["price_line"]
                .as_bool()
                .unwrap_or(false)
        }));
        assert_eq!(
            study
                .pane()
                .levels
                .iter()
                .map(|level| level.0)
                .collect::<Vec<_>>(),
            [40.0, -40.0, 0.0]
        );
        assert_eq!(study.level_fill_indices(), Some([0, 1]));
        let mut dialog = IndicatorSettingsDialog::default();
        for (action, theme, size) in [
            ("Cancel", Theme::dark(), egui::vec2(360.0, 640.0)),
            ("Cancel", Theme::light(), egui::vec2(360.0, 640.0)),
            ("Apply", Theme::light(), egui::vec2(1440.0, 900.0)),
        ] {
            dialog.open(SeriesId(2), &study, Some(Timeframe::Min1));
            dialog.tab = Tab::Style;
            let mut applied = None;
            let setup_theme = theme.clone();
            let mut harness = Harness::builder()
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &setup_theme);
                })
                .with_size(size)
                .build_ui(|ui| {
                    if let Some(edit) = dialog.show(ui.ctx(), &theme) {
                        applied = Some(edit);
                    }
                });
            harness.run();
            assert_eq!(harness.query_all_by_label("Latest value line").count(), 5);
            // Main, signal, upper and lower. The middle guide keeps its default.
            for index in [0, 1, 2, 3] {
                harness
                    .query_all_by_label("Latest value line")
                    .nth(index)
                    .unwrap()
                    .click();
                harness.run();
            }
            assert!(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                    .contains_rect(harness.get_by_label(action).rect())
            );
            harness.get_by_label(action).click();
            harness.run();
            drop(harness);
            assert!(study.snapshot_settings() == before);
            if action == "Apply" {
                let edit = applied.unwrap();
                study.apply_settings(&edit.inputs, edit.settings).unwrap();
            } else {
                assert!(applied.is_none());
            }
        }
        study.calculate(&bars);
        let edited = study.settings();
        let json = serde_json::to_value(&edited).unwrap();
        assert_eq!(json["price_line_plots"], serde_json::json!([0, 1]));
        for (index, enabled) in [(0, true), (1, true), (2, false)] {
            assert_eq!(json["levels"][index]["price_line"], enabled);
            assert_eq!(edited.levels[index].visible, defaults.levels[index].visible);
        }
        assert_eq!(edited.untagged_plots, defaults.untagged_plots);
        let after = study.snapshot_settings();
        let saved = serde_json::to_value(StudyTemplate::from_study(&study)).unwrap();
        let mut loaded = serde_json::from_value::<StudyTemplate>(saved.clone())
            .unwrap()
            .create()
            .unwrap();
        loaded.calculate(&bars);
        assert_eq!(loaded.settings(), edited);
        study.restore_settings(&before).unwrap();
        assert_eq!(study.settings(), defaults);
        study.restore_settings(&after).unwrap();
        assert_eq!(study.settings(), edited);
        let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
        let id = chart.add_indicator(study.clone_box());
        chart.user_action(|chart| {
            chart
                .indicator_mut(id)
                .unwrap()
                .apply_settings(&[], defaults.clone())
                .unwrap();
        });
        assert!(chart.undo());
        assert_eq!(
            chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .settings(),
            edited
        );
        assert!(chart.redo());
        assert_eq!(
            chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .settings(),
            defaults
        );

        // Independent hand values at bar 5: SMI=475/9, signal=575/9.
        // A 200 px pane from -100 to 100 maps a value v to y=100-v.
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 200.0));
        for (plots, guides, expected) in [
            (vec![], vec![], vec![]),
            (vec![0], vec![], vec![475.0 / 9.0]),
            (vec![1], vec![], vec![575.0 / 9.0]),
            (vec![], vec![0], vec![40.0]),
            (vec![], vec![1], vec![-40.0]),
            (
                vec![0, 1],
                vec![0, 1],
                vec![475.0 / 9.0, 575.0 / 9.0, 40.0, -40.0],
            ),
        ] {
            for (inverted, visible) in [(false, true), (true, true), (false, false), (true, false)]
            {
                let mut settings = edited.clone();
                settings.price_line_plots = plots.clone();
                settings.untagged_plots = vec![0, 1];
                for (index, level) in settings.levels.iter_mut().enumerate() {
                    level.price_line = guides.contains(&index);
                    level.visible = visible;
                }
                study.apply_settings(&[], settings).unwrap();
                let price = PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(-100.0, 100.0),
                    0.0,
                    200.0,
                    inverted,
                );
                let output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    crate::chart::indicators::plots::paint_price_lines(
                        &ui.painter().with_clip_rect(rect),
                        &study,
                        &price,
                    );
                });
                let mut rows: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) if !mesh.vertices.is_empty() => {
                            let bounds = mesh.calc_bounds();
                            (bounds.width() >= 200.0 && bounds.height() <= 1.0)
                                .then_some(bounds.center().y)
                        }
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                let mut wanted: Vec<_> = expected
                    .iter()
                    .map(|value| {
                        if inverted {
                            100.0 + value
                        } else {
                            100.0 - value
                        }
                    })
                    .collect();
                rows.sort_by(f32::total_cmp);
                wanted.sort_by(f64::total_cmp);
                assert_eq!(
                    rows.len(),
                    wanted.len(),
                    "plots={plots:?}, guides={guides:?}, visible={visible}"
                );
                for (got, want) in rows.iter().zip(wanted) {
                    assert!(
                        (f64::from(*got) - want).abs() <= 0.5,
                        "{rows:?} vs {expected:?}"
                    );
                }
            }
        }
        // Settings predating these switches keep every new guide line disabled.
        let mut older = saved;
        for level in older["settings"]["levels"].as_array_mut().unwrap() {
            level.as_object_mut().unwrap().remove("price_line");
        }
        let older = serde_json::from_value::<StudyTemplate>(older)
            .unwrap()
            .create()
            .unwrap();
        let json = serde_json::to_value(older.settings()).unwrap();
        assert!(
            json["levels"]
                .as_array()
                .unwrap()
                .iter()
                .all(|level| level["price_line"] == false)
        );
    }

    /// CRSI declares its guides so the shared editor can stage, cancel, save and undo them.
    #[test]
    fn crsi_bands_and_calculation_survive_edits_and_restore() {
        use crate::studies::{ConnorsRSI, IndicatorFactory, StudyTemplate, palette};
        use egui_kittest::{Harness, kittest::Queryable};

        let mut study =
            ConfiguredStudy::new(IndicatorFactory::new().create("Connors RSI").unwrap());
        let defaults = study.settings();
        assert_eq!(
            study.pane().levels,
            [
                (70.0, palette::MUTED, "Upper guide"),
                (50.0, palette::MUTED.gamma_multiply_u8(128), "Middle guide"),
                (30.0, palette::MUTED, "Lower guide"),
            ]
        );
        assert!(defaults.fill_between_levels);
        assert_eq!(
            defaults.level_fill_color,
            palette::LIGHT_BLUE.gamma_multiply_u8(26)
        );
        assert!(defaults.price_line_plots.is_empty());
        assert_eq!(
            (defaults.plots[0].width, defaults.plots[0].plot_type),
            (1.0, PlotType::Line)
        );
        for level in &defaults.levels {
            assert_eq!(
                (level.width, level.pattern, level.visible),
                (1.0, LineStyle::Dashed, true)
            );
        }
        let before = study.snapshot_settings();
        let mut edited = defaults.clone();
        for (level, (value, color, pattern, width, visible)) in edited.levels.iter_mut().zip([
            (80.0, egui::Color32::RED, LineStyle::Solid, 3.0, false),
            (45.0, egui::Color32::GRAY, LineStyle::Dashed, 1.5, true),
            (20.0, egui::Color32::GREEN, LineStyle::Dotted, 2.0, true),
        ]) {
            level.value = Some(value);
            level.color = color;
            level.pattern = pattern;
            level.width = width;
            level.visible = visible;
        }
        edited.level_fill_color = egui::Color32::from_rgba_unmultiplied(80, 120, 160, 64);
        edited.price_line_plots = vec![0];
        edited.precision = Some(4);
        let mut dialog = IndicatorSettingsDialog::default();
        dialog.open(SeriesId(2), &study, Some(Timeframe::Min1));
        dialog.tab = Tab::Style;
        dialog.edit.as_mut().unwrap().settings = edited.clone();
        let mut applied = None;
        for (action, theme, size) in [
            ("Cancel", Theme::dark(), egui::vec2(360.0, 640.0)),
            ("Cancel", Theme::light(), egui::vec2(360.0, 640.0)),
            ("Apply", Theme::light(), egui::vec2(1440.0, 900.0)),
        ] {
            let setup_theme = theme.clone();
            let mut harness = Harness::builder()
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &setup_theme);
                })
                .with_size(size)
                .build_ui(|ui| {
                    if let Some(edit) = dialog.show(ui.ctx(), &theme) {
                        applied = Some(edit);
                    }
                });
            harness.run();
            for caption in [
                "Upper guide",
                "Lower guide",
                "Middle guide",
                "Fill between levels",
                "Latest value line",
                "Fill colour",
            ] {
                assert!(
                    harness.query_all_by_label(caption).next().is_some(),
                    "{caption}"
                );
            }
            assert!(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                    .contains_rect(harness.get_by_label(action).rect())
            );
            harness.get_by_label(action).click();
            harness.run();
            drop(harness);
            assert!(!dialog.is_open());
            assert!(study.snapshot_settings() == before);
            if action == "Cancel" {
                assert!(applied.is_none());
                dialog.open(SeriesId(2), &study, Some(Timeframe::Min1));
                dialog.tab = Tab::Style;
                dialog.edit.as_mut().unwrap().settings = edited.clone();
                for input in &mut dialog.inputs {
                    if matches!(
                        input.id.as_str(),
                        "rsi_period" | "streak_period" | "rank_period"
                    ) {
                        input.value = InputValue::Length(2);
                    }
                }
            }
        }
        let applied = applied.unwrap();
        study
            .apply_settings(&applied.inputs, applied.settings)
            .unwrap();
        assert_eq!(study.settings(), edited);
        assert_eq!(study.pane().levels[0].1, egui::Color32::TRANSPARENT);
        assert_eq!(study.level_style(20.0), LineStyle::Dotted);
        assert_eq!(study.level_width(20.0), 2.0);
        assert!(study.plot_price_line(0));
        let mut bars: Vec<_> = [100.0, 101.0, 102.0, 101.0, 100.0, 100.0]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                crate::model::Bar::new(
                    chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap(),
                    p,
                    p,
                    p,
                    p,
                    1.0,
                )
            })
            .collect();
        study.calculate(&bars);
        // Wilder RSI(2) price 25, streak 450/7, and return rank 100 give 1325/21.
        assert!((study.values().value(0, 5).unwrap() - 1325.0 / 21.0).abs() < 1e-9);
        bars[5].close = 101.0;
        study.update(&bars, 5);
        assert!((study.values().value(0, 5).unwrap() - 4225.0 / 54.0).abs() < 1e-9);
        bars.push(crate::model::Bar::new(
            chrono::DateTime::from_timestamp(360, 0).unwrap(),
            102.0,
            102.0,
            102.0,
            102.0,
            1.0,
        ));
        study.update(&bars, 6);
        assert!((study.values().value(0, 6).unwrap() - 3675.0 / 52.0).abs() < 1e-9);
        let saved = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
        let mut loaded = serde_json::from_str::<StudyTemplate>(&saved)
            .unwrap()
            .create()
            .unwrap();
        loaded.calculate(&bars);
        assert_eq!(loaded.settings(), edited);
        for bar in 2..bars.len() {
            assert_eq!(study.values().value(0, bar), loaded.values().value(0, bar));
        }
        let after = study.snapshot_settings();
        study
            .set_input(
                "source",
                InputValue::Source(crate::studies::StudySource::Price(
                    crate::model::PriceSource::Open,
                )),
            )
            .unwrap();
        study
            .set_input("plot_offset", InputValue::Integer(2))
            .unwrap();
        study.calculate(&bars);
        assert!((study.plot_value(0, 7).unwrap() - 1325.0 / 21.0).abs() < 1e-9);
        study.restore_settings(&after).unwrap();
        let mut hidden = edited.clone();
        hidden.fill_between_levels = false;
        study.apply_settings(&[], hidden).unwrap();
        assert!(!study.pane().fill_between_levels);
        assert_eq!(study.level_fill_color(), edited.level_fill_color);
        study.restore_settings(&before).unwrap();
        assert_eq!(study.settings(), defaults);
        study.restore_settings(&after).unwrap();
        assert_eq!(study.settings(), edited);
        // A saved study without level fields inherits the declared guides on restore.
        let mut older = serde_json::to_value(StudyTemplate::from_study(&study)).unwrap();
        for key in ["levels", "fill_between_levels", "level_fill_color"] {
            older["settings"].as_object_mut().unwrap().remove(key);
        }
        let older = serde_json::from_value::<StudyTemplate>(older)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(older.pane().levels, ConnorsRSI::default().pane().levels);
        assert!(older.pane().fill_between_levels);
    }

    #[test]
    fn ao_presentation_saves_cancels_and_restores_across_gaps() {
        use crate::studies::{AwesomeOscillator, StudyTemplate};
        let bars: Vec<_> = [1.0, 2.0, 4.0, f64::NAN, 3.0, 1.0, 4.0, 2.0]
            .into_iter()
            .map(|p| {
                crate::model::Bar::new("2026-01-01T00:00:00Z".parse().unwrap(), p, p, p, p, 1.0)
            })
            .collect();
        let mut study = ConfiguredStudy::new(Box::new(AwesomeOscillator::new(2, 3)));
        study.calculate(&bars);
        let before = study.snapshot_settings();
        let mut edited = serde_json::to_value(study.settings()).unwrap();
        edited["price_line_plots"] = serde_json::json!([0, 99, 0]);
        edited["untagged_plots"] = serde_json::json!([0]);
        edited["plot_variants"] = serde_json::json!([[{
            "name": "Decreasing bars", "color": [191, 71, 17, 255], "width": 5.0
        }]]);
        edited["background"] = serde_json::json!([20, 40, 60, 16]);
        edited["background_visible"] = false.into();
        edited["plots"][0]["color"] = serde_json::json!([17, 71, 191, 255]);
        edited["plots"][0]["width"] = 2.0.into();
        edited["plots"][0]["visible"] = false.into();
        let mut dialog = IndicatorSettingsDialog::default();
        dialog.open(SeriesId(2), &study, Some(Timeframe::Min1));
        dialog.tab = Tab::Style;
        dialog.edit.as_mut().unwrap().settings = serde_json::from_value(edited.clone()).unwrap();
        let ctx = egui::Context::default();
        let mut cancel_at = None;
        for pressed in [None, None, Some(true), Some(false)] {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events: pressed.map_or_else(Vec::new, |pressed| {
                        let pos = cancel_at.expect("Cancel button is visible");
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ]
                    }),
                    ..Default::default()
                },
                |ui| {
                    assert!(dialog.show(ui.ctx(), &Theme::dark()).is_none());
                },
            );
            cancel_at = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Cancel" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
        }
        assert!(!dialog.is_open());
        assert!(study.snapshot_settings() == before);
        study
            .apply_settings(&[], serde_json::from_value(edited).unwrap())
            .unwrap();
        study.calculate(&bars);
        let applied = study.snapshot_settings();
        let saved = serde_json::to_value(StudyTemplate::from_study(&study)).unwrap();
        assert_eq!(
            saved["settings"]["price_line_plots"],
            serde_json::json!([0])
        );
        assert_eq!(saved["settings"]["plot_variants"][0][0]["width"], 5.0);
        assert_eq!(saved["settings"]["background_visible"], false);
        let template: StudyTemplate = serde_json::from_value(saved).unwrap();
        let mut loaded = template.create().unwrap();
        loaded.calculate(&bars);
        assert_eq!(loaded.settings(), study.settings());
        assert!(loaded.values().value(0, 3).is_none());
        assert!(loaded.values().value(0, 5).is_none());
        // At bar 7: (4 + 2)/2 - (1 + 4 + 2)/3 = 2/3, recovering after the gap.
        assert!((loaded.values().value(0, 7).unwrap() - 2.0 / 3.0).abs() < 1e-12);
        study.restore_settings(&before).unwrap();
        study.calculate(&bars);
        assert!(study.snapshot_settings() == before);
        study.restore_settings(&applied).unwrap();
        study.calculate(&bars);
        assert_eq!(study.settings(), loaded.settings());
        // Saved presentation predating these controls keeps the old widths and no new tint or line.
        let mut older = serde_json::to_value(template).unwrap();
        for key in [
            "price_line_plots",
            "plot_variants",
            "background",
            "background_visible",
            "fills",
        ] {
            older["settings"].as_object_mut().unwrap().remove(key);
        }
        let older: StudyTemplate = serde_json::from_value(older).unwrap();
        let older = older.create().unwrap();
        let older = serde_json::to_value(older.settings()).unwrap();
        assert_eq!(older["price_line_plots"], serde_json::json!([]));
        assert!(older["background"].is_null());
        assert_eq!(older["plots"][0]["width"], 2.0);
    }

    #[test]
    fn dialog_follows_the_window_when_it_shrinks() {
        for theme in [Theme::light(), Theme::dark()] {
            for (name, tab) in [
                ("MA Ribbon", None),
                ("MA Ribbon", Some(Tab::Inputs)),
                ("MA Ribbon", Some(Tab::Style)),
                ("MA Ribbon", Some(Tab::Visibility)),
                ("Vol Profile FR", Some(Tab::Inputs)),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let study = crate::studies::IndicatorFactory::new()
                    .create(name)
                    .unwrap();
                let mut dialog = IndicatorSettingsDialog::default();
                dialog.open(SeriesId(2), study.as_ref(), Some(Timeframe::Min1));
                let mut browser = super::super::IndicatorDialog::new();
                browser.open();
                if let Some(tab) = tab {
                    dialog.tab = tab;
                }
                for size in [
                    egui::vec2(1440.0, 900.0),
                    egui::vec2(320.0, 360.0),
                    egui::vec2(600.0, 300.0),
                ] {
                    let size = if tab.is_none() {
                        egui::vec2(size.x.max(600.0), size.y.max(500.0))
                    } else {
                        size
                    };
                    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                    for _ in 0..4 {
                        ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(screen),
                                ..Default::default()
                            },
                            |ui| {
                                if tab.is_some() {
                                    dialog.show(ui.ctx(), &theme);
                                } else {
                                    browser.show(ui.ctx(), &theme);
                                }
                            },
                        )
                        .drop_without_applying_deltas();
                    }
                    let shown = ctx
                        .memory(|m| {
                            m.area_rect(egui::Id::unique(if tab.is_some() {
                                "indicator_settings"
                            } else {
                                "indicator_dialog"
                            }))
                        })
                        .unwrap();
                    assert!(
                        screen.contains_rect(shown),
                        "{name} tab {}: {shown:?} outside {screen:?}",
                        tab.map(|tab| tab as u8).unwrap_or(3)
                    );
                }
            }
        }
    }

    /// The smoothing length and band deviation hide when the chosen type does not use them and
    /// keep their values, which an edit still carries.
    #[test]
    fn smoothing_inputs_hide_when_they_do_not_apply() {
        use crate::studies::RSI;
        use egui_kittest::{Harness, kittest::Queryable};

        let mut study = RSI::default();
        study
            .set_input("smoothing_deviation", InputValue::Number(3.0))
            .unwrap();
        let mut dialog = IndicatorSettingsDialog::default();
        dialog.open(
            SeriesId(2),
            &ConfiguredStudy::new(Box::new(study)),
            Some(Timeframe::Min1),
        );
        let at = |id| {
            dialog
                .inputs
                .iter()
                .position(|input| input.id == id)
                .unwrap()
        };
        let (smoothing, deviation) = (at("smoothing"), at("smoothing_deviation"));
        for (choice, length_shown, deviation_shown) in [
            (0, false, false),
            (1, true, false),
            (2, true, true),
            (6, true, false),
        ] {
            dialog.inputs[smoothing].value = InputValue::Choice(choice);
            let mut harness = Harness::builder()
                .with_context_setup(egui_extras::install_image_loaders)
                .with_size(egui::Vec2::new(1440.0, 900.0))
                .build_ui(|ui| {
                    dialog.show(ui.ctx(), &Theme::dark());
                });
            harness.run();
            for (label, shown) in [
                ("Smoothing length", length_shown),
                ("Band deviation", deviation_shown),
            ] {
                let found = harness.query_all_by_label(label).next().is_some();
                assert_eq!(found, shown, "{label} under choice {choice}");
            }
            drop(harness);
            assert_eq!(dialog.inputs[deviation].value, InputValue::Number(3.0));
        }
    }

    #[test]
    fn escape_discards_staged_inputs_and_presentation() {
        let study = ConfiguredStudy::new(Box::new(SMA::new(20)));
        let mut dialog = IndicatorSettingsDialog::default();
        dialog.open(SeriesId(2), &study, Some(Timeframe::Min1));
        dialog.inputs[0].value = InputValue::Length(2);
        dialog.edit.as_mut().unwrap().settings.plots[0].visible = false;
        let ctx = egui::Context::default();
        for escape in [false, true] {
            let events = if escape {
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            } else {
                Vec::new()
            };
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    assert!(dialog.show(ui.ctx(), &Theme::dark()).is_none());
                },
            )
            .drop_without_applying_deltas();
        }
        assert!(!dialog.is_open());
        assert_eq!(study.inputs()[0].value, InputValue::Length(20));
        assert!(study.plot_style(0).visible);

        let mut chart = crate::widget::test_frames::chart(&[100.0], &[110.0]);
        let id = chart.add_indicator(Box::new(SMA::new(20)));
        assert!(dialog.open_for_chart(&chart, id));
        assert!(chart.set_indicator_user_edit_enabled(id, false));
        assert!(
            dialog
                .show_for_chart(&ctx, &Theme::dark(), &chart)
                .is_none()
        );
        assert!(!dialog.is_open());
        assert!(!dialog.open_for_chart(&chart, id));
        assert!(chart.set_indicator_user_edit_enabled(id, true));
        assert!(dialog.open_for_chart(&chart, id));
    }
}

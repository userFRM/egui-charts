//! The script editor: scripted studies written with completion, checked on the active chart's
//! bars before they replace its study, and saved locally.

use super::DemoApp;
use egui_charts::services::StoreKind;
use egui_charts::studies::{
    Calculation, CustomIndicator, Indicator, IndicatorFactory, IndicatorValue, InputKind,
    InputValue, StudyInput,
};
use egui_charts::ui::dialogs::templates::TemplateStore;
use egui_charts::ui::study_editor::{
    Completion, Diagnostic, Severity, StudyEditor, StudyEditorAction,
};
use egui_charts::widget::SeriesId;
use egui_charts_scripting::{Profile, Reference, Script, TokenClass};
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

/// The native sample study, still registered so that charts saved with it open.
const TITLE: &str = "Sample close difference";
const GUIDE: &str = "Scripts are Rhai with declarations for inputs, plots and study calls; Examples opens the bundled ones. Apply checks the script on the active chart's bars, then adds it to the chart and saves it locally. Other charts showing a script of the same title keep the version they run until they are opened again.";

pub(super) struct SampleStudyEditor {
    pub open: bool,
    editor: StudyEditor,
    choosing: bool,
    message: String,
    /// The saved scripts, in `StoreKind::Scripts`.
    store: TemplateStore<ScriptDocument>,
    /// Every saved script that compiles, by title, as registered in the study dialog.
    scripts: BTreeMap<String, Script>,
    /// Whether the editor completes and colours scripts, set up the first time it shows.
    hooked: bool,
    /// A script being checked before it replaces the chart's study: title, text, script, study.
    pending: Option<(String, String, Script, CustomIndicator)>,
    /// The script last applied from the editor, whose later errors and profile the editor shows.
    applied: Option<Script>,
    /// Whether Apply records the calls and time of each line.
    profiling: bool,
    /// The profile shown as line notes.
    noted: Option<Profile>,
}

/// The stored form of a script.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct ScriptDocument {
    source: String,
}

impl SampleStudyEditor {
    /// The editor of the scripts saved in `storage`, opened on the first bundled example.
    pub(super) fn new(storage: Arc<super::storage::DemoStorage>, ctx: &egui::Context) -> Self {
        let mut store = TemplateStore::<ScriptDocument>::new(StoreKind::Scripts);
        store.set_storage(storage);
        // The demo's storage answers at once, so one poll reads every saved script.
        store.poll(ctx);
        let factory = IndicatorFactory::new();
        let scripts = (store.entries().iter())
            .filter_map(|(title, document)| {
                let script = Script::compile(title.as_str(), &document.source, &factory).ok()?;
                Some((title.clone(), script))
            })
            .collect();
        let examples = egui_charts_scripting::examples();
        let mut editor = StudyEditor::new();
        editor.set_examples(
            (examples.iter())
                .map(|(title, text)| (title.to_string(), text.to_string()))
                .collect(),
        );
        editor.open_document(examples[0].0, examples[0].1);
        Self {
            open: false,
            editor,
            choosing: false,
            message: String::new(),
            store,
            scripts,
            hooked: false,
            pending: None,
            applied: None,
            profiling: false,
            noted: None,
        }
    }

    /// The demo catalogue: the built-in studies, the native sample and every saved script that
    /// compiles, so that saved charts holding them open from start-up.
    pub(super) fn study_factory(&self, color: egui::Color32) -> IndicatorFactory {
        let mut factory = IndicatorFactory::new();
        factory.register_in(IndicatorFactory::PERSONAL, TITLE, move || {
            Box::new(sample(color))
        });
        for (title, script) in &self.scripts {
            let script = script.clone();
            factory.register_in(IndicatorFactory::PERSONAL, title, move || {
                Box::new(script.study())
            });
        }
        factory
    }
}

/// The native sample: the close less the close a number of bars before, 10 by default.
fn sample(color: egui::Color32) -> CustomIndicator {
    CustomIndicator::new(TITLE, |bars| difference(bars, 10))
        .with_overlay(false)
        .with_color(color)
        .with_inputs(
            vec![StudyInput::new(
                "lookback",
                "Lookback bars",
                InputValue::Length(10),
                InputKind::Length(1, 10_000),
            )],
            |bars, inputs, _, _| {
                let InputValue::Length(lookback) = inputs[0].value else {
                    return Vec::new();
                };
                difference(bars, lookback)
            },
        )
        .expect("the sample's input is valid")
}

fn difference(bars: &[egui_charts::model::Bar], lookback: usize) -> Vec<IndicatorValue> {
    bars.iter()
        .enumerate()
        .map(|(i, bar)| {
            i.checked_sub(lookback)
                .map_or(IndicatorValue::None, |before| {
                    IndicatorValue::Single(bar.close - bars[before].close)
                })
        })
        .collect()
}

/// The colours of a script's words for the editor, from the theme.
fn colours(text: &str, visuals: &egui::Visuals) -> Vec<(Range<usize>, egui::Color32)> {
    // Dark themes draw links close to the text colour; there keywords take the accent, which
    // outlines an active control, lightened to read on the dark background.
    let keyword = if visuals.dark_mode {
        (visuals.widgets.active.bg_stroke.color).lerp_to_gamma(egui::Color32::WHITE, 0.4)
    } else {
        visuals.hyperlink_color
    };
    let tokens = egui_charts_scripting::tokens(text).into_iter();
    tokens
        .map(|(range, class)| {
            let color = match class {
                TokenClass::Keyword | TokenClass::Declaration => keyword,
                TokenClass::Number | TokenClass::Text => visuals.warn_fg_color,
                TokenClass::Comment => visuals.weak_text_color(),
                // Built-in names, and any class a later version adds.
                _ => visuals.strong_text_color(),
            };
            (range, color)
        })
        .collect()
}

impl DemoApp {
    pub(super) fn show_sample_study(&mut self, ui: &mut egui::Ui) {
        self.sample_study.store.poll(ui.ctx());
        self.check_script(ui.ctx());
        self.follow_applied_script();
        if !self.sample_study.open {
            return;
        }
        let editor = &mut self.sample_study;
        if !editor.hooked {
            editor.hooked = true;
            let reference = Reference::new(&IndicatorFactory::new());
            editor
                .editor
                .set_completer(Arc::new(move |text: &str, cursor: usize| {
                    let entries = reference.complete(text, cursor).into_iter();
                    entries
                        .map(|entry| {
                            let detail = format!("{}: {}", entry.signature, entry.summary);
                            Completion::new(&entry.name, &entry.name, detail)
                        })
                        .collect()
                }));
            editor.editor.set_highlighter(Arc::new(colours));
        }
        let mut action = StudyEditorAction::None;
        let mut load = None;
        egui::Panel::bottom("sample_study_editor")
            .resizable(true)
            .default_size(250.0)
            .size_range(180.0..=450.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Script editor");
                    if ui.button("Reload saved").clicked() {
                        load = Some(editor.editor.title().trim().to_owned());
                    }
                    ui.checkbox(&mut editor.profiling, "Profile lines");
                    if ui.button("Close editor").clicked() {
                        editor.open = false;
                    }
                });
                ui.label(GUIDE);
                if !editor.message.is_empty() {
                    ui.label(&editor.message);
                }
                if editor.choosing {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Open local document:");
                        for name in editor.store.entries().keys() {
                            if ui.button(name).clicked() {
                                load = Some(name.clone());
                            }
                        }
                        if ui.button("Cancel open").clicked() {
                            editor.choosing = false;
                        }
                    });
                }
                action = editor.editor.show(ui);
            });
        if let Some(name) = load {
            match editor.store.entries().get(&name) {
                Some(document) => {
                    editor.editor.open_document(name, document.source.clone());
                    editor.choosing = false;
                    editor.message.clear();
                }
                None => editor.message = "Save this document locally before reloading it.".into(),
            }
        }
        self.handle_sample_study(action);
    }

    fn handle_sample_study(&mut self, action: StudyEditorAction) {
        match action {
            StudyEditorAction::Open => self.sample_study.choosing = true,
            StudyEditorAction::Save { title, text } => {
                let title = title.trim();
                let saved = self.check_title(title, "saving").and_then(|()| {
                    let script = Script::compile(title, &text, &IndicatorFactory::new());
                    self.store_script(title, &text, script.ok())
                });
                self.sample_study.message = match saved {
                    Ok(()) => format!("Saved locally: {title}"),
                    Err(message) => message,
                };
            }
            StudyEditorAction::Apply { title, text } => {
                let title = title.trim();
                // A newer Apply replaces the check of an older one.
                self.sample_study.pending = None;
                self.sample_study.message.clear();
                if let Err(message) = self.check_title(title, "applying") {
                    self.sample_study.message = message;
                    return;
                }
                match Script::compile(title, &text, &IndicatorFactory::new()) {
                    Ok(script) => {
                        let script = script.with_profiling(self.sample_study.profiling);
                        let mut study = script.study();
                        study.calculate(self.chart.bars(SeriesId::MAIN));
                        self.sample_study.pending = Some((title.to_owned(), text, script, study));
                        self.sample_study.message = "Checking the script".into();
                    }
                    Err(error) => {
                        let editor = &mut self.sample_study.editor;
                        let span = error.span(&text);
                        let message = error.to_string();
                        editor.set_diagnostics(vec![Diagnostic::new(
                            Severity::Error,
                            message.clone(),
                            span,
                        )]);
                        editor.apply_result(Err(message));
                    }
                }
            }
            _ => {}
        }
    }

    /// Refuses a blank title and the name of a study of the dialog that is not a script, so that
    /// a script never replaces a built-in study, the native sample or a feed's metric.
    fn check_title(&self, title: &str, doing: &str) -> Result<(), String> {
        if title.is_empty() {
            Err(format!("Enter a document title before {doing}."))
        } else if self.indicator_dialog.factory().has(title)
            && !self.sample_study.scripts.contains_key(title)
        {
            Err(format!(
                "A built-in study is named {title}; choose another title."
            ))
        } else {
            Ok(())
        }
    }

    /// Saves `text` as the script `title` and registers `script`, when it compiled, in the study
    /// dialog.
    fn store_script(
        &mut self,
        title: &str,
        text: &str,
        script: Option<Script>,
    ) -> Result<(), String> {
        let editor = &mut self.sample_study;
        let document = ScriptDocument {
            source: text.to_owned(),
        };
        editor.store.save(title, document)?;
        editor.editor.mark_saved(text);
        if let Some(script) = script {
            editor.scripts.insert(title.to_owned(), script);
            let factory = editor.study_factory(self.theme.ui.accent);
            let dialog = std::mem::take(&mut *self.indicator_dialog);
            *self.indicator_dialog = dialog.with_factory(factory);
        }
        Ok(())
    }

    /// Advances the check of the script being applied by one frame. Once it ends without an
    /// error, the script replaces the studies of its title on the active chart and is saved and
    /// registered; otherwise the chart is left as it was and the error is shown where it is.
    fn check_script(&mut self, ctx: &egui::Context) {
        let editor = &mut self.sample_study;
        let Some((_, text, script, study)) = &mut editor.pending else {
            return;
        };
        let error = match study.calculation() {
            Some(Calculation::Running(done)) => {
                let bars = self.chart.bars(SeriesId::MAIN);
                study.update(bars, bars.len());
                editor.message = format!("Checking the script, {:.0} %", done * 100.0);
                ctx.request_repaint();
                return;
            }
            Some(Calculation::Stopped(message)) => {
                Some((script.last_error()).map_or((message, 0..0), |error| {
                    (error.to_string(), error.span(text))
                }))
            }
            _ => (script.last_error()).map(|error| (error.to_string(), error.span(text))),
        };
        let Some((title, text, script, study)) = editor.pending.take() else {
            return;
        };
        editor.message.clear();
        if let Some((message, span)) = error {
            let diagnostic = Diagnostic::new(Severity::Error, message.clone(), span);
            editor.editor.set_diagnostics(vec![diagnostic]);
            editor.editor.apply_result(Err(message));
            return;
        }
        self.chart.user_action(|chart| {
            let previous: Vec<_> = chart
                .indicators()
                .filter(|(_, study)| study.name() == title)
                .map(|(id, _)| id)
                .collect();
            for id in previous {
                chart.remove_series(id);
            }
            chart.add_indicator(Box::new(study));
        });
        if let Err(message) = self.store_script(&title, &text, Some(script.clone())) {
            self.sample_study.message = message;
        }
        let editor = &mut self.sample_study;
        editor.editor.set_diagnostics(Vec::new());
        editor.editor.apply_result(Ok(()));
        editor.applied = Some(script);
        editor.noted = None;
    }

    /// Shows the later runtime errors of the script last applied and, while lines are profiled,
    /// the calls and time of each line, as long as the editor holds a document of its title.
    fn follow_applied_script(&mut self) {
        let editor = &mut self.sample_study;
        let Some(script) = (editor.applied.as_ref())
            .filter(|script| script.name() == editor.editor.title().trim())
        else {
            return;
        };
        if let Some(error) = script.last_error() {
            let message = error.to_string();
            let shown = editor.editor.diagnostics();
            if !shown.iter().any(|diagnostic| diagnostic.message == message) {
                let span = error.span(editor.editor.text());
                let diagnostic = Diagnostic::new(Severity::Error, message, span);
                editor.editor.set_diagnostics(vec![diagnostic]);
            }
        }
        if editor.profiling
            && let Some(profile) = script.profile()
            && editor.noted.as_ref() != Some(&profile)
        {
            let lines = profile.lines().iter();
            let notes = lines
                .map(|&(line, calls, time)| {
                    let ms = time.as_secs_f64() * 1e3;
                    (line, format!("{calls} calls, {ms:.1} ms"))
                })
                .collect();
            editor.editor.set_line_notes(notes);
            editor.noted = Some(profile);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DemoApp;
    use eframe::App;
    use egui::{Event, Key, Modifiers, Pos2, RawInput};
    use egui_charts::model::{Bar, BarData};
    use egui_charts::studies::{Calculation, IndicatorFactory, InputValue};
    use egui_charts::widget::{ChartLayoutMode, SeriesId};
    use std::time::Duration;

    /// A script with one input: the close less the close `lookback` bars before.
    const LOOKBACK: &str = "input lookback = length(\"Lookback bars\", 1, 1, 10000);\nplot(\"Difference\", close - close[lookback]);";

    #[derive(Default)]
    struct Memory(std::collections::HashMap<String, String>);

    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    /// The text shapes of one frame.
    fn frame(
        app: &mut DemoApp,
        ctx: &egui::Context,
        events: Vec<Event>,
    ) -> Vec<egui::epaint::TextShape> {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
        );
        let texts = (output.shapes.iter())
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    fn render(app: &mut DemoApp, ctx: &egui::Context, events: Vec<Event>) -> Vec<(String, Pos2)> {
        let texts = frame(app, ctx, events).into_iter();
        texts
            .map(|text| {
                let center = text.pos + text.galley.size() / 2.0;
                (text.galley.text().to_owned(), center)
            })
            .collect()
    }

    /// The underlined parts of the drawn `text`.
    fn underlined(app: &mut DemoApp, ctx: &egui::Context, text: &str) -> Vec<String> {
        let shapes = frame(app, ctx, Vec::new());
        let shape = (shapes.iter())
            .find(|shape| shape.galley.text() == text)
            .unwrap_or_else(|| panic!("not drawn: {text}"));
        let sections = shape.galley.job.sections.iter();
        sections
            .filter(|section| section.format.underline != egui::Stroke::NONE)
            .map(|section| text[section.byte_range.start.0..section.byte_range.end.0].to_owned())
            .collect()
    }

    fn click(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let point = render(app, ctx, Vec::new())
            .into_iter()
            .find_map(|(text, point)| {
                (text == label && point.y > ctx.content_rect().center().y).then_some(point)
            })
            .unwrap_or_else(|| panic!("missing control: {label}"));
        render(
            app,
            ctx,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }

    fn edit(app: &mut DemoApp, ctx: &egui::Context, old: &str, new: &str) {
        click(app, ctx, old);
        render(
            app,
            ctx,
            vec![
                Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
                Event::Text(new.into()),
            ],
        );
    }

    /// The demo with its live feed paused and the script editor open.
    fn editor_app(ctx: &egui::Context) -> Box<DemoApp> {
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.execute_search_command(ctx, "sample-study");
        app
    }

    /// One bar a minute from the epoch, at `closes`.
    fn minute_bars(closes: impl IntoIterator<Item = f64>) -> Vec<Bar> {
        let closes = closes.into_iter().enumerate();
        closes
            .map(|(i, close)| {
                let time = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i as i64);
                Bar::from_value(time, close)
            })
            .collect()
    }

    /// Renders frames until the script being applied has been checked and no study of a chart
    /// is calculating.
    fn calculated(app: &mut DemoApp, ctx: &egui::Context) {
        for _ in 0..5_000 {
            render(app, ctx, Vec::new());
            let extra = app.extra_charts.iter().map(|extra| &extra.chart);
            let mut studies = std::iter::once(&app.chart)
                .chain(extra)
                .flat_map(|chart| chart.indicators());
            let running = studies
                .any(|(_, study)| matches!(study.calculation(), Some(Calculation::Running(_))));
            if app.sample_study.pending.is_none() && !running {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("the scripts did not finish calculating");
    }

    /// The id of the only study named `name` on `chart`.
    fn study_id(chart: &egui_charts::Chart, name: &str) -> SeriesId {
        let mut ids = (chart.indicators())
            .filter(|(_, study)| study.name() == name)
            .map(|(id, _)| id);
        let id = ids
            .next()
            .unwrap_or_else(|| panic!("no study named {name}"));
        assert!(ids.next().is_none(), "two studies named {name}");
        id
    }

    /// The first output of the study `id` of `chart` on each bar.
    fn values(chart: &egui_charts::Chart, id: SeriesId) -> Vec<Option<f64>> {
        let (_, study) = (chart.indicators())
            .find(|(candidate, _)| *candidate == id)
            .unwrap();
        let values = study.values();
        (0..chart.bars(SeriesId::MAIN).len())
            .map(|bar| values.value(0, bar))
            .collect()
    }

    #[test]
    fn editor_applies_live_inputs_preserves_invalid_edits_and_reopens_local_documents() {
        let ctx = egui::Context::default();
        let mut app = editor_app(&ctx);
        assert!(
            render(&mut app, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text == "Script editor")
        );
        let bars = minute_bars([100.0, 103.0, 101.0]);
        let last = bars[2].time;
        *app.chart = egui_charts::Chart::new(BarData::from_bars(bars));
        app.sample_study
            .editor
            .open_document("Close difference", LOOKBACK);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        let id = study_id(&app.chart, "Close difference");
        assert_eq!(values(&app.chart, id), [None, Some(3.0), Some(-2.0)]);
        // A second chart showing the script, beside which another version is applied below.
        app.ensure_chart_slots(ChartLayoutMode::TwoHorizontal);

        let broken = LOOKBACK.replace("close[lookback]);", "close[");
        edit(&mut app, &ctx, LOOKBACK, &broken);
        click(&mut app, &ctx, "Apply");
        let text = render(&mut app, &ctx, Vec::new());
        assert!(
            text.iter()
                .any(|(text, _)| text.starts_with("line 2, column"))
        );
        assert!(text.iter().any(|(text, _)| *text == broken));
        assert_eq!(study_id(&app.chart, "Close difference"), id);
        app.chart
            .indicator_mut(id)
            .unwrap()
            .set_input("lookback", InputValue::Length(2))
            .unwrap();
        calculated(&mut app, &ctx);
        assert_eq!(values(&app.chart, id)[2], Some(1.0));
        app.chart
            .indicator_mut(id)
            .unwrap()
            .set_input("lookback", InputValue::Length(1))
            .unwrap();
        app.chart
            .update(SeriesId::MAIN, Bar::from_value(last, 108.0));
        calculated(&mut app, &ctx);
        assert_eq!(values(&app.chart, id)[2], Some(5.0));

        // Applied again, a script replaces its study on the active chart only: the other chart
        // keeps the version it runs.
        let negated = "input lookback = length(\"Bars back\", 1, 1, 10000);\nplot(\"Difference\", close[lookback] - close);";
        edit(&mut app, &ctx, &broken, negated);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        let id = study_id(&app.chart, "Close difference");
        assert_eq!(values(&app.chart, id), [None, Some(-3.0), Some(-5.0)]);
        let other = &app.extra_charts[0].chart;
        let (_, kept) = (other.indicators())
            .find(|(_, study)| study.name() == "Close difference")
            .unwrap();
        assert_eq!(kept.inputs()[0].label, "Lookback bars");
        // The copy of this test chart is still reading its drawings, which would keep the
        // workspace from being saved below.
        app.ensure_chart_slots(ChartLayoutMode::Single);

        edit(&mut app, &ctx, negated, LOOKBACK);
        click(&mut app, &ctx, "Save");
        edit(&mut app, &ctx, LOOKBACK, &broken);
        click(&mut app, &ctx, "Reload saved");
        assert!(
            render(&mut app, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text == LOOKBACK)
        );
        // The restarted application has storage: its first-visit choice is already answered.
        let answered = super::super::owndata::store::Saved {
            asked: true,
            ..Default::default()
        };
        answered.save(&app.storage);
        let mut saved = Memory::default();
        app.save(&mut saved);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&saved);
        let mut restored = DemoApp::new(&cc);
        restored.live.set_paused(true);
        restored.execute_search_command(&ctx, "sample-study");
        click(&mut restored, &ctx, "Open");
        click(&mut restored, &ctx, "Close difference");
        assert!(
            render(&mut restored, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text == LOOKBACK)
        );

        // The workspace kept the applied study with its input and opened it as saved, and the
        // study copies and applies.
        let restored_study = |app: &DemoApp| {
            app.chart
                .indicators()
                .filter(|(_, study)| study.name() == "Close difference")
                .map(|(_, study)| {
                    let input = study.inputs().swap_remove(0);
                    (input.label, input.value)
                })
                .collect::<Vec<_>>()
        };
        let saved_version = ("Lookback bars".to_owned(), InputValue::Length(1));
        assert_eq!(
            restored_study(&restored),
            std::slice::from_ref(&saved_version)
        );
        restored.paste_drawing_text(&app.chart.copy_indicator(id).unwrap());
        assert_eq!(restored_study(&restored).len(), 2);
        restored.load_study_template(&app.chart.study_template_set());
        assert_eq!(restored_study(&restored), [saved_version]);
    }

    #[test]
    fn editor_runs_study_calls_histories_and_the_profiler_on_the_active_chart() {
        let ctx = egui::Context::default();
        let mut app = editor_app(&ctx);
        // The chart's feed serves the daily history the script reads.
        let daily = "bars day = history(\"\", \"1D\");\nplot(\"Day close\", day.close);";
        app.sample_study.editor.open_document("Day close", daily);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        let id = study_id(&app.chart, "Day close");
        let mut last = None;
        for _ in 0..500 {
            super::super::tests::settle(&mut app, &ctx);
            last = values(&app.chart, id).last().copied().flatten();
            if last.is_some_and(f64::is_finite) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let closes: Vec<_> = (app.market_history().iter()).map(|bar| bar.close).collect();
        assert!(last.is_some_and(|last| closes.contains(&last)), "{last:?}");

        // A study call gives the values of the built-in study on the chart's bars.
        let bars = minute_bars((0..40).map(|i| 100.0 + (i % 7) as f64 - (i % 3) as f64));
        *app.chart = egui_charts::Chart::new(BarData::from_bars(bars.clone()));
        let call = "plot(\"x\", study(\"RSI\", period = 14));";
        app.sample_study.editor.open_document("RSI copy", call);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        let mut rsi = IndicatorFactory::new().create("RSI").unwrap();
        rsi.calculate(&bars);
        let native: Vec<_> = (0..bars.len())
            .map(|bar| rsi.values().value(0, bar))
            .collect();
        assert_eq!(values(&app.chart, study_id(&app.chart, "RSI copy")), native);

        // Profiled, each line shows the calls and time of its runtime operations.
        click(&mut app, &ctx, "Profile lines");
        let average = "let a = sma(close, 5);\nplot(\"x\", a);";
        app.sample_study.editor.open_document("Profiled", average);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        let notes = render(&mut app, &ctx, Vec::new())
            .into_iter()
            .filter(|(text, _)| text.starts_with("40 calls, "))
            .count();
        assert_eq!(notes, 2);
    }

    #[test]
    fn editor_completes_at_the_caret_and_keeps_the_study_on_errors() {
        let ctx = egui::Context::default();
        let mut app = editor_app(&ctx);
        let bars = minute_bars([100.0, 103.0, 101.0]);
        let next = bars[2].time + chrono::TimeDelta::minutes(1);
        *app.chart = egui_charts::Chart::new(BarData::from_bars(bars));
        app.sample_study
            .editor
            .open_document("Close difference", LOOKBACK);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        let id = study_id(&app.chart, "Close difference");
        let applied = [None, Some(3.0), Some(-2.0)];
        assert_eq!(values(&app.chart, id), applied);

        // Completion offers the inputs of the called study at the caret, and Tab writes one.
        edit(
            &mut app,
            &ctx,
            LOOKBACK,
            "let strength = study(\"RSI\", per",
        );
        assert!(
            render(&mut app, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text == "period")
        );
        let tab = Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        render(&mut app, &ctx, vec![tab]);
        let completed = "let strength = study(\"RSI\", period";
        assert_eq!(app.sample_study.editor.text(), completed);

        // A failed check leaves the chart's study as it was and shows the error where it is.
        let unknown = "plot(\"x\", nope(close));";
        edit(&mut app, &ctx, completed, unknown);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        assert!(
            render(&mut app, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text.contains("line 1, column 11"))
        );
        assert_eq!(underlined(&mut app, &ctx, unknown), ["nope"]);
        let mut text = unknown;
        for (script, shown) in [
            ("plot(\"x\", close[-1]);", "on bar 0"),
            (
                "loop {}\nplot(\"x\", close);",
                "time limit of 500 ms for one bar reached",
            ),
        ] {
            edit(&mut app, &ctx, text, script);
            text = script;
            click(&mut app, &ctx, "Apply");
            calculated(&mut app, &ctx);
            assert!(
                render(&mut app, &ctx, Vec::new())
                    .iter()
                    .any(|(drawn, _)| drawn.contains(shown)),
                "{script}"
            );
            assert_eq!(study_id(&app.chart, "Close difference"), id, "{script}");
            assert_eq!(values(&app.chart, id), applied, "{script}");
        }

        // An error of the applied script on a later bar is shown too.
        let late = "plot(\"x\", if bar_index > 2 { close[-1] } else { close });";
        edit(&mut app, &ctx, text, late);
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);
        assert_ne!(study_id(&app.chart, "Close difference"), id);
        app.chart
            .update(SeriesId::MAIN, Bar::from_value(next, 104.0));
        let mut shown = false;
        for _ in 0..500 {
            calculated(&mut app, &ctx);
            let texts = render(&mut app, &ctx, Vec::new());
            shown = texts.iter().any(|(text, _)| text.contains("on bar 3"));
            if shown {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(shown);
    }
}

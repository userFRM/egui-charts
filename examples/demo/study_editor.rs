//! The script editor: scripted studies and strategies written with completion, studies checked on
//! the active chart's bars before they replace its study, strategies selected in the strategy
//! tester, and both saved locally.

use super::DemoApp;
use egui_charts::services::StoreKind;
use egui_charts::strategies::StrategyFactory;
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
const GUIDE: &str = "Scripts are Rhai with declarations for inputs, plots and study calls; Examples opens the bundled ones. alert(\"message\", condition) declares an alert condition: after Apply, Add alert offers it as a condition of the study. Apply checks the script on the active chart's bars, then adds it to the chart and saves it locally; Profile lines takes effect at the next Apply. Other charts showing a script of the same title keep the version they run until they are opened again. A script that places orders is a strategy: Apply saves it and selects it in the strategy tester, where Run backtests it.";

pub(super) struct SampleStudyEditor {
    pub open: bool,
    editor: StudyEditor,
    choosing: bool,
    message: String,
    /// The saved scripts, in `StoreKind::Scripts`.
    store: TemplateStore<ScriptDocument>,
    /// Every saved script that compiles, by title: the studies as registered in the study dialog,
    /// the strategies as registered in the strategy tester.
    scripts: BTreeMap<String, Script>,
    /// The editor's text as last read and whether it places orders, `None` when it does not
    /// compile.
    kind: (String, Option<bool>),
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
            kind: (String::new(), None),
            hooked: false,
            pending: None,
            applied: None,
            profiling: false,
            noted: None,
        }
    }

    /// The demo catalogue: the built-in studies, the native sample and every saved study script
    /// that compiles, so that saved charts holding them open from start-up.
    pub(super) fn study_factory(&self, color: egui::Color32) -> IndicatorFactory {
        let mut factory = IndicatorFactory::new();
        factory.register_in(IndicatorFactory::PERSONAL, TITLE, move || {
            Box::new(sample(color))
        });
        let studies = self
            .scripts
            .iter()
            .filter(|(_, script)| !script.places_orders());
        for (title, script) in studies {
            let script = script.clone();
            factory.register_in(IndicatorFactory::PERSONAL, title, move || {
                Box::new(script.study())
            });
        }
        factory
    }

    /// The strategy tester's catalogue: the built-in strategies and every saved strategy script
    /// that compiles.
    pub(super) fn strategy_factory(&self) -> StrategyFactory {
        let mut factory = StrategyFactory::new();
        let strategies = self
            .scripts
            .iter()
            .filter(|(_, script)| script.places_orders());
        for (title, script) in strategies {
            let script = script.clone();
            factory.register(title, move || Box::new(script.strategy()));
        }
        factory
    }

    /// What the editor's text is, read again only when it changed.
    fn kind(&mut self) -> &'static str {
        let text = self.editor.text();
        if self.kind.0 != text {
            let compiled = Script::compile("", text, &IndicatorFactory::new());
            self.kind = (
                text.to_owned(),
                compiled.ok().map(|script| script.places_orders()),
            );
        }
        match self.kind.1 {
            Some(true) => "This script places orders, so it is a strategy.",
            Some(false) => "This script is a study.",
            None => "",
        }
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

/// The past tense of `doing`, "saving" or "applying", for a refusal.
fn done(doing: &str) -> &'static str {
    match doing {
        "saving" => "saved",
        _ => "applied",
    }
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
                let kind = editor.kind();
                if !kind.is_empty() {
                    ui.label(kind);
                }
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
                let compiled = Script::compile(title, &text, &IndicatorFactory::new());
                let orders = compiled.as_ref().ok().map(Script::places_orders);
                let saved = self.check_title(title, "saving", orders).and_then(|()| {
                    match compiled {
                        // Charts and saved layouts hold the registered version, which start-up
                        // registers again from the stored text only when it compiles.
                        Err(error) if self.sample_study.scripts.contains_key(title) => Err(
                            format!("Not saved: {title} is in use and this text does not compile ({error}). Fix it, or save it under another title."),
                        ),
                        script => self.store_script(title, &text, script.ok()),
                    }
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
                let compiled = Script::compile(title, &text, &IndicatorFactory::new());
                let orders = compiled.as_ref().ok().map(Script::places_orders);
                if let Err(message) = self.check_title(title, "applying", orders) {
                    self.sample_study.message = message;
                    return;
                }
                match compiled {
                    Ok(script) if script.places_orders() => {
                        self.apply_strategy(title, &text, script);
                    }
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

    /// Refuses a blank title, for a study the name of a study of the dialog that is not a script
    /// and for a strategy the name of a built-in strategy, so that a script never replaces a
    /// built-in study, the native sample, a feed's metric or a built-in strategy. A title saved
    /// as a study cannot take a strategy or the reverse, since the charts, layouts and tester
    /// holding it would no longer open it. `orders` says whether the text places orders, `None`
    /// when it does not compile.
    fn check_title(&self, title: &str, doing: &str, orders: Option<bool>) -> Result<(), String> {
        let saved = self.sample_study.scripts.get(title);
        if title.is_empty() {
            Err(format!("Enter a document title before {doing}."))
        } else if orders == Some(true) {
            if StrategyFactory::new().create(title).is_some() {
                Err(format!(
                    "A built-in strategy is named {title}; choose another title."
                ))
            } else if saved.is_some_and(|saved| !saved.places_orders()) {
                Err(format!(
                    "Not {}: {title} is saved as a study; save this strategy under another title.",
                    done(doing)
                ))
            } else {
                Ok(())
            }
        } else if self.indicator_dialog.factory().has(title) && saved.is_none() {
            Err(format!(
                "A built-in study is named {title}; choose another title."
            ))
        } else if orders == Some(false) && saved.is_some_and(Script::places_orders) {
            Err(format!(
                "Not {}: {title} is saved as a strategy; save this study under another title.",
                done(doing)
            ))
        } else {
            Ok(())
        }
    }

    /// Saves `text` as the script `title` and registers `script`, when it compiled, in the study
    /// dialog, or in the strategy tester when it places orders.
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
            let orders = script.places_orders();
            editor.scripts.insert(title.to_owned(), script);
            if orders {
                self.strategy_report.set_factory(editor.strategy_factory());
            } else {
                let factory = editor.study_factory(self.theme.ui.accent);
                let dialog = std::mem::take(&mut *self.indicator_dialog);
                *self.indicator_dialog = dialog.with_factory(factory);
            }
        }
        Ok(())
    }

    /// Saves the strategy `script` and selects it in the strategy tester, which shows, so that Run
    /// there backtests it on the active chart's bars.
    fn apply_strategy(&mut self, title: &str, text: &str, script: Script) {
        if self.synthetic_only("Strategy tester") {
            return;
        }
        let selected = self
            .store_script(title, text, Some(script.clone()))
            .and_then(|()| (self.strategy_report.select_strategy(title)).map_err(String::from));
        let editor = &mut self.sample_study;
        if let Err(message) = selected {
            editor.message = message;
            return;
        }
        self.strategy_open = true;
        editor.editor.set_diagnostics(Vec::new());
        editor.editor.apply_result(Ok(()));
        editor.applied = Some(script);
        editor.noted = None;
        editor.message = "Selected in the strategy tester; Run there backtests it.".into();
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

    /// Clicks the lowest drawn `label`: the bottom panels' control rather than the chart's.
    fn click(app: &mut DemoApp, ctx: &egui::Context, label: &str) {
        let point = render(app, ctx, Vec::new())
            .into_iter()
            .filter_map(|(text, point)| (text == label).then_some(point))
            .max_by(|a, b| a.y.total_cmp(&b.y))
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
        // A text that does not compile is not saved over a script charts use.
        edit(&mut app, &ctx, LOOKBACK, &broken);
        click(&mut app, &ctx, "Save");
        assert!(
            render(&mut app, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text.starts_with("Not saved: Close difference is in use"))
        );
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
    fn editor_backtests_strategy_scripts_in_the_strategy_tester() {
        let ctx = egui::Context::default();
        let mut app = editor_app(&ctx);
        // The tester runs on the first five bars; the last is still forming.
        let bars = minute_bars([100.0, 101.0, 103.0, 102.0, 101.0, 101.0]);
        *app.chart = egui_charts::Chart::new(BarData::from_bars(bars));
        let momentum = "if close > close[1] && position() == 0.0 {\n    entry(\"Long\", \"long\", #{ qty: equity() / 1000.0 });\n} else if close < close[1] && position() > 0.0 {\n    exit_order(\"Close\");\n}";
        app.sample_study
            .editor
            .open_document("Momentum script", momentum);
        let shown = |app: &mut DemoApp, wanted: &dyn Fn(&str) -> bool| {
            (render(app, &ctx, Vec::new()).iter()).any(|(text, _)| wanted(text))
        };
        assert!(shown(&mut app, &|text| text
            == "This script places orders, so it is a strategy."));
        click(&mut app, &ctx, "Apply");
        assert_eq!(app.strategy_report.strategy(), "Momentum script");
        assert!(shown(&mut app, &|text| text == "Strategy tester"));
        assert!(!app.indicator_dialog.factory().has("Momentum script"));

        let run = |app: &mut DemoApp| {
            click(app, &ctx, "Run");
            for _ in 0..1_000 {
                if !app.strategy_report.is_stale() {
                    return;
                }
                render(app, &ctx, Vec::new());
            }
            panic!("the backtest did not finish");
        };
        // Bought 10 at the open of 103 after the rise to 101, sold at the open of 101 after the
        // fall to 102.
        run(&mut app);
        let report = app.strategy_report.report().expect("a report");
        let [trade] = report.trades.as_slice() else {
            panic!("{:?}", report.trades);
        };
        let exit = trade.exit.as_ref().map(|exit| (exit.price, exit.profit));
        assert_eq!(
            (trade.entry_price, trade.quantity, exit),
            (103.0, 10.0, Some((101.0, -20.0)))
        );

        // A runtime error withholds the report and is shown at its line and bar.
        let broken = momentum.replace("\"long\", #{ qty: equity() / 1000.0 }", "\"up\"");
        edit(&mut app, &ctx, momentum, &broken);
        click(&mut app, &ctx, "Apply");
        run(&mut app);
        assert!(shown(&mut app, &|text| text.starts_with("line 2, column")
            && text.ends_with(", on bar 1")));
        assert!(app.strategy_report.report().unwrap().trades.is_empty());

        // A title saved as a strategy takes no study.
        let study = "plot(\"x\", close);";
        edit(&mut app, &ctx, &broken, study);
        click(&mut app, &ctx, "Save");
        assert!(shown(&mut app, &|text| text
            == "Not saved: Momentum script is saved as a strategy; save this study under another title."));

        // The restarted application lists the saved strategy in its tester.
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
        assert_eq!(
            restored.strategy_report.select_strategy("Momentum script"),
            Ok(())
        );
    }

    /// An applied script's alert call is a condition of its study: Add alert offers it, the
    /// alert fires on it, and it is still there after a restart.
    #[test]
    fn editor_alerts_fire_on_script_conditions() {
        use egui_charts::model::{AlertCondition, AlertSource};
        let ctx = egui::Context::default();
        // A fixed time keeps the generated feed the same on every run.
        let mut app = DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            "2026-11-04T17:30:00Z".parse().unwrap(),
            false,
        );
        app.live.set_paused(true);
        super::super::tests::finish_feed(&app.live);
        app.apply_session();
        app.execute_search_command(&ctx, "sample-study");
        app.sample_study.editor.open_document(
            "Positive close",
            "alert(\"Close above zero\", close > 0.0);\nplot(\"Close\", close);",
        );
        click(&mut app, &ctx, "Apply");
        calculated(&mut app, &ctx);

        app.open_alert(None);
        let (source, _) = (app.alert_dialog.sources.iter())
            .find(|(_, name)| name.to_string() == "Positive close: Close above zero")
            .cloned()
            .expect("the dialog offers the script's condition");
        assert!(matches!(
            source,
            AlertSource::StudyCondition { condition: 0, .. }
        ));
        app.alert_dialog.alert.source = source.clone();
        app.alert_dialog.alert.condition = AlertCondition::Holds;
        let created = app.alert_host.engine().alerts().len();
        // The modal places its controls from its second frame on.
        render(&mut app, &ctx, Vec::new());
        click(&mut app, &ctx, "Save alert");
        let alerts = app.alert_host.engine().alerts();
        assert_eq!(alerts.len(), created + 1);
        let alert = alerts.last().unwrap().clone();
        assert_eq!(alert.source, source);

        // Every close of the generated feed is above zero, so the alert fires once.
        let fired = app.alerts.log().len();
        for _ in 0..1_000 {
            super::super::tests::finish_feed(&app.alert_live);
            app.poll_alerts(&ctx);
            if app.alerts.log().len() > fired {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..4 {
            super::super::tests::finish_feed(&app.alert_live);
            app.poll_alerts(&ctx);
        }
        assert_eq!(app.alerts.log().len(), fired + 1);
        assert_eq!(
            Some(&app.alerts.log().back().unwrap().id),
            alert.id.as_ref()
        );

        // The restarted application keeps the alert and the study its condition belongs to.
        let answered = super::super::owndata::store::Saved {
            asked: true,
            ..Default::default()
        };
        answered.save(&app.storage);
        let mut saved = Memory::default();
        app.save(&mut saved);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&saved);
        let restored = DemoApp::new(&cc);
        let restored_alerts = restored.alert_host.engine().alerts();
        let kept = restored_alerts.iter().find(|kept| kept.id == alert.id);
        assert_eq!(kept.map(|kept| &kept.source), Some(&source));
        assert!(restored.indicator_dialog.factory().has("Positive close"));
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

        // Examples opens a bundled script, drawn with its declarations coloured apart from names.
        let (title, example) = egui_charts_scripting::examples()[1];
        click(&mut app, &ctx, "Examples");
        click(&mut app, &ctx, title);
        assert_eq!(app.sample_study.editor.text(), example);
        let shapes = frame(&mut app, &ctx, Vec::new());
        let drawn = (shapes.iter())
            .find(|shape| shape.galley.text() == example)
            .expect("the example is drawn");
        let colour_at = |word: &str| {
            let at = example.find(word).unwrap();
            let mut sections = drawn.galley.job.sections.iter();
            sections
                .find(|section| {
                    (section.byte_range.start.0..section.byte_range.end.0).contains(&at)
                })
                .map(|section| section.format.color)
        };
        assert_ne!(colour_at("input fast"), colour_at("fast ="));
    }
}

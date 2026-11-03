//! A local sample document and a deliberately small host expression evaluator.

use super::DemoApp;
use egui_charts::studies::{CustomIndicator, IndicatorValue, InputKind, InputValue, StudyInput};
use egui_charts::ui::study_editor::{Diagnostic, Severity, StudyEditor, StudyEditorAction};

const TITLE: &str = "Sample close difference";
const SOURCE: &str = "close - close[10]";
const STORAGE: &str = "Documents are kept locally with this demo. Apply calculates against the active chart's bars, including live updates.";
const SYNTAX: &str = "Sample syntax: close - close[N], where N is an integer from 1 to 10000 bars. Edit N and Apply. This sample supports only this expression.";

pub(super) struct SampleStudyEditor {
    pub open: bool,
    editor: StudyEditor,
    choosing: bool,
    message: String,
}

impl Default for SampleStudyEditor {
    fn default() -> Self {
        let mut editor = StudyEditor::new();
        editor.open_document(TITLE, SOURCE);
        Self {
            open: false,
            editor,
            choosing: false,
            message: String::new(),
        }
    }
}

fn compile(source: &str, color: egui::Color32) -> Result<CustomIndicator, String> {
    let lookback = source
        .trim()
        .strip_prefix("close - close[")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|number| number.parse::<usize>().ok())
        .filter(|lookback| (1..=10_000).contains(lookback))
        .ok_or_else(|| "Expected close - close[N], with N from 1 to 10000".to_owned())?;
    CustomIndicator::new(TITLE, move |bars| difference(bars, lookback))
        .with_overlay(false)
        .with_color(color)
        .with_inputs(
            vec![StudyInput::new(
                "lookback",
                "Lookback bars",
                InputValue::Length(lookback),
                InputKind::Length(1, 10_000),
            )],
            |bars, inputs| {
                let InputValue::Length(lookback) = inputs[0].value else {
                    return Vec::new();
                };
                difference(bars, lookback)
            },
        )
        .map_err(|error| error.to_string())
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

impl DemoApp {
    pub(super) fn show_sample_study(&mut self, ui: &mut egui::Ui) {
        if !self.sample_study.open {
            return;
        }
        let mut action = StudyEditorAction::None;
        let mut load = None;
        egui::Panel::bottom("sample_study_editor")
            .resizable(true)
            .default_size(250.0)
            .size_range(180.0..=450.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Sample study editor");
                    if ui.button("Reload saved").clicked() {
                        load = Some(self.sample_study.editor.title().to_owned());
                    }
                    if ui.button("Close editor").clicked() {
                        self.sample_study.open = false;
                    }
                });
                ui.label(SYNTAX);
                ui.label(STORAGE);
                if !self.sample_study.message.is_empty() {
                    ui.label(&self.sample_study.message);
                }
                if self.sample_study.choosing {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Open local document:");
                        if ui.button("Built-in sample").clicked() {
                            self.sample_study.editor.open_document(TITLE, SOURCE);
                            self.sample_study.choosing = false;
                        }
                        for name in self.storage.sample_studies() {
                            if ui.button(&name).clicked() {
                                load = Some(name);
                            }
                        }
                        if ui.button("Cancel open").clicked() {
                            self.sample_study.choosing = false;
                        }
                    });
                }
                action = self.sample_study.editor.show(ui);
            });
        if let Some(name) = load {
            match self.storage.sample_study(&name) {
                Some(text) => {
                    self.sample_study.editor.open_document(name, text);
                    self.sample_study.choosing = false;
                    self.sample_study.message.clear();
                }
                None => {
                    self.sample_study.message =
                        "Save this document locally before reloading it.".into()
                }
            }
        }
        self.handle_sample_study(action);
    }

    fn handle_sample_study(&mut self, action: StudyEditorAction) {
        match action {
            StudyEditorAction::Open => self.sample_study.choosing = true,
            StudyEditorAction::Save { title, text } => {
                if title.trim().is_empty() {
                    self.sample_study.message = "Enter a document title before saving.".into();
                } else {
                    self.storage.save_sample_study(&title, &text);
                    self.sample_study.editor.mark_saved(&text);
                    self.sample_study.message = format!("Saved locally: {title}");
                }
            }
            StudyEditorAction::Apply { text, .. } => match compile(&text, self.theme.ui.accent) {
                Ok(study) => {
                    self.chart.user_action(|chart| {
                        let previous: Vec<_> = chart
                            .indicators()
                            .filter(|(_, study)| study.name() == TITLE)
                            .map(|(id, _)| id)
                            .collect();
                        for id in previous {
                            chart.remove_series(id);
                        }
                        let inputs = super::study_input_summary(&study);
                        let id = chart.add_indicator(Box::new(study));
                        chart.set_indicator_inputs(id, inputs);
                    });
                    self.sample_study.editor.set_diagnostics(Vec::new());
                    self.sample_study.editor.apply_result(Ok(()));
                }
                Err(message) => {
                    self.sample_study.editor.set_diagnostics(vec![Diagnostic {
                        severity: Severity::Error,
                        message: message.clone(),
                        span: 0..text.len(),
                    }]);
                    self.sample_study.editor.apply_result(Err(message));
                }
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DemoApp;
    use eframe::App;
    use egui::{Event, Key, Modifiers, Pos2, RawInput};
    use egui_charts::model::{Bar, BarData};
    use egui_charts::studies::InputValue;
    use egui_charts::widget::SeriesId;

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

    fn render(app: &mut DemoApp, ctx: &egui::Context, events: Vec<Event>) -> Vec<(String, Pos2)> {
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
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_owned(),
                    text.pos + text.galley.size() / 2.0,
                )),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
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

    #[test]
    fn editor_applies_live_inputs_preserves_invalid_edits_and_reopens_local_documents() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        app.live.set_paused(true);
        app.execute_search_command(&ctx, "sample-study");
        assert!(
            render(&mut app, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text == "Sample study editor")
        );
        let times: Vec<_> = (0..3)
            .map(|i| chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i))
            .collect();
        let bars = [100.0, 103.0, 101.0]
            .into_iter()
            .zip(&times)
            .map(|(close, &time)| Bar::from_value(time, close))
            .collect();
        app.chart = egui_charts::Chart::new(BarData::from_bars(bars));
        edit(&mut app, &ctx, "close - close[10]", "close - close[1]");
        click(&mut app, &ctx, "Apply");
        let id = app
            .chart
            .indicators()
            .find(|(_, study)| study.name() == "Sample close difference")
            .unwrap()
            .0;
        let values = app
            .chart
            .indicators()
            .find(|(candidate, _)| *candidate == id)
            .unwrap()
            .1
            .values();
        assert_eq!(
            [values.value(0, 0), values.value(0, 1), values.value(0, 2)],
            [None, Some(3.0), Some(-2.0)]
        );
        edit(&mut app, &ctx, "close - close[1]", "close - close[0]");
        click(&mut app, &ctx, "Apply");
        let text = render(&mut app, &ctx, Vec::new());
        assert!(
            text.iter()
                .any(|(text, _)| text.contains("Expected close - close[N]"))
        );
        assert!(text.iter().any(|(text, _)| text == "close - close[0]"));
        assert_eq!(
            app.chart
                .indicators()
                .find(|(_, study)| study.name() == "Sample close difference")
                .unwrap()
                .0,
            id
        );
        app.chart
            .indicator_mut(id)
            .unwrap()
            .set_input("lookback", InputValue::Length(2))
            .unwrap();
        render(&mut app, &ctx, Vec::new());
        assert_eq!(
            app.chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .values()
                .value(0, 2),
            Some(1.0)
        );
        app.chart
            .indicator_mut(id)
            .unwrap()
            .set_input("lookback", InputValue::Length(1))
            .unwrap();
        app.chart
            .update(SeriesId::MAIN, Bar::from_value(times[2], 108.0));
        render(&mut app, &ctx, Vec::new());
        assert_eq!(
            app.chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1
                .values()
                .value(0, 2),
            Some(5.0)
        );
        edit(&mut app, &ctx, "close - close[0]", "close - close[2]");
        click(&mut app, &ctx, "Save");
        edit(&mut app, &ctx, "close - close[2]", "close - close[1]");
        click(&mut app, &ctx, "Reload saved");
        assert!(
            render(&mut app, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text == "close - close[2]")
        );
        let mut saved = Memory::default();
        app.save(&mut saved);
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(&saved);
        let mut restored = DemoApp::new(&cc);
        restored.live.set_paused(true);
        restored.execute_search_command(&ctx, "sample-study");
        click(&mut restored, &ctx, "Open");
        click(&mut restored, &ctx, "Sample close difference");
        assert!(
            render(&mut restored, &ctx, Vec::new())
                .iter()
                .any(|(text, _)| text == "close - close[2]")
        );
    }
}

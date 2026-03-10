//! Pine Script editor dialog implementation.
//!
//! Provides a code text area, compile/run button, output panel, and error display
//! with basic syntax highlighting for the Pine-like scripting language.

use egui::{Context, RichText, ScrollArea, TextEdit, Vec2};

use crate::ext::UiExt;
use crate::scripting::{Lexer, ParseError, Parser, Runtime};
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::{DialogFrame, dialog_header};

/// Action returned by the Pine editor dialog
#[derive(Debug, Clone, PartialEq)]
pub enum PineEditorAction {
    /// No action
    None,
    /// Dialog was closed
    Close,
    /// Script was compiled and executed successfully
    Executed,
}

/// Compilation/execution result for display
#[derive(Debug, Clone)]
enum EditorResult {
    /// No result yet
    Empty,
    /// Parse error with line/column information
    ParseError {
        message: String,
        line: usize,
        column: usize,
    },
    /// Runtime error during execution
    RuntimeError(String),
    /// Successful execution with output lines
    Success(Vec<String>),
}

/// Pine Script editor dialog
pub struct PineEditor {
    /// Whether the dialog is currently open
    pub is_open: bool,
    /// Source code in the editor
    code: String,
    /// Last compilation/execution result
    result: EditorResult,
    /// Whether the output panel is expanded
    output_expanded: bool,
}

impl Default for PineEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl PineEditor {
    pub fn new() -> Self {
        Self {
            is_open: false,
            code: Self::default_script().to_string(),
            result: EditorResult::Empty,
            output_expanded: true,
        }
    }

    /// Default example script shown when the editor opens
    fn default_script() -> &'static str {
        r#"//@version=5
indicator("My Script", overlay=true)

length = 20
sma_val = ta.sma(close, length)
plot(sma_val)"#
    }

    /// Open the dialog
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Show the dialog. Returns the action taken.
    pub fn show(&mut self, ctx: &Context) -> PineEditorAction {
        if !self.is_open {
            return PineEditorAction::None;
        }

        let mut action = PineEditorAction::None;

        DialogFrame::new(
            "Pine Script Editor",
            Vec2::new(
                DESIGN_TOKENS.sizing.dialog.default_width.max(560.0),
                DESIGN_TOKENS.sizing.dialog.default_height.max(480.0),
            ),
        )
        .show(ctx, |ui| {
            action = self.render_contents(ui);
        });

        // Close on Escape
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.is_open = false;
            action = PineEditorAction::Close;
        }

        action
    }

    fn render_contents(&mut self, ui: &mut egui::Ui) -> PineEditorAction {
        let mut action = PineEditorAction::None;

        // Title bar
        if dialog_header(ui, "Pine Script Editor") {
            action = PineEditorAction::Close;
            self.is_open = false;
        }
        ui.separator();

        // Toolbar with compile/run button
        ui.space_sm();
        ui.horizontal(|ui| {
            ui.space_lg();
            if ui
                .button(RichText::new("Compile & Run").size(typography::MD))
                .clicked()
            {
                self.compile_and_run();
                if matches!(self.result, EditorResult::Success(_)) {
                    action = PineEditorAction::Executed;
                }
            }
            ui.space_md();
            if ui
                .button(RichText::new("Clear").size(typography::MD))
                .clicked()
            {
                self.result = EditorResult::Empty;
            }
            ui.space_md();
            if ui
                .button(RichText::new("Reset").size(typography::MD))
                .clicked()
            {
                self.code = Self::default_script().to_string();
                self.result = EditorResult::Empty;
            }
        });
        ui.space_sm();
        ui.separator();

        // Code editor area
        ui.space_sm();

        let editor_height = if self.output_expanded {
            DESIGN_TOKENS.sizing.dialog.default_height.max(480.0) * 0.5
        } else {
            DESIGN_TOKENS.sizing.dialog.default_height.max(480.0) * 0.75
        };

        ScrollArea::vertical()
            .id_salt("pine_code_area")
            .max_height(editor_height)
            .show(ui, |ui| {
                self.render_code_editor(ui);
            });

        ui.separator();

        // Output/error panel
        self.render_output_panel(ui);

        action
    }

    /// Render the code editor with basic syntax highlighting via layouter
    fn render_code_editor(&mut self, ui: &mut egui::Ui) {
        let text_color = ui.style().visuals.text_color();
        let keyword_color = ui.style().visuals.hyperlink_color;
        let string_color = ui.style().visuals.warn_fg_color;
        let number_color = ui.style().visuals.widgets.active.fg_stroke.color;
        let comment_color = ui.style().visuals.weak_text_color();
        let builtin_color = keyword_color;

        let font_size = typography::SM_MD;

        let mut layouter = move |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            let text_str = text.as_str();
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = wrap_width;

            // Tokenize for syntax highlighting
            highlight_pine_source(
                text_str,
                &mut job,
                ui,
                font_size,
                text_color,
                keyword_color,
                string_color,
                number_color,
                comment_color,
                builtin_color,
            );

            ui.painter().layout_job(job)
        };

        let editor = TextEdit::multiline(&mut self.code)
            .font(egui::FontId::monospace(font_size))
            .desired_width(f32::INFINITY)
            .min_size(Vec2::new(
                DESIGN_TOKENS.sizing.dialog.default_width.max(560.0)
                    - DESIGN_TOKENS.spacing.xl * 2.0,
                200.0,
            ))
            .layouter(&mut layouter);

        ui.add(editor);
    }

    /// Render the output/error panel
    fn render_output_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.space_lg();
            let toggle_label = if self.output_expanded {
                "Output [-]"
            } else {
                "Output [+]"
            };
            if ui
                .add(
                    egui::Label::new(
                        RichText::new(toggle_label)
                            .size(typography::MD)
                            .strong()
                            .color(ui.style().visuals.text_color()),
                    )
                    .sense(egui::Sense::click()),
                )
                .clicked()
            {
                self.output_expanded = !self.output_expanded;
            }
        });

        if !self.output_expanded {
            return;
        }

        ui.space_xs();

        let output_height = DESIGN_TOKENS.sizing.dialog.default_height.max(480.0) * 0.25;

        ScrollArea::vertical()
            .id_salt("pine_output_area")
            .max_height(output_height)
            .show(ui, |ui| {
                ui.space_xs();
                match &self.result {
                    EditorResult::Empty => {
                        ui.horizontal(|ui| {
                            ui.space_lg();
                            ui.label(
                                RichText::new("Press 'Compile & Run' to execute your script.")
                                    .size(typography::SM)
                                    .color(ui.style().visuals.weak_text_color()),
                            );
                        });
                    }
                    EditorResult::ParseError {
                        message,
                        line,
                        column,
                    } => {
                        ui.horizontal(|ui| {
                            ui.space_lg();
                            ui.label(
                                RichText::new(format!("Parse Error at line {line}, col {column}:"))
                                    .size(typography::SM)
                                    .strong()
                                    .color(ui.style().visuals.error_fg_color),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.space_xl();
                            ui.label(
                                RichText::new(message)
                                    .size(typography::SM)
                                    .color(ui.style().visuals.error_fg_color),
                            );
                        });
                    }
                    EditorResult::RuntimeError(msg) => {
                        ui.horizontal(|ui| {
                            ui.space_lg();
                            ui.label(
                                RichText::new("Runtime Error:")
                                    .size(typography::SM)
                                    .strong()
                                    .color(ui.style().visuals.error_fg_color),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.space_xl();
                            ui.label(
                                RichText::new(msg)
                                    .size(typography::SM)
                                    .color(ui.style().visuals.error_fg_color),
                            );
                        });
                    }
                    EditorResult::Success(lines) => {
                        for line in lines {
                            ui.horizontal(|ui| {
                                ui.space_lg();
                                ui.label(
                                    RichText::new(line)
                                        .size(typography::SM)
                                        .color(ui.style().visuals.text_color()),
                                );
                            });
                        }
                    }
                }
                ui.space_sm();
            });
    }

    /// Compile and run the current script
    fn compile_and_run(&mut self) {
        // Step 1: Parse
        let lexer = Lexer::new(&self.code);
        let mut parser = Parser::new(lexer);
        let program = match parser.parse() {
            Ok(p) => p,
            Err(e) => {
                let (line, col) = extract_parse_error_location(&e);
                self.result = EditorResult::ParseError {
                    message: format!("{e}"),
                    line,
                    column: col,
                };
                return;
            }
        };

        // Step 2: Execute with an empty bar set (validation mode)
        let mut runtime = Runtime::new();
        crate::scripting::register_builtins(&mut runtime);

        // Execute the script to verify it runs
        match runtime.execute(&self.code) {
            Ok(()) => {
                let mut output = Vec::new();
                output.push("Script compiled and executed successfully.".to_string());

                // Report number of statements
                output.push(format!("Statements: {}", program.statements.len()));

                // Report plots
                let plots = runtime.get_plots();
                if !plots.is_empty() {
                    output.push(format!("Plots: {}", plots.len()));
                    for plot in plots {
                        output.push(format!("  - {} ({} values)", plot.name, plot.values.len()));
                    }
                }

                // Report strategy results
                let strategy = runtime.get_strategy_state();
                if strategy.metrics.total_trades > 0 {
                    output.push("Strategy Results:".to_string());
                    output.push(format!("  Total trades: {}", strategy.metrics.total_trades));
                    output.push(format!(
                        "  Win rate: {:.1}%",
                        strategy.metrics.win_rate * 100.0
                    ));
                    output.push(format!("  Net profit: {:.2}", strategy.metrics.net_profit));
                    output.push(format!(
                        "  Max drawdown: {:.2}",
                        strategy.metrics.max_drawdown
                    ));
                }

                self.result = EditorResult::Success(output);
            }
            Err(e) => {
                self.result = EditorResult::RuntimeError(format!("{e}"));
            }
        }
    }
}

/// Extract line/column from a ParseError (best-effort)
fn extract_parse_error_location(err: &ParseError) -> (usize, usize) {
    match err {
        ParseError::UnexpectedToken { line, column, .. } => (*line, *column),
        ParseError::UnexpectedEof => (0, 0),
        ParseError::InvalidSyntax(_) => (0, 0),
    }
}

/// Basic Pine Script syntax highlighting applied to a LayoutJob
fn highlight_pine_source(
    text: &str,
    job: &mut egui::text::LayoutJob,
    _ui: &egui::Ui,
    font_size: f32,
    text_color: egui::Color32,
    keyword_color: egui::Color32,
    string_color: egui::Color32,
    number_color: egui::Color32,
    comment_color: egui::Color32,
    builtin_color: egui::Color32,
) {
    let font_id = egui::FontId::monospace(font_size);

    let mut chars = text.char_indices().peekable();

    while let Some(&(start_byte, ch)) = chars.peek() {
        // Comments: // to end of line
        if ch == '/' {
            let mut lookahead = chars.clone();
            lookahead.next();
            if let Some(&(_, '/')) = lookahead.peek() {
                // Consume to end of line
                let mut end_byte = start_byte;
                while let Some(&(idx, c)) = chars.peek() {
                    end_byte = idx + c.len_utf8();
                    chars.next();
                    if c == '\n' {
                        break;
                    }
                }
                job.append(
                    &text[start_byte..end_byte],
                    0.0,
                    egui::text::TextFormat {
                        font_id: font_id.clone(),
                        color: comment_color,
                        ..Default::default()
                    },
                );
                continue;
            }
        }

        // String literals
        if ch == '"' || ch == '\'' {
            let quote = ch;
            chars.next(); // skip opening quote
            let mut end_byte = start_byte + ch.len_utf8();
            let mut escaped = false;
            while let Some(&(idx, c)) = chars.peek() {
                end_byte = idx + c.len_utf8();
                chars.next();
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                    continue;
                }
                if c == quote {
                    break;
                }
            }
            job.append(
                &text[start_byte..end_byte],
                0.0,
                egui::text::TextFormat {
                    font_id: font_id.clone(),
                    color: string_color,
                    ..Default::default()
                },
            );
            continue;
        }

        // Numbers
        if ch.is_ascii_digit()
            || (ch == '.'
                && chars
                    .clone()
                    .nth(1)
                    .is_some_and(|(_, c)| c.is_ascii_digit()))
        {
            let mut end_byte = start_byte;
            while let Some(&(idx, c)) = chars.peek() {
                if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
                    // Allow +/- only after e/E
                    if (c == '+' || c == '-') && end_byte > start_byte {
                        let prev = text.as_bytes()[end_byte - 1];
                        if prev != b'e' && prev != b'E' {
                            break;
                        }
                    }
                    end_byte = idx + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            job.append(
                &text[start_byte..end_byte],
                0.0,
                egui::text::TextFormat {
                    font_id: font_id.clone(),
                    color: number_color,
                    ..Default::default()
                },
            );
            continue;
        }

        // Identifiers and keywords
        if ch.is_alphabetic() || ch == '_' {
            let mut end_byte = start_byte;
            while let Some(&(idx, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '_' || c == '.' {
                    end_byte = idx + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &text[start_byte..end_byte];
            let color = classify_identifier(word, keyword_color, builtin_color, text_color);
            job.append(
                word,
                0.0,
                egui::text::TextFormat {
                    font_id: font_id.clone(),
                    color,
                    ..Default::default()
                },
            );
            continue;
        }

        // Everything else (operators, whitespace, punctuation)
        chars.next();
        let end_byte = start_byte + ch.len_utf8();
        job.append(
            &text[start_byte..end_byte],
            0.0,
            egui::text::TextFormat {
                font_id: font_id.clone(),
                color: text_color,
                ..Default::default()
            },
        );
    }
}

/// Classify an identifier token for syntax highlighting
fn classify_identifier(
    word: &str,
    keyword_color: egui::Color32,
    builtin_color: egui::Color32,
    default_color: egui::Color32,
) -> egui::Color32 {
    // Keywords
    match word {
        "if" | "else" | "for" | "while" | "var" | "varip" | "true" | "false" | "and" | "or"
        | "not" | "import" | "export" | "return" | "switch" | "break" | "continue" => {
            return keyword_color;
        }
        _ => {}
    }

    // Built-in variables
    match word {
        "open" | "high" | "low" | "close" | "volume" | "time" | "hl2" | "hlc3" | "ohlc4"
        | "bar_index" | "na" | "nz" => {
            return builtin_color;
        }
        _ => {}
    }

    // Namespaced builtins (ta.*, math.*, str.*, strategy.*, input.*, array.*, color.*)
    if word.starts_with("ta.")
        || word.starts_with("math.")
        || word.starts_with("str.")
        || word.starts_with("strategy.")
        || word.starts_with("input.")
        || word.starts_with("array.")
        || word.starts_with("color.")
    {
        return builtin_color;
    }

    // Top-level functions
    match word {
        "indicator" | "strategy" | "plot" | "plotshape" | "plotchar" | "plotarrow" | "bgcolor"
        | "fill" | "hline" | "alertcondition" | "fixnan" => {
            return builtin_color;
        }
        _ => {}
    }

    default_color
}

//! The names a script can use, for documentation and completion. The crate's names come from the
//! tables the engine registers and the compile check reads, and a registry's studies are read as a
//! `study` call resolves them, so what the reference lists is what a script compiles with.

use crate::rhai::engine::{self, Group, HELPERS};
use crate::rhai::syntax::{self, Collector, DECLARATIONS, KINDS, NA, PRICES, Token};
use crate::runtime::bindings;
use crate::runtime::series::SeriesRef;
use ::rhai::Engine;
use egui_charts::studies::{
    Indicator, IndicatorFactory, InputKind, InputValue, StudyInput, StudySource,
};
use std::collections::HashSet;
use std::fmt::Write;
use std::sync::{Arc, Mutex};

/// Every name a script can use, for documentation and completion: the declarations, the input
/// kinds, the series and the helpers of this crate, and the studies a script can call of the
/// registry it was built from, each with its inputs and outputs as a call writes them.
#[derive(Clone, Debug)]
pub struct Reference {
    entries: Vec<ReferenceEntry>,
}

/// One name a script can use.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ReferenceEntry {
    /// The name: a keyword, an input kind, a series or helper name, a registry name, an input's
    /// id or an output's title.
    pub name: String,
    /// What the name is.
    pub kind: EntryKind,
    /// The heading it is listed under: a helper's group such as "Streaming", a study's category
    /// such as "Momentum"; empty otherwise.
    pub group: String,
    /// How a script writes it, such as `sma(x, n)`, `study("RSI")`, `period = 14` or
    /// `output = "RSI"`.
    pub signature: String,
    /// What it is or gives.
    pub summary: String,
    /// For a study, its inputs and then its outputs; empty otherwise.
    pub items: Vec<ReferenceEntry>,
}

/// What a [`ReferenceEntry`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryKind {
    /// `display`, `input`, `state`, `series`, `bars`, `plot`, `fill` or `study`.
    Declaration,
    /// A kind of `input`, such as `length`.
    InputKind,
    /// A price series such as `close`, or `time` and `bar_index`.
    Series,
    /// A helper such as `sma`, or the missing value `na`.
    Helper,
    /// A study of the registry.
    Study,
    /// An input of a study, given as `ID = EXPR` in its call.
    Input,
    /// An output of a study, read with `output = TITLE`.
    Output,
}

impl EntryKind {
    /// The name the JSON form gives the kind.
    fn key(self) -> &'static str {
        match self {
            Self::Declaration => "declaration",
            Self::InputKind => "input_kind",
            Self::Series => "series",
            Self::Helper => "helper",
            Self::Study => "study",
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}

impl ReferenceEntry {
    fn new(
        name: &str,
        kind: EntryKind,
        group: &str,
        signature: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            kind,
            group: group.to_owned(),
            signature: signature.into(),
            summary: summary.into(),
            items: Vec::new(),
        }
    }
}

impl Reference {
    /// The reference for scripts compiled against `factory`: its studies as
    /// [`Script::compile`](crate::Script::compile) resolves them, at the defaults they declare,
    /// without those a script can never call.
    pub fn new(factory: &IndicatorFactory) -> Self {
        let mut entries: Vec<_> = DECLARATIONS
            .iter()
            .map(|declaration| {
                let summary = (declaration.summary)();
                let kind = EntryKind::Declaration;
                ReferenceEntry::new(declaration.name, kind, "", declaration.signature, summary)
            })
            .collect();
        entries.extend(KINDS.iter().map(|(name, _, signature, summary)| {
            ReferenceEntry::new(name, EntryKind::InputKind, "", *signature, *summary)
        }));
        entries.extend(PRICES.iter().map(|(name, _, summary)| {
            ReferenceEntry::new(name, EntryKind::Series, "", *name, *summary)
        }));
        let missing = Group::Missing.title();
        entries.push(ReferenceEntry::new(
            NA.0,
            EntryKind::Helper,
            missing,
            NA.0,
            NA.1,
        ));
        entries.extend(HELPERS.iter().map(|helper| {
            let group = helper.group.title();
            ReferenceEntry::new(
                helper.name,
                EntryKind::Helper,
                group,
                helper.signature,
                helper.summary,
            )
        }));
        // The parser of a script's first pass, which decides the ids a call writes bare.
        let mut parser = engine::engine();
        syntax::register(&mut parser, &Arc::new(Mutex::new(Collector::new(None))));
        let mut names = factory.list();
        names.sort();
        entries.extend(
            names
                .iter()
                .filter_map(|name| study(factory, name, &parser)),
        );
        Self { entries }
    }

    /// The entries: declarations, input kinds, series and helpers in the order the reference
    /// lists them, then the studies by name.
    pub fn entries(&self) -> &[ReferenceEntry] {
        &self.entries
    }

    /// The reference as one Markdown document.
    pub fn to_markdown(&self) -> String {
        let mut blocks = vec![
            "# Script reference".to_owned(),
            "The names a script can use, generated from this crate's tables and from the studies \
             of the registry the reference was built for. The crate's guide describes the language."
                .to_owned(),
        ];
        let studies = self
            .entries
            .iter()
            .position(|entry| entry.kind == EntryKind::Study);
        let (names, studies) = self.entries.split_at(studies.unwrap_or(self.entries.len()));
        let mut section = None;
        for chunk in names.chunk_by(|a, b| a.kind == b.kind && a.group == b.group) {
            let kind = chunk[0].kind;
            if section != Some(kind) {
                let title = match kind {
                    EntryKind::Declaration => "Declarations",
                    EntryKind::InputKind => "Input kinds",
                    EntryKind::Series => "Series",
                    _ => "Helpers",
                };
                blocks.push(format!("## {title}"));
                section = Some(kind);
            }
            if !chunk[0].group.is_empty() {
                blocks.push(format!("### {}", prose(&chunk[0].group)));
            }
            blocks.push(list(chunk));
        }
        blocks.push("## Studies".to_owned());
        blocks.push(if studies.is_empty() {
            "The registry has no study a script can call.".to_owned()
        } else {
            "Studies a script can never call are left out: those that calculate on their own or \
             have no line, and those that need histories or order flow a script cannot supply \
             whatever inputs a call gives."
                .to_owned()
        });
        for study in studies {
            blocks.push(format!("### {}", prose(&study.name)));
            blocks.push(format!(
                "{}, {}. {}",
                code(&study.signature),
                prose(&study.group),
                prose(&study.summary)
            ));
            let outputs = study
                .items
                .iter()
                .position(|item| item.kind == EntryKind::Output);
            let (inputs, outputs) = study.items.split_at(outputs.unwrap_or(study.items.len()));
            for (title, items) in [("Inputs:", inputs), ("Outputs:", outputs)] {
                if !items.is_empty() {
                    blocks.push(title.to_owned());
                    blocks.push(list(items));
                }
            }
        }
        blocks.join("\n\n") + "\n"
    }

    /// The entries as JSON: `{"format":1,"entries":[...]}`, each entry an object with the keys
    /// `name`, `kind`, `group`, `signature`, `summary` and `items`, in that order. `format` changes
    /// only when a key changes its meaning or goes away.
    pub fn to_json(&self) -> String {
        let mut json = String::from("{\"format\":1,\"entries\":");
        entries_json(&self.entries, &mut json);
        json.push('}');
        json
    }

    /// The entries that complete the word before byte `cursor` of `text`: names of this crate,
    /// the input kinds after `input NAME =`, registry names inside the string of `study("`, and
    /// inside a `study` call the ids of its inputs and, after `output = "`, the titles of its
    /// outputs. Empty in comments, in other strings and where no word ends at the cursor.
    pub fn complete(&self, text: &str, cursor: usize) -> Vec<ReferenceEntry> {
        let mut end = cursor.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let prefix = &text[..end];
        let tokens: Vec<_> = syntax::tokens(prefix)
            .into_iter()
            .map(|(token, _)| token)
            .collect();
        let Some((&last, before)) = tokens.split_last() else {
            return Vec::new();
        };
        let start = |piece: &str| piece.as_ptr() as usize - prefix.as_ptr() as usize;
        let back = |n: usize| before.len().checked_sub(n).map(|at| before[at]);
        let call = self.open_call(before);
        let named = |kinds: &[EntryKind], typed: &str| -> Vec<&ReferenceEntry> {
            let entries = self.entries.iter();
            let found = entries.filter(|entry| kinds.contains(&entry.kind));
            found
                .filter(|entry| entry.name.starts_with(typed))
                .collect()
        };
        let item = |kind: EntryKind, typed: &str| -> Vec<&ReferenceEntry> {
            let items = call.into_iter().flat_map(|study| &study.items);
            let found = items.filter(|item| item.kind == kind);
            found.filter(|item| item.name.starts_with(typed)).collect()
        };
        let found = match last {
            Token::Word(typed)
                if start(typed) + typed.len() == end
                    && !typed.starts_with(|c: char| c.is_ascii_digit()) =>
            {
                // A range `a..b` ends in a `.` too.
                let method =
                    back(1) == Some(Token::Symbol('.')) && back(2) != Some(Token::Symbol('.'));
                let kind = (back(3), back(2), back(1));
                if method {
                    Vec::new()
                } else if call.is_some() && back(1) == Some(Token::Symbol(',')) {
                    item(EntryKind::Input, typed)
                } else if matches!(
                    kind,
                    (
                        Some(Token::Word("input")),
                        Some(Token::Word(_)),
                        Some(Token::Symbol('='))
                    )
                ) {
                    named(&[EntryKind::InputKind], typed)
                } else {
                    let kinds = [EntryKind::Declaration, EntryKind::Series, EntryKind::Helper];
                    named(&kinds, typed)
                }
            }
            Token::Text(typed)
                if start(typed) + typed.len() == end && prefix[..start(typed)].ends_with('"') =>
            {
                if (back(2), back(1)) == (Some(Token::Word("study")), Some(Token::Symbol('('))) {
                    named(&[EntryKind::Study], typed)
                } else if (back(2), back(1))
                    == (Some(Token::Word("output")), Some(Token::Symbol('=')))
                {
                    item(EntryKind::Output, typed)
                } else if back(1) == Some(Token::Symbol(',')) {
                    item(EntryKind::Input, typed)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        // The forms of one helper, such as `nz(x)` and `nz(x, y)`, complete one name.
        let mut seen = HashSet::new();
        found
            .into_iter()
            .filter(|entry| seen.insert(&entry.name))
            .cloned()
            .collect()
    }

    /// The study of the innermost call open at the end of `tokens`: the first `(` back from the
    /// end that no `)` closes, when it opens `study("NAME", ...` with more after the name.
    fn open_call(&self, tokens: &[Token]) -> Option<&ReferenceEntry> {
        let mut depth = 0;
        for at in (0..tokens.len()).rev() {
            match tokens[at] {
                Token::Symbol(')') => depth += 1,
                Token::Symbol('(') if depth > 0 => depth -= 1,
                Token::Symbol('(') => {
                    let Some(&Token::Text(name)) = tokens.get(at + 1) else {
                        return None;
                    };
                    let called = at > 0 && tokens[at - 1] == Token::Word("study");
                    let study = |entry: &&ReferenceEntry| {
                        entry.kind == EntryKind::Study && entry.name == name
                    };
                    return (called && at + 2 < tokens.len())
                        .then(|| self.entries.iter().find(study))
                        .flatten();
                }
                _ => {}
            }
        }
        None
    }
}

/// The entry of study `name` of `factory` as a call resolves it, with the ids `parser` takes as
/// names written bare, or `None` for a study a script can never call.
fn study(factory: &IndicatorFactory, name: &str, parser: &Engine) -> Option<ReferenceEntry> {
    let prototype = bindings::prototype(factory, name)?;
    let study = prototype.lock();
    if bindings::unreadable(name, &**study).is_some() {
        return None;
    }
    let placement = if study.is_overlay() {
        "Drawn over the prices."
    } else {
        "Drawn in a pane of its own."
    };
    let mut summary = match study.desc().trim() {
        "" => placement.to_owned(),
        desc if desc.ends_with('.') => format!("{desc} {placement}"),
        desc => format!("{desc}. {placement}"),
    };
    let id = |id: &str| {
        // A bare `output` names the line a call reads.
        let word = id.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && id.chars().all(|c| c.is_alphanumeric() || c == '_');
        if word && id != "output" && parser.compile(format!("let {id} = 0;")).is_ok() {
            id.to_owned()
        } else {
            format!("{id:?}")
        }
    };
    let given = |input: &StudyInput, value: &InputValue| match written(&input.kind, value) {
        Some(value) => format!("{} = {value}", id(&input.id)),
        None => id(&input.id),
    };
    if let Some(refusal) = bindings::unsupplied(name, &**study) {
        let (input, value) = needed(name, &**study)?;
        let _ = write!(
            summary,
            " A call gives {}, since with the defaults {refusal}.",
            given(&input, &value)
        );
    }
    let mut entry = ReferenceEntry::new(
        name,
        EntryKind::Study,
        factory.category(name).unwrap_or_default(),
        format!("study({name:?})"),
        summary,
    );
    let inputs = study.inputs();
    for (at, input) in inputs.iter().enumerate() {
        // The chart's own input follows a study's of the same id, and is the one a call sets.
        if inputs[at + 1..].iter().any(|later| later.id == input.id) {
            continue;
        }
        let columns = input.id == "source" && study.source().is_some();
        let takes = bindings::takes(&input.kind, columns);
        let summary = format!("{}: {takes}.", input.label);
        let signature = given(input, &input.value);
        let item = ReferenceEntry::new(&input.id, EntryKind::Input, "", signature, summary);
        entry.items.push(item);
    }
    let names = study.line_names();
    for (at, (title, shown)) in bindings::titles(&names).iter().zip(&names).enumerate() {
        let mut summary = format!(
            "Line {}, shown as {shown:?} with the default inputs.",
            at + 1
        );
        if at == 0 {
            summary.push_str(" A call without output reads it.");
        }
        let signature = format!("output = {title:?}");
        let item = ReferenceEntry::new(title, EntryKind::Output, "", signature, summary);
        entry.items.push(item);
    }
    Some(entry)
}

/// An input that, given in a call, lets `study` run on a script's bars where its defaults do not,
/// with the value it takes there, or `None`. One input changes at a time, to each value a script
/// gives that is no number: the other of a checkbox, every other choice, and no text, symbol or
/// interval.
fn needed(name: &str, study: &dyn Indicator) -> Option<(StudyInput, InputValue)> {
    for input in study.inputs() {
        let values = match (&input.kind, &input.value) {
            (InputKind::Toggle, InputValue::Toggle(on)) => vec![InputValue::Toggle(!on)],
            (InputKind::Choice(labels), InputValue::Choice(chosen)) => (0..labels.len())
                .filter(|at| at != chosen)
                .map(InputValue::Choice)
                .collect(),
            (InputKind::Text, InputValue::Text(text)) if !text.is_empty() => {
                vec![InputValue::Text(String::new())]
            }
            (InputKind::Symbol, InputValue::Symbol(text)) if !text.is_empty() => {
                vec![InputValue::Symbol(String::new())]
            }
            (InputKind::Timeframe, InputValue::Timeframe(Some(_))) => {
                vec![InputValue::Timeframe(None)]
            }
            _ => Vec::new(),
        };
        for value in values {
            let mut changed = study.clone_box();
            if changed.set_input(&input.id, value.clone()).is_ok()
                && bindings::unsupplied(name, &*changed).is_none()
            {
                return Some((input, value));
            }
        }
    }
    None
}

/// How a script writes `value` for an input of `kind`, or `None` for one a script cannot give.
fn written(kind: &InputKind, value: &InputValue) -> Option<String> {
    Some(match (kind, value) {
        (InputKind::Length(..), InputValue::Length(value)) => value.to_string(),
        (InputKind::Integer(..), InputValue::Integer(value)) => value.to_string(),
        (InputKind::Number(..), InputValue::Number(value)) if value.is_finite() => {
            format!("{value:?}")
        }
        (InputKind::Number(..), InputValue::Number(_)) => NA.0.to_owned(),
        (InputKind::Toggle, InputValue::Toggle(value)) => value.to_string(),
        (InputKind::Choice(labels), InputValue::Choice(at)) => format!("{:?}", labels.get(*at)?),
        (InputKind::Text, InputValue::Text(text))
        | (InputKind::Symbol, InputValue::Symbol(text)) => format!("{text:?}"),
        (InputKind::Timeframe, InputValue::Timeframe(interval)) => {
            format!(
                "{:?}",
                interval.map_or_else(String::new, |interval| interval.to_string())
            )
        }
        (InputKind::Source, InputValue::Source(StudySource::Price(price))) => {
            let series = SeriesRef::Price(*price);
            let found = PRICES.iter().find(|(_, price, _)| *price == series);
            found?.0.to_owned()
        }
        _ => return None,
    })
}

/// `entries` as Markdown list items, each its signature and summary.
fn list(entries: &[ReferenceEntry]) -> String {
    let items = entries
        .iter()
        .map(|entry| format!("- {}: {}", code(&entry.signature), prose(&entry.summary)));
    items.collect::<Vec<_>>().join("\n")
}

/// `text` as a Markdown code span, its fence longer than any run of backticks in it.
fn code(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// `text` as Markdown prose: on one line, with each character that would format it escaped.
fn prose(text: &str) -> String {
    let text = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\`*_[]<>|#~".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Write `entries` to `json` as an array of objects.
fn entries_json(entries: &[ReferenceEntry], json: &mut String) {
    json.push('[');
    for (at, entry) in entries.iter().enumerate() {
        json.push_str(if at == 0 { "{" } else { ",{" });
        let fields = [
            ("name", entry.name.as_str()),
            ("kind", entry.kind.key()),
            ("group", entry.group.as_str()),
            ("signature", entry.signature.as_str()),
            ("summary", entry.summary.as_str()),
        ];
        for (key, value) in fields {
            let _ = write!(json, "\"{key}\":");
            string_json(value, json);
            json.push(',');
        }
        json.push_str("\"items\":");
        entries_json(&entry.items, json);
        json.push('}');
    }
    json.push(']');
}

/// Write `text` to `json` as a string: quotes, backslashes and control characters escaped.
fn string_json(text: &str, json: &mut String) {
    json.push('"');
    for c in text.chars() {
        match c {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(json, "\\u{:04x}", u32::from(c));
            }
            c => json.push(c),
        }
    }
    json.push('"');
}

//! Host-supplied UI translations, scoped to an egui context.
//!
//! English text from the crate is the lookup key and the fallback. Install a
//! lookup before drawing the UI; replacing it changes the next frame, including
//! already-open dialogs. Missing entries return `None`. The host owns language
//! selection, translation resources and fonts for its chosen languages.
//!
//! ```
//! let ctx = egui::Context::default();
//! egui_charts::localization::set_translator(&ctx, |english| {
//!     match english {
//!         "Cancel" => Some("Abbrechen".to_owned()),
//!         _ => None,
//!     }
//! });
//! ```

use egui::{Context, Id};
use std::sync::Arc;

type Lookup = dyn Fn(&str) -> Option<String> + Send + Sync;

#[derive(Clone)]
struct Translator(Arc<Lookup>);

fn translator_id() -> Id {
    Id::NULL.with("egui_charts::localization")
}

/// Install or replace this context's translation lookup and request a repaint.
///
/// Keys are the original English labels, including punctuation and case.
/// Templates passed to [`format_text`] use braces, such as `"Level {0} visible"`;
/// translations may reorder or repeat those placeholders. Return `None` to
/// retain the English text. Lookups run outside egui's context locks.
pub fn set_translator(
    ctx: &Context,
    lookup: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
) {
    ctx.data_mut(|data| data.insert_temp(translator_id(), Translator(Arc::new(lookup))));
    ctx.request_repaint();
}

/// Restore this context's built-in English UI and request a repaint.
pub fn clear_translator(ctx: &Context) {
    ctx.data_mut(|data| data.remove::<Translator>(translator_id()));
    ctx.request_repaint();
}

/// Resolve a UI label through the host lookup, falling back to its English text.
///
/// This also lets hosts use the same lookup in their own controls. Translation
/// changes presentation only; keep identifiers and stored values in their
/// original form. Numeric and date formatting use the chart's formatters.
pub fn text(ctx: &Context, english: impl AsRef<str>) -> String {
    let english = english.as_ref();
    let translator = ctx.data(|data| data.get_temp::<Translator>(translator_id()));
    translator
        .and_then(|lookup| (lookup.0)(english))
        .unwrap_or_else(|| english.to_owned())
}

/// Translate a message template and substitute its named, already-formatted values.
///
/// Unknown placeholders remain literal. Values are inserted once, so braces in
/// a symbol, name or error message cannot become another substitution.
pub fn format_text(ctx: &Context, english: &str, values: &[(&str, String)]) -> String {
    let template = text(ctx, english);
    let mut rest = template.as_str();
    let mut output = String::with_capacity(template.len());
    while let Some(open) = rest.find('{') {
        output.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let key = &rest[1..close];
        if let Some((_, value)) = values.iter().find(|(name, _)| *name == key) {
            output.push_str(value);
        } else {
            output.push_str(&rest[..=close]);
        }
        rest = &rest[close + 1..];
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_lookup_falls_back_switches_and_substitutes_values_once() {
        let ctx = Context::default();
        let other = Context::default();
        assert_eq!(text(&ctx, "Cancel"), "Cancel");
        let read_context = ctx.clone();
        set_translator(&ctx, move |english| {
            // A host lookup may read egui state without reentering a context lock.
            let _ = read_context.global_style();
            match english {
                "Cancel" => Some("Abbrechen".into()),
                "Copy price {price_str}" => Some("{price_str} kopieren ({price_str})".into()),
                "Name {name}, price {price}" => Some("{price}: {name}; {missing}; {".into()),
                "Hidden" => Some(String::new()),
                _ => None,
            }
        });
        assert_eq!(text(&ctx, "Cancel"), "Abbrechen");
        assert_eq!(text(&other, "Cancel"), "Cancel");
        assert_eq!(text(&ctx, "Unknown"), "Unknown");
        assert_eq!(text(&ctx, "Hidden"), "");
        for (english, values, expected) in [
            (
                "Copy price {price_str}",
                vec![("price_str", "12.50".into())],
                "12.50 kopieren (12.50)",
            ),
            (
                "Name {name}, price {price}",
                vec![("name", "{price}".into()), ("price", "12.50".into())],
                "12.50: {price}; {missing}; {",
            ),
            (
                "Missing {name}",
                vec![("name", "TEST".into())],
                "Missing TEST",
            ),
        ] {
            assert_eq!(format_text(&ctx, english, &values), expected);
        }
        set_translator(&ctx, |english| {
            (english == "Cancel").then(|| "Annuler".into())
        });
        assert_eq!(text(&ctx, "Cancel"), "Annuler");
        clear_translator(&ctx);
        assert_eq!(text(&ctx, "Cancel"), "Cancel");
    }
}

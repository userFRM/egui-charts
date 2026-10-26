# Themes and scales

A `theme::Theme` owns UI colours, chart colours, font sizes, spacing and corner radii. Start with `Theme::dark()`, `Theme::light()` or `Theme::high_contrast()`, change public fields, then call `theme::apply_to_egui` at startup and after a change. `Theme::with_accent` derives its interaction shades and readable text colours. With the `serde` feature, hosts can serialise the theme in their own settings storage.

```rust
use egui_charts::theme::{Theme, apply_to_egui};

let ctx = egui::Context::default();
let mut theme = Theme::light();
theme.text = theme.text.scaled(1.25);
theme.spacing.lg = 10.0;
theme.radius.sm = 6.0;
apply_to_egui(&ctx, &theme);
```

The built-in egui text styles follow `theme.text`: Body, Button and Monospace use `body`, Small uses `small`, and Heading uses `heading`. Applying a theme updates those font sizes while retaining each style's font family. UI components use the same scale for their explicit text roles. `TextSizes::scaled` includes chart text roles, small badges (`micro`), markers (`marker`) and dialog titles (`dialog_title`). Old saved themes default missing roles.

Inside a widget, import `theme::ThemeExt` and read `ui.active_theme()` for colours or scales egui does not model. Use `ui.visuals()` for standard widget states. When applying a theme inside an existing root `Ui`, call `ui.set_style(ui.ctx().global_style())` so its children use the new egui style immediately.

Settings dialogs read the theme directly. Shared buttons, lists, forms and dialog padding resolve their default spacing and radii when shown. An explicit `FormGrid::spacing` or component configuration remains a host override. Theme-derived toolbar configurations follow changes field by field while keeping edited fields; configurations supplied through `with_config` keep their values. Fixed component dimensions such as icon and button heights remain in `tokens::DESIGN_TOKENS.sizing`.

`studies::palette` contains literal engine colours for study defaults. Changing UI status colours does not recolour a study. Hosts set study colours through `Indicator::set_colors`, the study's builder, or plot settings. Calculations and existing default colours are unchanged.

Chart theme propagation and chart configuration overrides are handled by the chart API. Use `Chart::set_theme` or `set_theme_override` with the chart implementation available to your host; applying the egui theme alone does not replace explicit chart settings.

## Migration

Read `Theme` directly in place of the removed `theme::components` module and its `DialogStyle`, `SettingsDialogStyle` and `ContextMenuStyle` exports. For example, use `theme.ui.text`, `theme.ui.border_control`, `theme.radius.md` and egui's widget visuals instead of copied component style fields.

Pass `ui` instead of `ui.visuals()` to `StyledPanelFrame::{left, right_widget_bar, right_content, top, bottom, chart}`. The constructors read the applied spacing and radii so the separate `outer_frame` and `inner_frame` methods use the same scales as `show`.

Replace `DESIGN_TOKENS.typography` and `TypographyTokens` with `theme.text`. Replace UI reads of `DESIGN_TOKENS.semantic` with `theme.ui` or `theme.chart`. The unused button, alert, modal and command palette colour groups and their structs have been removed. `UiSemanticTokens` retains only `text_muted_dark` and `border_dark`, which existing engine consumers still require; UI chrome does not read them.

The unused colour token fields below have been removed. Study defaults now come from `studies::palette`; runtime UI colours come from `theme.ui`.

| Token group | Removed fields |
| --- | --- |
| `DESIGN_TOKENS.semantic.indicators` | `sma`, `bb_upper`, `bb_middle`, `bb_lower`, `rsi`, `macd_line`, `macd_signal`, `vwap` |
| `DESIGN_TOKENS.semantic.extended` | `bg_active`, `text_secondary`, `text_muted`, `accent_hover`, `accent_active`, `success_light`, `success_dark`, `success_darker`, `error_dark`, `error_darker`, `warning`, `info_light`, `deep_purple`, `cyan`, `teal`, `indigo`, `pink`, `brown`, `deep_orange` |

When constructing `TextSizes` explicitly, add `micro`, `marker` and `dialog_title`, or use `..Default::default()`. Theme serialisation remains compatible with missing fields. The constants in `styles::typography` and the token spacing/radius defaults remain available for engine code and existing hosts; runtime UI rendering reads the applied scales.

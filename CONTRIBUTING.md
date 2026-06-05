# Contributing to egui-charts

Thanks for your interest in `egui-charts` — a high-performance financial charting engine for [egui](https://docs.rs/egui), with candlesticks, drawing tools, technical indicators, and a full design-token theme system.

This project is actively evolving and genuinely open to pull requests. Whether you are fixing a one-line bug or adding a brand-new chart type, your contribution is welcome. We especially love creative additions: new indicators, drawing tools, chart types, and themes are exactly the kind of thing this engine is built to grow. If you have an idea that makes charts more useful or more beautiful, open an issue and let's talk about it.

The live WebAssembly demo is at <https://userfrm.github.io/egui-charts/>, the API docs are at <https://docs.rs/egui-charts>, and the source lives at <https://github.com/userFRM/egui-charts>.

## Ways to contribute

All of these are valued, and none is too small:

- **Bug reports.** Open an issue with a clear reproduction. The more precise the steps, the faster the fix.
- **Features.** Propose an enhancement to the engine, the widget API, or the demo. Open an issue first so we can agree on the shape before you write code.
- **Documentation.** Doc comments, README clarifications, examples, and rustdoc fixes are all real contributions.
- **Examples.** New entries under `examples/` that show off a workflow help everyone learn the API faster.
- **Creative additions.** This is where the project most wants your imagination — new indicators, drawing tools, chart types, and themes. See below for exactly where each lives and how to add one.

## Creative additions: where things live and how to add one

The engine is designed so that each kind of creative addition has one clear home and one clear extension point. Read a couple of existing siblings before you start — they are the best possible template.

### A new indicator (study)

Built-in indicators live in [`src/studies/builtin/`](src/studies/builtin), one file per indicator (for example `ema.rs`, `rsi.rs`, `bollinger_bands.rs`). Each implements the `Indicator` trait defined in [`src/studies/indicator_trait.rs`](src/studies/indicator_trait.rs).

To add one: create a new file in `src/studies/builtin/`, implement `Indicator` (the core methods are `name`, `calculate`, `values`, `colors`, `set_colors`, `set_visible`, and `clone_box`), give it a `Default` impl with conventional parameters, declare the module, and register the type in `register_builtins!` in [`src/studies/factory.rs`](src/studies/factory.rs) so it is reachable by name through `IndicatorFactory`. For runtime, closure-based indicators without a new struct, see `CustomIndicator` in [`src/studies/custom.rs`](src/studies/custom.rs).

### A new drawing tool

Drawing tools live under [`src/drawings/`](src/drawings). The tool identity is the `DrawingToolType` enum in [`src/drawings/domain/tool_type/`](src/drawings/domain/tool_type), and the rendering for each family is under [`src/drawings/rendering/`](src/drawings/rendering) (for example `fibonacci.rs`, `gann.rs`, `patterns.rs`, `channels.rs`, `elliott.rs`).

To add one: add a variant to `DrawingToolType`, slot it into the right category in `categories.rs`, define its interaction behavior in `behavior.rs` (interaction mode and required points), and implement its rendering in the matching file under `src/drawings/rendering/`.

### A new chart type

Chart types are defined by the `ChartType` enum in [`src/model/chart_type.rs`](src/model/chart_type.rs). Price-transform chart types (Renko, Kagi, Point & Figure, Line Break, range bars, footprint, TPO) have their model logic alongside it in [`src/model/`](src/model) (for example `renko.rs`, `kagi.rs`, `point_figure.rs`, `line_break.rs`, `range_bar.rs`, `footprint.rs`, `tpo.rs`). Rendering and series dispatch live under [`src/chart/series/`](src/chart/series) and [`src/chart/rendering/`](src/chart/rendering).

To add one: add a `ChartType` variant with its metadata, add any price transform under `src/model/`, then wire the variant into the series/rendering dispatch so it draws.

### A new theme

Themes are built from design tokens. The token layer is in [`src/tokens/`](src/tokens) (raw and semantic tokens, plus `design_tokens.ron`), and theme presets live in [`src/theme/`](src/theme) — the `ThemePreset` enum in [`src/theme/presets.rs`](src/theme/presets.rs) selects which light/dark variants to use.

To add one: add a `ThemePreset` variant, include it in `ThemePreset::all()`, and map it to the token variants it should resolve. If you need new colors, add them to the token layer rather than hard-coding values in the preset.

## Development setup

Install the Rust toolchain with [rustup](https://rustup.rs/). The minimum supported Rust version (MSRV) is **1.88**. CI tests against both `1.88.0` and current `stable`.

For the web demo you also need the WebAssembly target and [Trunk](https://trunkrs.dev/):

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

### Running the demo

The demo is a self-contained app under [`examples/demo`](examples/demo).

Natively:

```sh
cargo run --manifest-path examples/demo/Cargo.toml
```

On the web (serves with live reload at a local URL):

```sh
cd examples/demo && trunk serve
```

To produce the release WebAssembly build (the same one that is deployed):

```sh
cd examples/demo && trunk build --release
```

The bundled library examples can be run with the usual cargo invocation, for example `cargo run --example basic_chart`.

## Project layout

A short map of the main modules (see the crate-level docs in [`src/lib.rs`](src/lib.rs) for the full table):

- [`model`](src/model) — domain model: `Bar`, `Symbol`, `Timeframe`, `ChartType`, and the Renko/Kagi/P&F transforms.
- [`data`](src/data.rs) — the `DataSource` trait and data-update abstractions.
- [`chart`](src/chart) — the chart engine: pan/zoom, hit-testing, coordinate mapping, series rendering, interaction.
- [`drawings`](src/drawings) — drawing tools, undo/redo, snapping.
- [`studies`](src/studies) — technical indicators, the `Indicator` trait, and `IndicatorRegistry`.
- [`scales`](src/scales) — price and time scales, tick generators, formatters.
- [`config`](src/config) — chart configuration and options.
- [`validation`](src/validation) — OHLC integrity and data-quality checks.
- [`theme`](src/theme) / [`tokens`](src/tokens) / [`styles`](src/styles) — the design-token theme system.
- [`widget`](src/widget) — the `Chart` egui widget, `ChartBuilder`, and `TradingChart`.
- [`ext`](src/ext) — extension traits (`UiExt`, `ContextExt`, `HasDesignTokens`).
- [`icons`](src/icons) — compile-time embedded SVG icons.

### Feature flags

The default build is the core engine, theme system, chart widget, and compile-time icons.

- `icons` (**default on**) — compile-time embedded SVG icons. Required by `ui`.
- `ui` (off) — application-level UI: toolbars, panels, sidebars, dialogs, and the `ui_kit` widget library they are built on. Enable this when building a full trading-terminal interface around the engine.
- `backtest` (off) — backtesting framework for strategy evaluation on historical data.
- `scripting` (off) — embedded scripting support for user-defined indicators and strategies.

## Contribution workflow

We keep a tight, predictable workflow so `main` is always releasable.

1. **Open or find an issue first.** Discuss the change before writing code so we agree on scope and approach. This is true for features and non-trivial fixes alike.
2. **Branch off `main`.** Never commit directly to `main`. Use a short, descriptive branch name, ideally prefixed by intent, for example `feat/supertrend-indicator` or `fix/rsi-alignment`.
3. **Use [Conventional Commits](https://www.conventionalcommits.org/).** Every commit message and the PR title must follow the convention, for example `feat: add Supertrend indicator` or `fix(studies): align RSI output length with bar count`.
4. **Keep the PR focused.** One logical change per pull request. Smaller PRs are reviewed and merged faster.
5. **Keep it green.** Run the local gate (below) before pushing, and make sure CI is green before requesting a merge. PRs are squash-merged.

## Local gate (run before every push)

CI mirrors these checks exactly. Run them from the repository root and make sure each passes before you push:

```sh
cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings
cargo clippy -- -D warnings
cargo test --all-features --lib
cargo test --doc
cargo build --all-features
cargo build --no-default-features
```

And for the web demo:

```sh
cd examples/demo && trunk build --release
```

CI runs clippy with warnings denied. Its `stable` toolchain is current stable, which is newer than the 1.88 MSRV, so run clippy on current stable to catch the same lints CI will. CI additionally builds the docs with warnings denied (`cargo doc --no-deps --all-features`) and runs a dependency audit (`cargo audit`), so it is worth keeping doc comments warning-clean.

## Code style

- **Idiomatic Rust.** Match the conventions already in the surrounding code; `cargo fmt` is the source of truth for formatting.
- **No dead code.** Delete unused functions, fields, and stubs rather than leaving them behind. Re-add them when they are actually wired up.
- **No new `#[allow(...)]`.** Fix the underlying issue rather than silencing the lint. If an exception seems genuinely necessary, raise it in the PR discussion.
- **Document public APIs.** Every public item should carry a doc comment, and doc examples should compile (`cargo test --doc` runs them).
- **Add tests.** New behavior should come with library tests; bug fixes should come with a test that would have caught the bug.
- **No fabricated metrics.** Any number that appears in docs, comments, or the README must reflect the actual code. If you cannot verify a count or measurement, omit it.

## Project links

- **Issues:** open one before you start — <https://github.com/userFRM/egui-charts/issues>
- **License:** dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), your choice. Contributions are accepted under the same dual license.
- **Live demo:** <https://userfrm.github.io/egui-charts/>
- **API docs:** <https://docs.rs/egui-charts>
- **Changelog:** [CHANGELOG.md](CHANGELOG.md)

By contributing, you agree that your contributions are dual-licensed under MIT and Apache-2.0, matching the rest of the project. Thank you for helping `egui-charts` grow.

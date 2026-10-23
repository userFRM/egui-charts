# Contributing to egui-charts

Thanks for your interest in `egui-charts`, a financial charting engine for [egui](https://docs.rs/egui) with candlesticks, drawing tools, technical indicators, and a design-token theme system.

This project is actively evolving and genuinely open to pull requests. Whether you are fixing a one-line bug or adding a brand-new chart type, your contribution is welcome. We especially love creative additions: new indicators, drawing tools, chart types, and themes are exactly the kind of thing this engine is built to grow. If you have an idea that makes charts more useful or more beautiful, open an issue and let's talk about it.

The live WebAssembly demo is at <https://userfrm.github.io/egui-charts/demo/>, the API docs are at <https://docs.rs/egui-charts>, and the source lives at <https://github.com/userFRM/egui-charts>.

## Ways to contribute

All of these are valued, and none is too small:

- **Bug reports.** Open an issue with a clear reproduction. The more precise the steps, the faster the fix.
- **Features.** Propose an enhancement to the engine, the widget API, or the demo. Open an issue first so we can agree on the shape before you write code.
- **Documentation.** Doc comments, README clarifications, examples, and rustdoc fixes are all real contributions.
- **Examples.** New entries under `examples/` that show off a workflow help everyone learn the API faster.
- **Creative additions.** This is where the project most wants your imagination: new indicators, drawing tools, chart types, and themes. See below for exactly where each lives and how to add one.

## Creative additions: where things live and how to add one

The engine is designed so that each kind of creative addition has one clear home and one clear extension point. Read a couple of existing siblings before you start; they are the best template.

### A new indicator (study)

Built-in indicators live in [`src/studies/builtin/`](src/studies/builtin), one file per indicator (for example `ema.rs`, `rsi.rs`, `bollinger_bands.rs`). Each implements the `Indicator` trait defined in [`src/studies/indicator_trait.rs`](src/studies/indicator_trait.rs).

To add one: create a new file in `src/studies/builtin/`, implement `Indicator` (the core methods are `name`, `calculate`, `values`, `colors`, `set_colors`, `set_visible`, and `clone_box`), give it a `Default` impl with conventional parameters, declare the module, and register the type in `register_builtins!` in [`src/studies/factory.rs`](src/studies/factory.rs) so it is reachable by name through `IndicatorFactory`. For runtime, closure-based indicators without a new struct, see `CustomIndicator` in [`src/studies/custom.rs`](src/studies/custom.rs).

### A new drawing tool

Drawing tools live under [`src/drawings/`](src/drawings). The tool identity is the `DrawingToolType` enum in [`src/drawings/domain/tool_type/`](src/drawings/domain/tool_type), and the rendering for each family is under [`src/drawings/rendering/`](src/drawings/rendering) (for example `fibonacci.rs`, `gann.rs`, `patterns.rs`, `channels.rs`, `elliott.rs`).

To add one: add a variant to `DrawingToolType`, slot it into the right category in `categories.rs`, define its interaction behavior in `behavior.rs` (interaction mode and required points), and implement its rendering in the matching file under `src/drawings/rendering/`.

### A new chart type

Chart types are defined by the `ChartType` enum in [`src/model/chart_type.rs`](src/model/chart_type.rs). Price-transform chart types (Renko, Kagi, Point & Figure, Line Break, range bars, TPO) have their model logic alongside it in [`src/model/`](src/model) (for example `renko.rs`, `kagi.rs`, `point_figure.rs`, `line_break.rs`, `range_bar.rs`, `tpo.rs`). Rendering lives under [`src/chart/rendering/`](src/chart/rendering): the per-type candle, bar, line, area and price-transform renderers are in `src/chart/rendering/candles/`, dispatched by `ChartType` in its `mod.rs`.

To add one: add a `ChartType` variant with its metadata, add any price transform under `src/model/`, then wire the variant into the rendering dispatch in `src/chart/rendering/candles/mod.rs` so it draws.

### A new theme

A theme is a `Theme` value with public fields. The presets are literal `Theme` values in [`src/theme/presets.rs`](src/theme/presets.rs); the token layer they draw on is in [`src/tokens/`](src/tokens): `DESIGN_TOKENS`, a Rust constant holding the spacing, sizing and semantic colour tokens.

To add one: add a `Theme::<name>()` constructor in `src/theme/presets.rs` and add it to `presets()`. The test `theme::tests::text_and_control_outlines_are_readable_in_every_preset` then checks its text and outline contrast.

## Development setup

Install the Rust toolchain with [rustup](https://rustup.rs/). The minimum supported Rust version (MSRV) is **1.95**. CI runs the library and doc tests on Linux, macOS and Windows with current `stable` and on Linux with `1.95.0`.

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

### Measuring frame time

`benches/frame_time.rs` renders the chart headlessly (layout, painting and tessellation through egui, with no window or GPU) on generated sample data, and prints the median frame time and the median tessellation time of each scenario as a Markdown table to paste into a pull request:

```sh
cargo bench --bench frame_time
```

The scenarios are one chart of 5,000,000 bars zoomed out and zoomed in, four charts of 100,000 bars, and eight charts of 100,000 bars with ten studies each, in a 1920 by 1080 window with the pointer over the charts. `FRAME_TIME_FRAMES` sets the number of measured frames (default 300, after 60 warm-up frames), and arguments run only the scenarios whose name contains one of them, for example `cargo bench --bench frame_time -- "zoomed out"`. `benches/bare_chart.rs` times the frame of the `bare_chart` example in the same way.

A second table prints the first frame and the median and 95th percentile of steady, tick and new-bar frames for candles and the five price-based chart types on 1,000,000 one-minute bars; `-- price-based` runs it alone.

`benches/stream.rs` feeds one chart 10,000 trades a second from a thread, with 10 or 30 studies on 100,000 and 1,000,000 bars and frames paced at 60 Hz on the wall clock. It prints frame p50, p99 and max, and full recomputes per second for a fallback study: `cargo bench --bench stream`. `FRAME_TIME_FRAMES` and name filters work as above; warm-up waits for every study to have one value per bar, then runs 60 more frames.

To compare two commits, build the bench on each and run the two binaries in turn, so that changes in machine load and clock speed affect both alike. `cargo bench --no-run` prints the path of the binary it builds:

```sh
git switch --detach <base> && cargo bench --bench frame_time --no-run
cp target/release/deps/frame_time-<hash> /tmp/frame_time-base
git switch <branch> && cargo bench --bench frame_time --no-run
cp target/release/deps/frame_time-<hash> /tmp/frame_time-head
for round in 1 2 3 4 5; do /tmp/frame_time-base; /tmp/frame_time-head; done
```

Compare the medians round by round, and quote the machine and the number of rounds with the figures.

### Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets that look for panics and hangs on hostile input: `bars` feeds appends, live updates, prepends and replacements of bars with non-finite prices and unordered timestamps to a chart and paints a frame, `studies` calculates and updates every study of the catalogue over such a series, and `drawings` loads saved drawing files from arbitrary bytes. They need a nightly toolchain:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run bars -- -max_total_time=120 -timeout=10 -len_control=0 -max_len=8192
mkdir -p fuzz/corpus/drawings
cargo +nightly fuzz run drawings fuzz/corpus/drawings src/drawings/fixtures -- -max_total_time=120 -timeout=10
```

`-len_control=0` lets the inputs grow long enough to hold a series from the start, and `drawings` starts from the saved files in `src/drawings/fixtures/`, since arbitrary bytes rarely form a drawing file.

A crash is saved under `fuzz/artifacts/<target>/`; rerun it with `cargo +nightly fuzz run <target> <file>`. A scheduled workflow (`.github/workflows/fuzz.yml`) runs every target weekly and uploads the crash inputs.

## Project layout

A short map of the main modules (see the crate-level docs in [`src/lib.rs`](src/lib.rs) for the full table):

- [`model`](src/model): domain types (`Bar`, `Symbol`, `Timeframe`, `ChartType`) and the Renko/Kagi/P&F transforms.
- [`data`](src/data): the `Datafeed` contract, trades, depth and bar merging.
- [`chart`](src/chart): rendering, hit-testing, coordinate mapping, overlays.
- [`services`](src/services): traits a host implements for storage, symbol search, signals, alerts and a trading account; nothing calls them yet.
- [`drawings`](src/drawings): drawing tools, selection, handles, undo/redo.
- [`studies`](src/studies): technical indicators, the `Indicator` trait, and `IndicatorRegistry`.
- [`scales`](src/scales): price and time scales, tick generators, formatters.
- [`config`](src/config): chart configuration and options.
- [`theme`](src/theme) / [`tokens`](src/tokens) / [`styles`](src/styles): the design-token theme system.
- [`widget`](src/widget): the `Chart` egui widget and `ChartBuilder`, with pan and zoom, keyboard and drawing input, indicator panes and loading from a `Datafeed`.
- [`ext`](src/ext): extension traits (`UiExt`, `ResponseExt`, widget builders), with the `ui` feature.
- [`icons`](src/icons): compile-time embedded SVG icons.

### Feature flags

The default build includes the chart widget, the icons, the built-in studies and the application-level UI components. An application that only shows charts uses `default-features = false, features = ["widget"]`. With `default-features = false` alone the crate is the headless engine (`model`, `scales`, `data`, `services`, `tokens`, `validation` and the `Indicator` trait layer of `studies`), which does not depend on egui.

- `widget` (**default on**): the egui chart widget and the modules built on it (`chart`, `drawings`, `config`, `theme`, `styles`, `widget`). Required by `icons` and `ui`.
- `studies` (**default on**): the built-in studies and `IndicatorFactory`. Needs no egui, so a headless build can compute the same studies. Required by `ui`.
- `icons` (**default on**): compile-time embedded SVG icons. Required by `ui`.
- `ui` (**default on**): application-level UI (toolbars, panels, sidebars, dialogs), the `ui_kit` widget library it is built on, and the `ext` helpers. Enables `icons` and `studies`.
- `serde` (off): `Serialize` and `Deserialize` for `theme::Theme`.

## Contribution workflow

We keep a tight, predictable workflow so `main` is always releasable.

1. **Open or find an issue first.** Discuss the change before writing code so we agree on scope and approach. This is true for features and non-trivial fixes alike.
2. **Branch off `main`.** Never commit directly to `main`. Use a short, descriptive branch name, ideally prefixed by intent, for example `feat/supertrend-indicator` or `fix/rsi-alignment`.
3. **Use [Conventional Commits](https://www.conventionalcommits.org/).** Every commit message and the PR title must follow the convention, for example `feat: add Supertrend indicator` or `fix(studies): align RSI output length with bar count`.
4. **Keep the PR focused.** One logical change per pull request. Smaller PRs are reviewed and merged faster.
5. **Keep it green.** Run the local gate (below) before pushing, and make sure CI is green before requesting a merge. PRs are squash-merged.

## Local gate (run before every push)

CI runs these checks, with `RUSTFLAGS=-Dwarnings` set in `ci.yml`. Run them from the repository root and make sure each passes before you push:

```sh
cargo fmt --all -- --check
cargo build
cargo build --no-default-features
cargo build --no-default-features --features widget
cargo build --no-default-features --features studies
cargo build --all-features
cargo build --examples
cargo clippy --all-targets -- -D warnings
cargo clippy --all-features --all-targets -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings
cargo clippy --no-default-features --features widget --all-targets -- -D warnings
cargo clippy --no-default-features --features studies --all-targets -- -D warnings
cargo test --all-features --lib
cargo test --doc --all-features
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --all-features
```

And for the web demo:

```sh
cd examples/demo && trunk build --release
```

CI runs clippy with warnings denied. Its `stable` toolchain is current stable, which is newer than the 1.95 MSRV, so run clippy on current stable to catch the same lints CI will. It also checks the dependency tree and runs `cargo audit`.

## Code style

- **Idiomatic Rust.** Match the conventions already in the surrounding code; `cargo fmt` is the source of truth for formatting.
- **No dead code.** Delete unused functions, fields, and stubs rather than leaving them behind. Re-add them when they are actually wired up.
- **No new `#[allow(...)]`.** Fix the underlying issue rather than silencing the lint. If an exception seems genuinely necessary, raise it in the PR discussion.
- **Document public APIs.** Every public item should carry a doc comment, and doc examples should compile (`cargo test --doc` runs them).
- **Add tests.** New behavior should come with library tests; bug fixes should come with a test that would have caught the bug.
- **No fabricated metrics.** Any number that appears in docs, comments, or the README must reflect the actual code. If you cannot verify a count or measurement, omit it.

## Project links

- **Issues:** open one before you start: <https://github.com/userFRM/egui-charts/issues>
- **License:** dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), your choice. Contributions are accepted under the same dual license.
- **Live demo:** <https://userfrm.github.io/egui-charts/demo/>
- **API docs:** <https://docs.rs/egui-charts>
- **Changelog:** [CHANGELOG.md](CHANGELOG.md)

By contributing, you agree that your contributions are dual-licensed under MIT and Apache-2.0, matching the rest of the project. Thank you for helping `egui-charts` grow.

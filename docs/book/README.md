# Building the documentation

Use the exact mdBook release pinned by `MDBOOK_VERSION` in
`.github/workflows/deploy-demo.yml`. The Pages workflow installs its prebuilt binary.
From the repository root:

```sh
python3 docs/book/build.py
RUSTUP_TOOLCHAIN=stable cargo test --doc --all-features
RUSTUP_TOOLCHAIN=stable RUSTDOCFLAGS="-Dwarnings --extend-css docs/book/theme/api.css" cargo doc --no-deps --all-features
cp -r target/doc target/book/api
python3 -m http.server --directory target/book
```

Open `http://localhost:8000`. The book build removes unused theme menu items from
mdBook's output so its keyboard navigation visits only Light and Dark. Themes
follow the system preference until the reader chooses one. Navigation and search
use mdBook's bundled scripts; no external assets or services are loaded.

Keep existing `docs/*.md` guides in place and include them from chapter files.
When a chapter or an included guide carries Rust examples, add a `cfg(doctest)`
`include_str!` in `src/lib.rs`, following `ReadmeDoctests`, and keep that source
available in the package's `exclude` exceptions. The builder guide is already
compiled through `guide`. Use `no_run` only for examples that cannot run headlessly.

The workflow assembles the book at `/egui-charts/docs/` and rustdoc at
`/egui-charts/docs/api/`, with a redirect from the API root to the crate index.
It preserves the existing root and `/demo/` pages. Capture previews outside the
repository; images from the chart demo do not belong in this book.

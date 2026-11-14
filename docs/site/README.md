# Documentation site

This is an Astro site using Starlight, with exact dependencies and a committed npm
lockfile. Use Node 24, matching `.github/workflows/deploy-demo.yml`.

```sh
cd docs/site
npm ci
npm run build
```

The output is `docs/site/dist/`, with base `/egui-charts/docs/`. For live editing,
run `npm run dev`. After editing a shared guide, restart it to reload that source.

Author site pages in `src/content/docs/*.mdx`. Shared `docs/*.md` guides and the
root `CHANGELOG.md` stay in place: `prepare-guides.mjs` adds front matter in ignored
build inputs. `guides.mjs` maps their routes, repository-relative sources and edit
paths; generator tests run with the build. Edit links always point at the original
source on `main`. `remark-docs.mjs` converts relative source links to site routes
and hides Rust documentation setup lines. Source links still work on GitHub.

Keep every Rust-bearing page in a `cfg(doctest)` `include_str!` in `src/lib.rs`,
following the existing pattern, and keep those sources available in Cargo's
package excludes. `cargo test --doc --all-features` checks them. Only examples
that open native windows use `no_run`.

`npm run build` also builds rustdoc with all features into `target/doc`, then
`check-api-links.mjs` fails on any local guide or API page or anchor that does not
resolve, so the build needs a Rust toolchain. Link each option, type or method a
page discusses to its rustdoc member under `/egui-charts/docs/api/`. The Pages build
sets `RUSTDOCFLAGS=-Dwarnings` and repeats the check on the assembled `_site/docs`.

The workflow's assembly step combines the guide, API and demo in `_site/`.
To preview the production URL prefix locally, serve a temporary parent containing
an `egui-charts` symlink to `_site`:

```sh
preview_root=$(mktemp -d)
ln -s "$PWD/_site" "$preview_root/egui-charts"
python3 -m http.server --bind 127.0.0.1 --directory "$preview_root"
```

Open `http://localhost:8000/egui-charts/docs/`.
Starlight provides system theme detection, its theme picker and Pagefind search.
All assets are local.

## Running guide examples

`examples/docs/` is an unpublished widget-only application, separate from the
full demo. Build it with the same pinned Trunk version as Pages:

```sh
trunk build --release --locked --public-url /egui-charts/docs/examples/widget/ --config examples/docs/Trunk.toml
```

Pages serves it at `/egui-charts/docs/examples/widget/?scene=first-chart`.
The scene uses fixed synthetic bars and needs no data service.

Measured on 2026-11-02 with Trunk 0.21.14, Rust 1.99.1, the committed lockfile and
the command above: the optimized WebAssembly file is **4,763,509 bytes**
(4.54 MiB), or **1,980,772 bytes** (1.89 MiB) with gzip level 9. These sizes
exclude the JavaScript loader; gzip is a transfer-size measurement, and actual
compression depends on the server. The build uses the `wasm-release` profile
and `wasm-opt -Oz` declared in `examples/docs/`.

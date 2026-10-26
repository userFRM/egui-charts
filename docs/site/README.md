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

Author site pages in `src/content/docs/*.mdx`. The seven shared `docs/*.md` guides
stay in place: `prepare-guides.mjs` adds front matter in ignored build inputs.
`guides.mjs` maps their sources and routes. Edit links always point at the original
source on `main`. `remark-docs.mjs` converts relative source links to site routes
and hides Rust documentation setup lines. Source links still work on GitHub.

Keep every Rust-bearing page in a `cfg(doctest)` `include_str!` in `src/lib.rs`,
following the existing pattern, and keep those sources available in Cargo's
package excludes. `cargo test --doc --all-features` checks them. Only examples
that open native windows use `no_run`.

The Pages build also builds rustdoc with all features. From the repository root:

```sh
RUSTUP_TOOLCHAIN=stable RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --all-features
```

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

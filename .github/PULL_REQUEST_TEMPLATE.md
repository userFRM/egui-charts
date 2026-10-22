## Summary

Describe what this PR changes and why. Keep it focused: one logical change per PR.

## Related issue

Closes #

## Type of change

- [ ] fix: bug fix
- [ ] feat: new feature
- [ ] docs: documentation only
- [ ] refactor: no behavior change
- [ ] test: adds or fixes tests
- [ ] chore: tooling, CI, deps

## How tested

Describe how you verified the change (native and/or web demo, examples run, tests added).

## Checklist

- [ ] PR title follows Conventional Commits (e.g. `feat: add ATR indicator`)
- [ ] Ran the CI gate locally on stable with `RUSTFLAGS=-Dwarnings`: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo clippy --no-default-features --all-targets -- -D warnings`, `cargo build`, `cargo build --no-default-features`, `cargo build --all-features`, `cargo test --all-features --lib`, `cargo test --doc --all-features`, and `RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --all-features`
- [ ] Clippy also passes on the MSRV: `cargo +1.88 clippy --all-features --all-targets -- -D warnings`
- [ ] If the web demo is affected: `cd examples/demo && trunk build --release` succeeds
- [ ] No new dead code and no new `#[allow(...)]`
- [ ] Docs and CHANGELOG updated where relevant
- [ ] One focused change; unrelated edits split into separate PRs

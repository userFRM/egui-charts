## Summary

Describe what this PR changes and why. Keep it focused — one logical change per PR.

## Related issue

Closes #

## Type of change

- [ ] fix — bug fix
- [ ] feat — new feature
- [ ] docs — documentation only
- [ ] refactor — no behavior change
- [ ] test — adds or fixes tests
- [ ] chore — tooling, CI, deps

## How tested

Describe how you verified the change (native and/or web demo, examples run, tests added).

## Checklist

- [ ] PR title follows Conventional Commits (e.g. `feat: add ATR indicator`)
- [ ] Ran the local pre-push gate on stable: `cargo fmt --all -- --check`, `cargo clippy --all-features -- -D warnings`, `cargo clippy -- -D warnings`, `cargo test --all-features --lib`, `cargo test --doc`, `cargo build --all-features`, `cargo build --no-default-features`
- [ ] If the web demo is affected: `cd examples/demo && trunk build --release` succeeds
- [ ] No new dead code and no new `#[allow(...)]`
- [ ] Docs and CHANGELOG updated where relevant
- [ ] One focused change — unrelated edits split into separate PRs

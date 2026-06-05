# Security Policy

Thanks for helping keep egui-charts and the people who use it safe. This document explains which versions receive security fixes, how to report a vulnerability privately, and what to expect afterward.

## Supported versions

egui-charts is in its initial `0.x` development series, and security fixes are applied to the latest published release on that line.

| Version | Supported |
| ------- | --------- |
| 0.2.x   | Yes       |
| < 0.2   | No        |

While the crate is pre-1.0, the API may evolve between minor releases, so the most reliable way to stay current with security fixes is to track the newest `0.x` release on [crates.io](https://crates.io/crates/egui-charts).

## Reporting a vulnerability

Please report security issues privately rather than opening a public issue or pull request, so the problem can be assessed and fixed before details are widely known.

Use GitHub's private vulnerability reporting: go to the [Security tab](https://github.com/userFRM/egui-charts/security) of this repository and click **Report a vulnerability** to open a private security advisory. This keeps the report visible only to the maintainers until a fix is ready.

If you are unsure whether something qualifies as a security issue, err on the side of reporting it privately — we would rather take a look than miss a real problem.

## What to include

A clear report helps a small project triage quickly. Where possible, please include:

- A description of the issue and the impact you believe it has.
- The affected version (the crate version, commit, or `Cargo.lock` entry).
- Step-by-step instructions or a minimal example that reproduces the problem.
- Any sample input (for example a CSV file or script) that triggers the behavior.
- Your assessment of severity and any suggested remediation, if you have one.

## What to expect

This is a small, community-maintained project, so responses are made on a best-effort basis rather than against a formal SLA. You can generally expect:

- An acknowledgement that the report was received, typically within a few days.
- An initial assessment of whether the issue is reproducible and in scope.
- Coordination with you on a fix and on timing for any public disclosure.
- Credit in the release notes for the fix, unless you prefer to remain anonymous.

If a report turns out to be a bug without security impact, we may ask you to refile it as a regular public issue so it can be tracked openly.

## Scope and threat model

egui-charts is a client-side charting library: it renders charts inside an egui application and does not run a server, open network listeners, or manage credentials on its own. In practice the most relevant security surface is the parsing and handling of untrusted input — for example CSV data loaded into a chart, or user-supplied scripts when the `scripting` feature is enabled. Reports about crashes, panics, or unexpected behavior triggered by malformed or hostile input of this kind are in scope and welcome.

Dependency vulnerabilities are also relevant. The CI pipeline runs a `cargo audit` job against the [RustSec advisory database](https://rustsec.org/) on every push and pull request, so advisories affecting our dependency tree are surfaced automatically; if you spot one that we have missed, please let us know.

Because the library runs as part of a host application, the overall security of any deployment also depends on how that application sources its data and configures features such as `scripting`. We are happy to discuss hardening guidance for a given integration.

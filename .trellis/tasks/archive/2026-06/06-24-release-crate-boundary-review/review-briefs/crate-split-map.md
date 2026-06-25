# Brief: Crate Split Coupling Map

Active task: .trellis/tasks/06-24-release-crate-boundary-review

You are `review-crates`. Review Cargo/module dependencies and propose a release-oriented crate split map.

Write your final report to `.trellis/tasks/06-24-release-crate-boundary-review/research/05-crate-split-coupling-map.md`.

Scope:

- Inspect workspace `Cargo.toml`, `crates/*/Cargo.toml`, module facades and important `pub use` exports.
- Use `cargo metadata --no-deps --format-version 1` and `rg` to identify dependency direction and application module hotspots.
- Evaluate split candidates: AgentRun application crate/module, Lifecycle application crate/module, RuntimeSession substrate, RuntimeGateway, VFS/resource surface, application ports.
- Recommend batches: boundary facade first, visibility/import cleanup second, physical crate extraction third.
- Include risk, expected compile/test gates and child task candidates.

Use repository evidence and file paths. Do not modify source code.

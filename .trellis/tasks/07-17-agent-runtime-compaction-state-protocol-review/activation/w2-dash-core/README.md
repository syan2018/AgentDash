# W2 Dash Agent / AgentCore activation component

## Identity

- Target base: `9dc0c84b5633f31982aab3f588ca0abbac58a626`
- Physical move commit:
  `265155ea513e576b11897d531fe0279903627e7e`
- Consumer cleanup / inventory commit:
  `e1abec31fd021b23799ccc7b62908548376d14d4`
- Reviewed inventory correction:
  `7fbdd764`
- Source branch: `codex/agent-runtime-s2-dash-activation`
- Source worktree: `F:\Projects\AgentDash-s2-dash-activation`

This directory freezes the W2-owned physical/API component for the S5 hard cut. It is not a
standalone production activation and does not change the current production registry, schema,
migration or Product route.

## Owned result

- `agentdash-agent-core` owns the explicit provider/tool loop, streaming, cancel,
  summarization and Core vocabulary.
- `agentdash-agent` exposes only the Dash Agent middle layer.
- Native is the only direct `agentdash-agent` consumer and only imports Dash APIs.
- True Core execution consumers depend directly on `agentdash-agent-core`.
- Unused `agentdash-agent-types` dependencies and conversions identified by W2 are removed.
- Remaining consumer symbols, final owners and deletion prerequisites are frozen in
  `research/agent-types-hard-cut-consumer-inventory.md`.

## Combined activation prerequisite

The patch series intentionally remains an activation component. The following Wave 4 inputs must
be present on the same frozen revision before it becomes `activation_ready`:

1. W7 removes Product/Application construction of Core tool objects and routes Business Surface,
   Runtime Tool Broker and `AgentHostCallbacks` through the target Runtime path.
2. W7 reads product history/context from Runtime snapshot/change rather than projecting Core
   transcript from presentation journal.
3. W8 removes legacy application-ports/runtime-gateway/SPI/agent-protocol consumers.
4. Dash/Native removes the temporary same-shape serde transcode once the legacy consumer graph is
   gone.
5. W8 deletes `agentdash-agent-types`, its workspace member and lockfile entry after consumer count
   reaches zero.

The combined S5 set must pass:

```powershell
rg -n "agentdash-agent-types" Cargo.toml crates
rg -n "transcode_protocol_owned" crates
cargo metadata --format-version 1
cargo test -p agentdash-agent-core
cargo test -p agentdash-agent
cargo test -p agentdash-integration-native-agent
git diff --check
```

The first two searches must return no production result. Application/Product crates must not gain
a direct dependency on Dash Agent or AgentCore.

## Component evidence

- AgentCore 56 tests, Dash Agent 19 tests and Native 73 tests passed for the physical move.
- `cargo check -p agentdash-contracts -p agentdash-executor --tests --locked` passed for the
  consumer cleanup.
- `cargo metadata --format-version 1 --no-deps --locked` and `git diff --check` passed.
- Independent component review result is recorded in `dispatch-status.md`.

## Patch files

- `0001-refactor-agent-runtime-Dash-Core.patch`
  - SHA-256:
    `107967F0229F1896C966158F6BA0987E0F2C97E0606989E2AEA8AA925AC91A7E`
- `0002-refactor-agent-runtime-Agent.patch`
  - SHA-256:
    `AF0E1E6A0F0BF93B5E9B0C0CCA86011D4899F19D36DCAFA104BFFBD6EE966760`

The series was applied with `git am` to a detached worktree at target base `9dc0c84b`; the resulting
`Cargo.toml`, `Cargo.lock` and `crates/**` tree matched activation tip `e1abec31` exactly. Apply the
patches only in the S5 staging worktree together with the signed W7/W8 components.

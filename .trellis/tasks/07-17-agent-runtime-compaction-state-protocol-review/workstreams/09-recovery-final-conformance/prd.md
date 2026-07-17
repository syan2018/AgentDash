# W9 — Recovery & Final Conformance

## Depends On

- W8 Schema / Crate Hard Cut

## Ownership

- conformance/fault/dependency tests
- final negative gates
- `.trellis/spec/` final updates

W9 不接手未完成的生产实现。

## Goal

在 crash、restart、duplicate、cursor gap、stale generation、unknown external outcome 与
并发条件下证明最终架构收敛，并让 specs、schema、contracts、Rust 和 frontend 只有一套
事实链。

## Exit Criteria

- Native/Codex/Remote service conformance；
- Runtime/PostgreSQL behavior suite；
- Dash history replay/fork/compaction suite；
- AgentRun/Fork/Companion/reconnect E2E；
- initial package fidelity/digest/unknown outcome/first-input ordering conformance；
- Tool/Hook unique route；
- AgentHostCallbacks in-process/remote reverse call、deadline/replay/generation；
- Fork saga 全 durability boundary；
- negative gates 无生产残留；
- specs 与最终实现一致；
- directed gate 与一次 `pnpm check:quick` 通过。

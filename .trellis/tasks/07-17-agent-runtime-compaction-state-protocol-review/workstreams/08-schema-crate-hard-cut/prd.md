# W8 — Schema / Crate Hard Cut

## Depends On

- W2 Dash Agent & AgentCore
- W3 Runtime State & Host Coordination
- W4 Surface / Tool / Hook
- W5 Native / Dash Adapter
- W6 Codex / Remote Adapters
- W7 Product & Protocol Cutover

## Ownership

- workspace Cargo graph/lockfile
- final migration/schema
- W8-owned legacy crate deletions；W2-owned Agent/Core 不再移动
- composition roots
- final generated contracts

## Goal

一次性切换到最终 Product/Runtime/Host/Dash/external projection schema 与 crate DAG，删除
旧 types/protocol/executor/SPI/hooks/journal 路径，不留下兼容 facade 或生产双读写。

## Exit Criteria

- final forward migration 与 constraints 通过；
- W8 是唯一正式 migration owner；
- 旧 crates/interfaces/tables/fields 无生产引用；
- `agentdash-application-runtime-session` 保持缺席，平台 `RuntimeSession*` 语义残留清零；
- Dash Agent/Core/service API/Runtime/Host 物理 DAG 正确；
- Runtime Wire 与 Runtime test support 仅保留共享 framing/codegen、跨 adapter
  conformance 的独立职责，无旧 facade/module；
- 每个保留 crate 有独立边界理由；
- production composition 只组装最终路径。

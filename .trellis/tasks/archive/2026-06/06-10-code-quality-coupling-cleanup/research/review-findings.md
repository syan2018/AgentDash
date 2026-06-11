# Review Findings

## Source

本文件记录 2026-06-10 全仓只读 review 与三个 subagent 结果，用于解释本任务树为什么拆分为快清项与架构追踪项。

## Fast Cleanup Findings

- `crates/agentdash-executor/src/mcp/mod.rs` 无条件引用 optional `agentdash_agent`，导致 `cargo check -p agentdash-executor --no-default-features` 失败。该问题边界集中，适合快速修复。
- `packages/app-web/src/services/story.ts` 手写 `StoryPriority` / `StoryType` 兼容映射，接受非 generated contract 值，并遗漏合法 `"other"`。
- `packages/app-web/src/services/mcpPreset.ts` 将响应作为 `Record<string, unknown>` 重建 DTO，并在缺少 `created_at` / `updated_at` 时伪造当前时间。
- `crates/agentdash-contracts/src/session.rs` 少数 session response DTO 使用 `camelCase`，与项目 HTTP snake_case contract 约定不一致。

## Architecture Tracking Findings

- `agentdash-application` 直接调用 `agentdash_executor::mcp::discover_*`，且 `tool_builder` 与 `TurnPreparationDeps` 中存在重复 MCP tool 构建逻辑。
- `agentdash-application-ports` 的端口签名直接暴露 `agentdash_relay` command/response payload，relay wire shape 穿透 application 边界。
- `agentdash-application` 直接构造 `agentdash-contracts` 前端 wire DTO，application read model 和 HTTP DTO 边界混在一起。
- workflow/canvas 等 API routes 直接返回 domain/application 类型，前端可能依赖内部 serde shape。
- extension manifest 校验在 JS validator、Rust domain、SDK 类型三处已经出现 required 字段和 schema nullability 差异。
- extension dev runtime 以 TS 注册表执行 actions/channels，而安装态 runtime projection 从 JSON manifest 读取，开发态和安装态存在两套事实来源。
- `legacy_machine_ids` 仍贯穿 DB、domain、application、local、Tauri、contracts、frontend generated types，不符合当前预研阶段不保留兼容链路的项目约束。
- workspace tab store 直接 import React render registry，store、UI composition root、全局 singleton 存在 import-order 耦合。
- relay tool payload 仍有 legacy/default 兼容语义，`docs/relay-protocol.md` 也不是当前协议模型的准确描述。

## Task Split Rationale

- 快清项的判断标准：修改范围集中、验收命令明确、不会要求先重新设计跨层数据流。
- tracking 项的判断标准：跨 crate/package、涉及事实源归属或协议边界，需要先写 `design.md` 和 `implement.md` 再开始实现。

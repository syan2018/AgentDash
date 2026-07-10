# 后端规范索引

后端 spec 先读 architecture 主文档，再按模块读取契约附录。

## Architecture Entry

- [Backend Architecture](./architecture.md)

## 通用 Appendices

| 文档 | 说明 |
| --- | --- |
| [Directory Structure](./directory-structure.md) | crate 布局与分层基线 |
| [Repository Pattern](./repository-pattern.md) | aggregate repository 边界 |
| [Database Guidelines](./database-guidelines.md) | PostgreSQL / SQLite / migration 契约 |
| [Error Handling](./error-handling.md) | 分层错误体系与 HTTP 映射 |
| [Domain Payload Typing](./domain-payload-typing.md) | `serde_json::Value` 治理 |
| [Quality Guidelines](./quality-guidelines.md) | 后端编码约定 |
| [Diagnostics Guidelines](./diagnostics-guidelines.md) | 平台过程诊断 `diag!` facade、Subsystem、落地与查询 |
| [Logging Guidelines](./logging-guidelines.md) | 日志级别 / 字段 / 脱敏通用约定 |
| [Runtime Gateway](./runtime-gateway.md) | runtime action 调用边界 |
| [Channel Architecture](./channel.md) | 通信空间、owner-local registry、admission 与 binding provider 契约 |
| [Embedded Skill Bundles](./embedded-skill-bundles.md) | 源码内嵌 skill bundle 契约 |

## 模块 Architecture

| 模块 | 主文档 | Appendices |
| --- | --- | --- |
| session | [Session Architecture](./session/architecture.md) | [startup](./session/session-startup-pipeline.md), [runtime state](./session/runtime-execution-state.md), [agentrun mailbox](./session/agentrun-mailbox.md), [execution frames](./session/execution-context-frames.md), [bundle](./session/bundle-main-datasource.md), [streaming](./session/streaming-protocol.md), [pi-agent streaming](./session/pi-agent-streaming.md), [context compaction projection](./session/context-compaction-projection.md) |
| workflow | [Workflow Architecture](./workflow/architecture.md) | [activity lifecycle](./workflow/activity-lifecycle.md), [lifecycle edge](./workflow/lifecycle-edge.md), [lifecycle run link](./workflow/lifecycle-run-link.md), [story task runtime](./story-task-runtime.md) |
| vfs | [VFS Architecture](./vfs/architecture.md) | [vfs access](./vfs/vfs-access.md), [materialization](./vfs/vfs-materialization.md) |
| hooks | [Hooks Architecture](./hooks/architecture.md) | [execution hook runtime](./hooks/execution-hook-runtime.md), [hook script engine](./hooks/hook-script-engine.md) |
| capability | [Capability Architecture](./capability/architecture.md) | [tool pipeline](./capability/tool-capability-pipeline.md), [dimension pipeline](./capability/capability-dimension-pipeline.md), [LLM model config](./capability/llm-model-config.md), [integration api](./capability/integration-api.md) |
| permission | [Permission Architecture](./permission/architecture.md) | [grant lifecycle](./permission/grant-lifecycle.md), [policy engine](./permission/policy-engine.md) |

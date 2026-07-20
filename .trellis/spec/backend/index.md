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
| [In-Memory Agent Runtime Kernel](./agent-runtime-kernel.md) | 同步 command handoff、Agent read/inspect、live normalize 与重连合同 |
| [Managed Agent Runtime Context](./agent-runtime-context.md) | ContextRecipe/checkpoint/head fidelity与managed compaction activation/recovery合同 |
| [Agent Runtime 持久化权威](./agent-runtime-persistence.md) | Product owner document、concrete Agent authority 与 Runtime/Host 纯内存边界 |
| [Managed Agent Runtime Hook Orchestration](./agent-runtime-hooks.md) | immutable HookPlan、canonical HookRun、failure policy、effect与恢复合同 |
| [Business Agent Surface and Platform Tool Broker](./agent-runtime-surface-tool-broker.md) | capability编译、profile binding与callable tool执行合同 |
| [Integration Complete Agent Host](./agent-runtime-driver-host.md) | service contribution、live attachment、surface、route 与 callback fencing合同 |
| [Dash Complete Agent 与 Clean Agent Core](./agent-runtime-native-adapter.md) | Dash source authority、真实 execution callbacks 与 Clean Core合同 |
| [Codex App Server Runtime Adapter](./agent-runtime-codex-adapter.md) | App Server lifecycle、typed input/interaction、opaque context与native Hook合同 |
| [AgentRun Product / Agent Facade](./agent-runtime-agentrun-facade.md) | 同步 input handoff、Product shell、Agent read/live 组合合同 |
| [Embedded Skill Bundles](./embedded-skill-bundles.md) | 源码内嵌 skill bundle 契约 |

## 模块 Architecture

| 模块 | 主文档 | Appendices |
| --- | --- | --- |
| agent runtime conversation | [Agent Runtime Conversation Architecture](./session/architecture.md) | [runtime kernel](./agent-runtime-kernel.md), [persistence](./agent-runtime-persistence.md), [context](./agent-runtime-context.md), [facade](./agent-runtime-agentrun-facade.md) |
| workflow | [Workflow Architecture](./workflow/architecture.md) | [activity lifecycle](./workflow/activity-lifecycle.md), [lifecycle edge](./workflow/lifecycle-edge.md), [lifecycle run link](./workflow/lifecycle-run-link.md), [story task runtime](./story-task-runtime.md) |
| vfs | [VFS Architecture](./vfs/architecture.md) | [vfs access](./vfs/vfs-access.md), [materialization](./vfs/vfs-materialization.md) |
| hooks | [Hooks Architecture](./hooks/architecture.md) | [execution hook runtime](./hooks/execution-hook-runtime.md), [hook script engine](./hooks/hook-script-engine.md) |
| capability | [Capability Architecture](./capability/architecture.md) | [tool pipeline](./capability/tool-capability-pipeline.md), [dimension pipeline](./capability/capability-dimension-pipeline.md), [LLM model config](./capability/llm-model-config.md), [integration api](./capability/integration-api.md) |
| permission | [Permission Architecture](./permission/architecture.md) | AgentRun facade、RuntimeInteraction 与未来 LifecycleRun-scoped Grant |

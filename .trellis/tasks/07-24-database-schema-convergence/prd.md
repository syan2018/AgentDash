# 数据库表收束与迁移基线压缩

## Goal

在项目首发前删除没有独立业务事实或真实读取链路的 PostgreSQL 表，合并只属于同一 owner 的状态表，并将现有 116 段 migration 压缩为可从空库直接建立最终 schema 的新基线，降低持久化维护成本。

## Background

- 项目尚未上线，不需要保留旧 schema 的兼容读取或渐进升级路径。
- `crates/agentdash-infrastructure/migrations/` 是 PostgreSQL schema 事实源。
- 当前开发库已有 52 张业务表、607 个字段、116 个已成功应用的 migration。
- `pnpm run migration:guard` 当前通过。
- 产品继续保留可搜索、可离线查询的组织 Group directory，因此 `groups` 与 `group_memberships`
  保持现状，不改为只依赖认证 token claims。
- 当前代码审计确认：
  - `views` 没有生产 API 读取或写入链路。
  - `agent_run_control_effects` 没有任何业务代码读写。
  - `agent_run_terminal_projection_outbox` 和 `agent_run_terminal_control_correlation` 只有写入，没有消费者或查询路径。
  - `gate_result_delivery_markers` 的所有操作都以已知 `gate_id` 定位，没有跨 Gate 扫描合同。
  - 两张 Canvas runtime state 表使用相同 owner key，且都只保存 latest state。

## Requirements

- 删除没有真实业务读取者的表及其 Repository、domain contract、装配和前端遗留类型。
- Terminal projection 只保留 current projection、change log 与并发 revision head；删除无消费者的 outbox 和重复 correlation projection。
- Gate result delivery 状态收回 `lifecycle_gates` owner，由 typed document 或等价的明确字段表达，并保留当前幂等 claim、lease 与 completion 语义。
- 将 `agent_run_canvas_runtime_observations` 与 `agent_run_canvas_interaction_snapshots` 合并为一份 AgentRun Canvas state，保留两类状态的独立更新和读取能力。
- 删除只被装配但没有业务调用的 `AgentRunLineageRepository` 抽象；保留由 `AgentRunForkGraphStore` 使用的 `agent_run_lineages` 表及跨 Run 防环合同。
- 用当前最终 schema 重建 migration baseline；baseline 只包含 schema、约束、索引、序列和必要扩展，不包含 runtime 或 seed 数据。
- embedded PostgreSQL 与测试数据库必须从新 baseline 重建，不维护旧 migration checksum 或旧数据目录兼容路径。
- 更新 schema readiness、Repository mapping、测试和数据库规范，使文档只描述最终保留的事实边界。
- 保留当前真实独立事实表，包括 `agent_lineages`、`agent_run_lineages`、`lifecycle_subject_associations`、`routine_executions`、`workflow_executor_effects`、`state_changes`、Workspace/Backend placement 表、`dash_complete_source` 与 `dash_complete_effect`。
- 保留 `groups`、`group_memberships`、`users` 与 `project_subject_grants` 的现有身份目录和授权边界。

## Acceptance Criteria

- [ ] `views`、`agent_run_control_effects`、`agent_run_terminal_projection_outbox`、`agent_run_terminal_control_correlation` 不再存在于代码和最终 schema。
- [ ] Gate result delivery 不再使用独立表，waiter claim、parent continuation claim、lease recovery、幂等 replay 和 completion 行为保持通过测试。
- [ ] 两张旧 Canvas latest-state 表被一份 canonical Canvas state storage 替代，runtime observation 与 interaction snapshot 均可独立 roundtrip。
- [ ] `AgentRunLineageRepository` 冗余装配被删除，AgentRun fork graph 的创建、防环、查询和删除仍通过测试。
- [ ] migration 目录只保留新的首发 baseline，空 PostgreSQL 数据库可一次完成迁移并通过 readiness。
- [ ] migration guard、相关 PostgreSQL integration tests、API/backend 编译检查通过。
- [ ] 工作区内不存在对已删除表名或旧 migration 版本的非历史性引用。
- [ ] 数据库规范和架构描述与最终 PostgreSQL shape 一致。

## Out of Scope

- 不引入旧 schema 兼容层、双写、回退表或数据目录自动升级。
- 不重新设计保留表所属的业务功能。
- 不保留当前开发数据库中的临时 Runtime、会话或测试数据。

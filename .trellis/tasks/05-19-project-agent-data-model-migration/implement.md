# Project Agent 数据结构迁移实施计划

## Pre-Implementation Gate

- [x] 用户确认 `ProjectAgent.id` 继承旧 `agents.id`。
- [x] 本轮仅合并数据结构并保留 `AgentPresetConfig` 类型治理为后续任务。
- [x] 运行 `git status`，确认现有未提交改动与本任务文件无冲突。
- [x] 读取相关 spec：
  - `.trellis/spec/backend/repository-pattern.md`
  - `.trellis/spec/backend/database-guidelines.md`
  - `.trellis/spec/backend/shared-library.md`
  - `.trellis/spec/cross-layer/shared-library-contract.md`
  - `.trellis/spec/guides/cross-layer-thinking-guide.md`

## Phase 1: Domain And Repository

- [x] 将 `crates/agentdash-domain/src/agent/entity.rs` 收敛为 `ProjectAgent`。
- [x] 将 `AgentRepository` / `ProjectAgentLinkRepository` 收敛为 `ProjectAgentRepository`。
- [x] 更新 `crates/agentdash-domain/src/agent/mod.rs` export。
- [x] 删除 `list_by_agent`、`find_by_project_and_agent` 等全局关联接口。
- [x] 在迁移中确保旧 `base_config + config_override` 能形成新 `ProjectAgent.config`。

## Phase 2: Database And Infrastructure

- [x] 新增迁移：创建 `project_agents`，从 `agents + project_agent_links` 合并数据。
- [x] 遵循数据库规范保留已提交 migration，不回改 `0001_init.sql`；最终 schema 由递增迁移 `0044_project_agents.sql` 收敛。
- [x] 更新 `PostgresAgentRepository` 为 `PostgresProjectAgentRepository` 或同等命名。
- [x] 移除运行初始化中的 `agents` / `project_agent_links` 旧表创建和旧补列逻辑。
- [x] 通过递增迁移处理旧 link 知识库、容器、config override 数据到新表。
- [x] 更新 `RepositorySet` 和 `AppState` 注入字段。

## Phase 3: Shared Library

- [x] 更新 `InstallLibraryAssetOutput::ProjectAgent` 为只返回 `project_agent_id`。
- [x] `install_agent_template` 直接创建 `ProjectAgent`。
- [x] `publish_agent_payload` 直接按 `project_agent_id` 读取项目资源。
- [x] `source-status` 从 `ProjectAgent` 读取 installed source。
- [x] 更新 API DTO `shared_library.rs` 和前端 `shared-library.ts` 类型。

## Phase 4: Project Agent API

- [x] 更新 `routes.rs`：从 `/agent-links` 迁移到 `/agents`。
- [x] 重命名 `ProjectAgentLinkResponse`、create/update/delete handler。
- [x] `list_project_agents` / summary route 直接读取 `ProjectAgentRepository`。
- [x] `open_project_agent_session` / `list_project_agent_sessions` 使用 `project_agent_id`。
- [x] 删除解析 `agent_id + link` 的逻辑。
- [x] 更新并通过 route serialization 测试。

## Phase 5: Runtime Consumers

- [x] 更新 `session/construction_planner.rs`。
- [x] 更新 `session/assembler.rs` companion candidate 与 owner bootstrap。
- [x] 更新 `capability/session_workflow_context.rs`。
- [x] 更新 `routine` domain/API/executor，将绑定身份改为 `project_agent_id`。
- [x] 更新 `project_sessions.rs` 的 agent display name 映射。
- [x] 更新相关 mock repository 与单元测试。

## Phase 6: VFS And Inline Files

- [x] `InlineFileOwnerKind::ProjectAgentLink` 改为 `ProjectAgent`。
- [x] owner kind 字符串迁移为 `project_agent`。
- [x] 更新 VFS mount 构建函数参数类型和命名。
- [x] `ResolvedVfsSurfaceSource::ProjectAgentKnowledge` 移除 `link_id`。
- [x] 更新 `vfs_surfaces.rs` 校验逻辑。
- [x] 为旧 inline file owner 数据提供开发库迁移。

## Phase 7: Frontend

- [x] 更新 `packages/app-web/src/types/index.ts`：移除 `AgentEntity` / `ProjectAgentLink`，引入 `ProjectAgent`。
- [x] 更新 `projectStore.ts` API 路径和 store 字段命名。
- [x] 更新 Project Agent 页面、Agent 编辑器、Routine 页面、AssetPickerDrawer。
- [x] 更新 VFS surface source 类型。
- [x] 更新 Shared Library install response 类型和 Marketplace install summary 显示。
- [x] 移除用户可见的 `ProjectAgentLink` 文案。

## Phase 8: Specs And Docs

- [x] 更新 backend shared-library spec，删除 `ProjectAgentLink` 作为 Project 资源的表述。
- [x] 更新 cross-layer shared-library contract 的 API signatures 和 DTO 字段。
- [x] 更新 README 中 Project 与 Agent 关系描述。
- [x] 更新 `AgentPresetConfig` 注释，避免继续传播旧 `Agent + Link` 概念。

## Validation Commands

- [x] `cargo fmt`
- [x] `cargo check`
- [x] `cargo test -p agentdash-domain`
- [x] `cargo test -p agentdash-application`
- [x] `cargo test -p agentdash-api`
- [x] `cargo test -p agentdash-infrastructure`
- [x] `pnpm --filter app-web typecheck`
- [x] `pnpm --filter app-web test`
- [ ] 需要人工验证时使用 `pnpm dev`，Rust 后端更新后先杀掉旧进程再重新启动。

## Review Checklist

- [x] `rg "ProjectAgentLink|project_agent_link|agent-links|agent_link_repo|ProjectAgentLinkRepository"` 只剩任务规划中的历史说明。
- [x] `rg "AgentRepository|struct Agent"` 不再指向旧全局 Agent 领域模型。
- [x] `rg "link_id"` 不再用于 Project Agent knowledge surface。
- [x] `rg "agent_id"` 中与 Project Agent 身份有关的字段均已评估是否应改为 `project_agent_id`；执行器配置内的 `agent_id` 保留。
- [x] 删除 Project Agent 时不会留下知识库 owner；仍被 Routine 使用时拒绝删除。

## Rollback Notes

项目未上线，本任务不设计兼容回退。若实施中发现迁移面过大，应暂停在任务规划阶段拆分子任务，而不是引入旧模型兼容层。

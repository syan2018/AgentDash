# Project Agent 数据结构迁移实施计划

## Pre-Implementation Gate

- [ ] 用户确认 `ProjectAgent.id` 继承旧 `agents.id` 还是旧 `project_agent_links.id`。
- [ ] 用户确认本任务是否一并拆分 `AgentPresetConfig`，还是仅合并数据结构并保留配置类型治理为后续任务。
- [ ] 运行 `git status`，确认现有未提交改动与本任务文件无冲突。
- [ ] 读取相关 spec：
  - `.trellis/spec/backend/repository-pattern.md`
  - `.trellis/spec/backend/database-guidelines.md`
  - `.trellis/spec/backend/shared-library.md`
  - `.trellis/spec/cross-layer/shared-library-contract.md`
  - `.trellis/spec/guides/cross-layer-thinking-guide.md`

## Phase 1: Domain And Repository

- [ ] 将 `crates/agentdash-domain/src/agent/entity.rs` 收敛为 `ProjectAgent`。
- [ ] 将 `AgentRepository` / `ProjectAgentLinkRepository` 收敛为 `ProjectAgentRepository`。
- [ ] 更新 `crates/agentdash-domain/src/agent/mod.rs` export。
- [ ] 删除 `list_by_agent`、`find_by_project_and_agent` 等全局关联接口。
- [ ] 增加配置合并 helper 或迁移 helper，确保旧 `base_config + config_override` 能形成新 `ProjectAgent.config`。

## Phase 2: Database And Infrastructure

- [ ] 新增迁移：创建 `project_agents`，从 `agents + project_agent_links` 合并数据。
- [ ] 同步更新 `0001_init.sql` 的最终 schema。
- [ ] 更新 `PostgresAgentRepository` 为 `PostgresProjectAgentRepository` 或同等命名。
- [ ] 移除初始化中的 `agents` / `project_agent_links` 旧表创建和旧补列逻辑。
- [ ] 处理 `0011_agent_link_knowledge_and_containers.sql`、`0028_agent_config_tool_clusters_to_capability_directives.sql` 中旧表引用。
- [ ] 更新 `RepositorySet` 和 `AppState` 注入字段。

## Phase 3: Shared Library

- [ ] 更新 `InstallLibraryAssetOutput::ProjectAgent` 为只返回 `project_agent_id`。
- [ ] `install_agent_template` 直接创建 `ProjectAgent`。
- [ ] `publish_agent_payload` 直接按 `project_agent_id` 读取项目资源。
- [ ] `source-status` 从 `ProjectAgent` 读取 installed source。
- [ ] 更新 API DTO `shared_library.rs` 和前端 `shared-library.ts` 类型。

## Phase 4: Project Agent API

- [ ] 更新 `routes.rs`：从 `/agent-links` 迁移到 `/agents`。
- [ ] 重命名 `ProjectAgentLinkResponse`、create/update/delete handler。
- [ ] `list_project_agents` / summary route 直接读取 `ProjectAgentRepository`。
- [ ] `open_project_agent_session` / `list_project_agent_sessions` 使用 `project_agent_id`。
- [ ] 删除解析 `agent_id + link` 的逻辑。
- [ ] 补充或更新 route serialization 测试。

## Phase 5: Runtime Consumers

- [ ] 更新 `session/construction_planner.rs`。
- [ ] 更新 `session/assembler.rs` companion candidate 与 owner bootstrap。
- [ ] 更新 `capability/session_workflow_context.rs`。
- [ ] 更新 `routine` domain/API/executor，将绑定身份改为 `project_agent_id`。
- [ ] 更新 `project_sessions.rs` 的 agent display name 映射。
- [ ] 更新相关 mock repository 与单元测试。

## Phase 6: VFS And Inline Files

- [ ] `InlineFileOwnerKind::ProjectAgentLink` 改为 `ProjectAgent`。
- [ ] owner kind 字符串迁移为 `project_agent`。
- [ ] 更新 VFS mount 构建函数参数类型和命名。
- [ ] `ResolvedVfsSurfaceSource::ProjectAgentKnowledge` 移除 `link_id`。
- [ ] 更新 `vfs_surfaces.rs` 校验逻辑。
- [ ] 为旧 inline file owner 数据提供开发库迁移。

## Phase 7: Frontend

- [ ] 更新 `packages/app-web/src/types/index.ts`：移除 `AgentEntity` / `ProjectAgentLink`，引入 `ProjectAgent`。
- [ ] 更新 `projectStore.ts` API 路径和 store 字段命名。
- [ ] 更新 Project Agent 页面、Agent 编辑器、Routine 页面、AssetPickerDrawer。
- [ ] 更新 VFS surface source 类型。
- [ ] 更新 Shared Library install response 类型和 Marketplace install summary 显示。
- [ ] 移除用户可见的 `ProjectAgentLink` 文案。

## Phase 8: Specs And Docs

- [ ] 更新 backend shared-library spec，删除 `ProjectAgentLink` 作为 Project 资源的表述。
- [ ] 更新 cross-layer shared-library contract 的 API signatures 和 DTO 字段。
- [ ] 更新 README 中 Project 与 Agent 关系描述。
- [ ] 如实现中发现可复用约定，更新 repository/database/spec 相关文档。

## Validation Commands

- [ ] `cargo fmt`
- [ ] `cargo test -p agentdash-domain`
- [ ] `cargo test -p agentdash-application`
- [ ] `cargo test -p agentdash-api`
- [ ] `cargo test -p agentdash-infrastructure`
- [ ] `pnpm --filter app-web typecheck`
- [ ] `pnpm --filter app-web test`
- [ ] 需要人工验证时使用 `pnpm dev`，Rust 后端更新后先杀掉旧进程再重新启动。

## Review Checklist

- [ ] `rg "ProjectAgentLink|project_agent_link|agent-links|agent_link_repo|ProjectAgentLinkRepository"` 只剩迁移说明或历史文档中允许保留的记录。
- [ ] `rg "AgentRepository|struct Agent"` 不再指向旧全局 Agent 领域模型。
- [ ] `rg "link_id"` 不再用于 Project Agent knowledge surface。
- [ ] `rg "agent_id"` 中与 Project Agent 身份有关的字段均已评估是否应改为 `project_agent_id`。
- [ ] 删除 Project Agent 时不会留下孤儿配置或知识库 owner。

## Rollback Notes

项目未上线，本任务不设计兼容回退。若实施中发现迁移面过大，应暂停在任务规划阶段拆分子任务，而不是引入旧模型兼容层。

# 95 个重叠路径最终耦合审计

## 审计坐标

- merge-base: `957fa9d6`
- Workspace/Channel source: `7070f6b0`
- Agent Runtime base: `efdfa5dc`
- 重叠路径数: **95**

## Before / After

基线冲突来自两条分支同时修改旧 RuntimeSession、Canvas、AgentFrame surface 与集中装配文件。最终实现以 PR #93 Managed Runtime 为执行事实源，以 Operation、Interaction、Channel、WorkspaceModule 为业务事实源；Runtime 具体类型收敛在 AgentRun adapter 与 API composition，业务核心仅暴露稳定坐标和 port。分类计数：协作/规范 16、集中装配 38、Runtime 接缝 23、并行 cutover 2、业务能力 14、迁移/生成 2。

## 逐路径关闭记录

| # | Path | 根因类别 | Canonical owner | 最终处置 | 复核证据 |
| ---: | --- | --- | --- | --- | --- |
| 1 | `.trellis/spec/backend/architecture.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 2 | `.trellis/spec/backend/capability/architecture.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 3 | `.trellis/spec/backend/capability/tool-capability-pipeline.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 4 | `.trellis/spec/backend/index.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 5 | `.trellis/spec/backend/runtime-gateway.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 6 | `.trellis/spec/backend/session/architecture.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 7 | `.trellis/spec/backend/vfs/architecture.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 8 | `.trellis/spec/backend/vfs/vfs-access.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 9 | `.trellis/spec/backend/workflow/architecture.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 10 | `.trellis/spec/cross-layer/desktop-local-runtime.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 11 | `.trellis/spec/cross-layer/frontend-backend-contracts.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 12 | `.trellis/spec/frontend/architecture.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 13 | `.trellis/spec/frontend/state-management.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 14 | `.trellis/spec/frontend/type-safety.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 15 | `.trellis/workspace/codex-agent/index.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 16 | `.trellis/workspace/codex-agent/journal-2.md` | 协作/规范 | 最终规范与任务记录 | 按最终架构重写或合并 | 存在；依赖扫描 + owning tests |
| 17 | `Cargo.lock` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 18 | `crates/agentdash-agent-protocol/src/backbone/platform.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 19 | `crates/agentdash-agent-protocol/src/lib.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 20 | `crates/agentdash-api/src/agent_run_runtime_surface.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 存在；依赖扫描 + owning tests |
| 21 | `crates/agentdash-api/src/agent_run_terminal_control.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 22 | `crates/agentdash-api/src/app_state.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 23 | `crates/agentdash-api/src/bootstrap/repositories.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 24 | `crates/agentdash-api/src/bootstrap/session.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 25 | `crates/agentdash-api/src/bootstrap/vfs.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 26 | `crates/agentdash-api/src/dto/mod.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 27 | `crates/agentdash-api/src/integrations.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 28 | `crates/agentdash-api/src/lib.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 29 | `crates/agentdash-api/src/relay/registry.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 30 | `crates/agentdash-api/src/relay/ws_handler.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 31 | `crates/agentdash-api/src/routes.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 32 | `crates/agentdash-api/src/routes/canvases.rs` | 并行 cutover | Interaction | 删除 Canvas，保留 shared Interaction 最终模型 | 已删除；依赖扫描 + owning tests |
| 33 | `crates/agentdash-application-agentrun/src/agent_run/frame/mod.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 存在；依赖扫描 + owning tests |
| 34 | `crates/agentdash-application-agentrun/src/agent_run/frame/surface_service.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 存在；依赖扫描 + owning tests |
| 35 | `crates/agentdash-application-agentrun/src/agent_run/mailbox/delivery.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 36 | `crates/agentdash-application-agentrun/src/agent_run/mailbox/mod.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 37 | `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 38 | `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 39 | `crates/agentdash-application-agentrun/src/agent_run/mod.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 存在；依赖扫描 + owning tests |
| 40 | `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 41 | `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 42 | `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 43 | `crates/agentdash-application-lifecycle/src/lifecycle/surface/surface_projector.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 44 | `crates/agentdash-application-ports/Cargo.toml` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 45 | `crates/agentdash-application-ports/src/agent_frame_materialization.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 46 | `crates/agentdash-application-ports/src/lib.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 47 | `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 48 | `crates/agentdash-application-runtime-session/src/session/launch/plan.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 49 | `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 50 | `crates/agentdash-application-runtime-session/src/session/turn_processor.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 51 | `crates/agentdash-application-runtime-session/src/session/turn_supervisor.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 52 | `crates/agentdash-application-workflow/Cargo.toml` | 业务能力 | Operation / Workflow | 搬运 canonical Operation；Runtime 仅 adapter 调用 | 存在；依赖扫描 + owning tests |
| 53 | `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs` | 业务能力 | Operation / Workflow | 搬运 canonical Operation；Runtime 仅 adapter 调用 | 存在；依赖扫描 + owning tests |
| 54 | `crates/agentdash-application/src/canvas/diagnostics.rs` | 并行 cutover | Interaction | 删除 Canvas，保留 shared Interaction 最终模型 | 已删除；依赖扫描 + owning tests |
| 55 | `crates/agentdash-application/src/channel.rs` | 业务能力 | Channel | 搬运 V2 模型；delivery 经 AgentRun facade | 存在；依赖扫描 + owning tests |
| 56 | `crates/agentdash-application/src/companion/tools.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 57 | `crates/agentdash-application/src/frame_construction/composer_project_agent.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 58 | `crates/agentdash-application/src/frame_construction/mod.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 59 | `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 60 | `crates/agentdash-application/src/lib.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 61 | `crates/agentdash-application/src/relay_connector.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 已删除；依赖扫描 + owning tests |
| 62 | `crates/agentdash-application/src/repository_set.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 63 | `crates/agentdash-application/src/runtime_tools/provider.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 存在；依赖扫描 + owning tests |
| 64 | `crates/agentdash-application/src/task/context_builder.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 已删除；依赖扫描 + owning tests |
| 65 | `crates/agentdash-application/src/vfs_owner_providers.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 已删除；依赖扫描 + owning tests |
| 66 | `crates/agentdash-application/src/wait_activity/tests.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 67 | `crates/agentdash-contracts/src/generate_ts.rs` | 迁移/生成 | schema 与 contract composition | 以 0061–0067 final schema 统一生成 | 存在；依赖扫描 + owning tests |
| 68 | `crates/agentdash-contracts/src/project/contract.rs` | 迁移/生成 | schema 与 contract composition | 以 0061–0067 final schema 统一生成 | 存在；依赖扫描 + owning tests |
| 69 | `crates/agentdash-domain/src/channel/mod.rs` | 业务能力 | Channel | 搬运 V2 模型；delivery 经 AgentRun facade | 存在；依赖扫描 + owning tests |
| 70 | `crates/agentdash-domain/src/workflow/dispatch.rs` | 业务能力 | Operation / Workflow | 搬运 canonical Operation；Runtime 仅 adapter 调用 | 存在；依赖扫描 + owning tests |
| 71 | `crates/agentdash-domain/src/workflow/mod.rs` | 业务能力 | Operation / Workflow | 搬运 canonical Operation；Runtime 仅 adapter 调用 | 存在；依赖扫描 + owning tests |
| 72 | `crates/agentdash-domain/src/workflow/repository.rs` | 业务能力 | Operation / Workflow | 搬运 canonical Operation；Runtime 仅 adapter 调用 | 存在；依赖扫描 + owning tests |
| 73 | `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 已删除；依赖扫描 + owning tests |
| 74 | `crates/agentdash-infrastructure/Cargo.toml` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 75 | `crates/agentdash-infrastructure/src/lib.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 76 | `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 存在；依赖扫描 + owning tests |
| 77 | `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 78 | `crates/agentdash-infrastructure/src/persistence/postgres/mod.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 79 | `crates/agentdash-integration-api/src/integration.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 80 | `crates/agentdash-integration-api/src/lib.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 81 | `crates/agentdash-local/src/extensions/backend_service.rs` | 业务能力 | Extension | 搬运 Component/Operation provider，删除 session action | 存在；依赖扫描 + owning tests |
| 82 | `crates/agentdash-local/src/handlers/mod.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 83 | `crates/agentdash-relay/src/protocol.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 84 | `crates/agentdash-spi/src/connector/mod.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 85 | `crates/agentdash-spi/src/lib.rs` | 集中装配 | 所属 crate / composition root | 保留薄注册、导出或测试支撑 | 存在；依赖扫描 + owning tests |
| 86 | `crates/agentdash-spi/src/session_persistence.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 存在；依赖扫描 + owning tests |
| 87 | `crates/agentdash-test-support/src/workflow.rs` | 业务能力 | Operation / Workflow | 搬运 canonical Operation；Runtime 仅 adapter 调用 | 存在；依赖扫描 + owning tests |
| 88 | `crates/agentdash-workspace-module/src/workspace_module/mod.rs` | 业务能力 | WorkspaceModule | 保留 projection-only 模型 | 存在；依赖扫描 + owning tests |
| 89 | `crates/agentdash-workspace-module/src/workspace_module/runtime_bridge.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 90 | `crates/agentdash-workspace-module/src/workspace_module/runtime_context.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 91 | `crates/agentdash-workspace-module/src/workspace_module/runtime_tool_provider.rs` | Runtime 接缝 | AgentRun / Runtime adapter | 保留 PR #93 Runtime，业务能力经窄接口重接 | 已删除；依赖扫描 + owning tests |
| 92 | `crates/agentdash-workspace-module/src/workspace_module/surface.rs` | 业务能力 | WorkspaceModule | 保留 projection-only 模型 | 已删除；依赖扫描 + owning tests |
| 93 | `crates/agentdash-workspace-module/src/workspace_module/tools.rs` | 业务能力 | WorkspaceModule | 保留 projection-only 模型 | 已删除；依赖扫描 + owning tests |
| 94 | `crates/agentdash-workspace-module/src/workspace_module/visibility.rs` | 业务能力 | WorkspaceModule | 保留 projection-only 模型 | 已删除；依赖扫描 + owning tests |
| 95 | `packages/app-web/src/pages/AgentRunWorkspacePage.workspace-module.test.ts` | 业务能力 | WorkspaceModule | 保留 projection-only 模型 | 存在；依赖扫描 + owning tests |

## 最终判定

每条路径均以 final tree 是否存在为机械证据，并由对应 owning tests、workspace clippy/test、migration guard、contract/frontend gates 与 dependency scan 共同复核。已删除路径不恢复兼容入口；保留的共享路径仅承担注册、导出、schema orchestration 或 adapter 接线。

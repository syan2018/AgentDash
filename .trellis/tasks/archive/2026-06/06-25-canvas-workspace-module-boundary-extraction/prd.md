# Canvas Workspace Module 边界预抽取

## Goal

建立 `agentdash-canvas` 的纯边界 crate，先抽取 Canvas / Workspace Module 的 identity、URI、operation key 和轻量 value/helper，供后续 Canvas interaction/observation/submit-to-Agent 在稳定业务引用上实现。

## User Value

- 后续 Canvas 交互状态能挂在 AgentRun 到 Canvas 的可见/展示引用上，而不是继续扩散到 runtime session 表述。
- Canvas module id、presentation URI、VFS mount URI、operation key 由单一 crate 提供，减少 application/API/frontend contract 漂移。
- 本阶段只做纯边界迁移，避免与 runtime gateway、VFS provider、AgentRun surface update 等行为改动混在一起。

## Requirements

- 新增 `crates/agentdash-canvas` crate，并加入 workspace 成员和 workspace dependency。
- 将 Canvas identity helpers 迁入新 crate，包括 mount id prefix、module id、presentation URI、VFS URI、provider root ref 等纯函数。
- 将 Canvas workspace operation key / renderer key 常量迁入新 crate，至少覆盖 `canvas.bind_data`、`preview`、`canvas` renderer。
- 将 Workspace Module id prefix 常量和 Canvas module ref helper 归入新 crate或通过新 crate 统一导出。
- 更新 `agentdash-application` 的 Canvas / Workspace Module 调用点，改用新 crate helper，保持行为不变。
- 保留 `agentdash-domain::canvas::Canvas` entity、repository trait、access policy 现状；本阶段不迁移数据库实体和 repository。
- 保留 RuntimeGateway、VFS service、AgentRun runtime surface update、HTTP auth、DTO generation 的现有归属；本阶段不实现 observation/interaction/submit。

## Acceptance Criteria

- [ ] `agentdash-canvas` 能独立 `cargo test -p agentdash-canvas`。
- [ ] `agentdash-application` 不再定义 Canvas identity URI helper 的权威常量，只 re-export 或直接使用 `agentdash-canvas`。
- [ ] `workspace_module` Canvas descriptor 继续生成同样的 `canvas:{canvas_mount_id}`、`canvas://{canvas_mount_id}`、`canvas.bind_data`。
- [ ] `cargo check --workspace` 通过。
- [ ] 相关 Canvas/workspace_module 单元测试通过或被等价更新。

## Out Of Scope

- 不新增 observation / interaction / submit-to-Agent API。
- 不新增数据库 migration。
- 不迁移 Canvas entity/repository 或 PostgreSQL implementation。
- 不拆 `agentdash-workspace-module`，除非实现中发现 prefix/helper 无法干净放入 `agentdash-canvas`；如需拆分，先回到父任务更新设计。

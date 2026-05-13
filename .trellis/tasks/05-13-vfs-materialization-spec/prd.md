# VFS 物化规则收口与规范化

## Goal

把云端 VFS 资源物化到本机的规则从“每次工具调用生成临时目录”收口为“按资源 root 稳定映射到本机目录”。默认可共享资源进入本机公共稳定物化区，只有语义明确绑定 session 的资源（尤其 lifecycle 投影）进入 session scope，并形成可执行的 spec 约束后续实现。

## What I already know

- 当前实现里 `plan_id` 每次生成 UUID，且本机只读 cache 路径包含 `session_id/plan_id`，导致同一资源多次物化落到多个无关目录。
- `skill-assets://skills/foo`、内置 skill scripts/references/assets 等资源是全局公共内容，不应默认放在 session 层级。
- `uv init skill-assets://skills/foo` 这类目录参数应进入本机公共稳定 workdir，而不是一次性临时目录。
- `lifecycle_vfs` 是 run/step/session 的动态投影，若需要物化，应跟随 session scope。
- 重新物化通常只应在明确知道云端 VFS 资源更新、显式 refresh、manifest 缺失或冲突时触发，不应每次 shell/MCP 调用都做复杂同步。
- `backend_id` / `local-dev-1` 是 relay 路由身份，不应默认出现在本机用户可见路径中。

## Requirements

- 新增明确的 VFS 物化 spec，定义 scope、access mode、本机目录布局、provider 映射规则、刷新/失效规则和测试要求。
- 明确默认策略：可共享 VFS 资源默认公共稳定物化；session 派生资源才进入 session scope。
- 明确 `skill_asset_fs` 的 root 规则：skill 脚本/目录均以 `skills/{skill_key}` 为物化 root，保留 `SKILL.md / scripts / references / assets` 相对布局。
- 明确 `lifecycle_vfs` 的 root 和 scope：复用资源组规划规则，但落在 `sessions/{session_id}` 下。
- 明确 `plan_id / turn_id / tool_call_id / backend_id` 只进入 manifest/audit，不参与稳定路径。
- 更新后端规范索引，让后续实现 VFS 物化时必须读取该 spec。

## Acceptance Criteria

- [x] `.trellis/spec/backend/vfs/` 下存在独立 VFS 物化 spec。
- [x] 后端规范索引能链接到该 spec。
- [x] spec 明确列出不同 VFS/provider 的默认 scope/access/root 映射。
- [x] spec 明确公共物化、session 物化、workdir、temporary 的路径布局。
- [x] spec 明确 `skill-assets://skills/foo` 与 `lifecycle://skills/foo` 的差异。
- [x] spec 明确现有实现需要修正的测试断言方向。

## Definition of Done

- 文档使用中文。
- 规范描述足够具体，可直接指导后续代码实现。
- 工作区无意外脏改动。

## Technical Approach

新增一份专门的 `vfs-materialization.md`，避免把物化细节塞进已有 `vfs-access.md` 的通用 VFS 访问契约中。`vfs-access.md` 继续定义统一 VFS / provider / runtime tool 边界；新 spec 专注于“云端 VFS URI 如何落成本机 path/URL/workdir”。

## Decision (ADR-lite)

**Context**: 当前物化实现把执行归因字段用于本机路径，导致公共资源不可读、不可复用，且 session 层级承担了过多职责。

**Decision**: 采用“默认公共稳定物化，session 资源显式 session scope”的规则。Materialization planner 输出统一 policy；local store 按 policy 选择 `readonly`、`workdirs`、`sessions` 或 `temp` 路径。

**Consequences**: 常见 skill/assets 场景变简单且稳定；lifecycle 等动态投影仍保留 session 隔离；后续 provider 扩展只需补充 provider -> materialization policy 映射。

## Out of Scope

- 本任务不直接改 Rust 实现。
- 本任务不恢复或实现 localhost URL server。
- 本任务不实现 VFS 写回云端；workdir publish/import 需要独立显式能力。
- 本任务不设计完整实时同步，只规定显式更新/refresh 驱动的失效模型。

## Technical Notes

- 相关现有 spec：`.trellis/spec/backend/vfs/vfs-access.md`
- 相关现有任务：`.trellis/tasks/04-08-cross-mount-shell-materialization/prd.md`
- 相关代码观察：
  - `crates/agentdash-application/src/vfs/materialization.rs` 当前生成 UUID `plan_id`
  - `crates/agentdash-local/src/materialization.rs` 当前使用 `backend_id/session_id/plan_id` 组成只读 cache 路径
  - `crates/agentdash-application/src/session/hub/tool_builder.rs` 的 relay MCP RuntimeGateway 入口存在 context 不一致风险

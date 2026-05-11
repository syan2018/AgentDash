# 主线 3：Setup Action Plane

## Goal

将创建 Session 前或准备运行环境阶段的操作纳入 Runtime Gateway 统一协议，避免 workspace detect、browse directory、MCP probe、环境准备等能力长期保留独立业务道路。Setup Action 不进入普通 Session Runtime Surface，但必须复用 Gateway 的 Invocation / Provider / Audit 模型。

## Scope

- Setup Action context 与 policy。
- workspace detect / detect git。
- browse directory。
- MCP transport probe。
- workspace prepare / environment prepare 的归属梳理。
- 现有 API thin route 迁移策略。

## Dependencies

- 依赖主线 1 的 Gateway Core Protocol。
- 复用现有 local 实现：
  - `workspace_prepare`
  - `workspace_detect`
  - `browse_directory`
  - `mcp_probe_transport`
  - terminal relay/API 通路

## Execution Plan

1. 定义 Setup Context：
   - actor：`platform_user` / `environment_setup`。
   - 目标：`backend_id`、候选 `root_ref`、可选 `project_id` / `workspace_id`。
   - 输入：transport config、path、workspace identity contract 等。
2. 定义 Setup Action allowlist：
   - `workspace.detect`
   - `workspace.detect_git`
   - `workspace.browse_directory`
   - `mcp.probe_transport`
   - `backend.probe`
   - `environment.prepare`
3. 实现 provider 适配：
   - 通过 BackendRegistry 发送现有 relay command。
   - 不重写 local handler。
   - 统一 response / error / timeout。
4. 迁移 API route 为 thin route：
   - route 负责 auth、参数解析、调用 Gateway。
   - 业务判断和 relay 调用进入 Gateway provider。
5. 明确 workspace prepare 两种形态：
   - Setup prepare：用户/平台在创建或校验 binding 时触发。
   - Session preflight prepare：Session 启动或首 prompt 前触发。
6. 测试：
   - Setup action 无权限时拒绝。
   - 非 setup actor 不可调用 setup action。
   - API thin route 与原响应语义兼容项目当前状态。
   - provider 超时和 relay offline 统一错误。

## Acceptance Criteria

- Setup Action 纳入 Runtime Gateway，但不出现在普通 Session Runtime Surface。
- 现有 API 不再直接拼 relay command 作为业务实现主体。
- workspace detect、browse directory、MCP probe 至少有明确迁移路径。
- local 端已有 handler 不被重写，只复用。
- Invocation trace 覆盖 setup action。

## Risks

- Setup Action 没有 `CapabilityState`，权限必须来自项目权限、backend 权限和配置流程上下文。
- browse directory 容易变成过宽本机枚举能力，需要限制 actor 和 UI 场景。
- MCP probe 可能启动本机 stdio 进程，必须保留超时和清理。

## First PR Shape

- 先迁移 `mcp.probe_transport` 或 `workspace.detect` 中一个最小闭环。
- 保留 API 路径不变，但内部转 Gateway。
- 确认 trace / error / timeout 模型后再迁移其它 setup route。

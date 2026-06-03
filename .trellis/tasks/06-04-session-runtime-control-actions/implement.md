# 实施计划：Session 控制动作模型修复

## Phase 0: Pre-Dev Context

- 读取 `.trellis/spec/frontend/index.md`、`.trellis/spec/backend/index.md`、`.trellis/spec/cross-layer/index.md`、`.trellis/spec/shared/index.md`。
- 按索引补读类型安全、状态管理、API 合同、migration、测试相关规范。
- 确认当前工作区干净，避免混入上个任务归档文件。

## Phase 1: Contract And Backend Control View

1. 更新 `crates/agentdash-contracts/src/workflow.rs`：
   - 新增 runtime control plane / action availability DTO。
   - 新增 steering request / response DTO。
   - 移除或停止暴露旧 `can_send/send_unavailable_reason` 控制字段。

2. 更新 `crates/agentdash-api/src/routes/sessions.rs` 的 runtime-control：
   - session meta 存在但无 anchor 时返回 `unbound_trace`，不返回 BadRequest。
   - anchor 存在时加载 run/agent/frame，计算 control plane status。
   - 根据 execution state、agent status、frame、connector capabilities 计算 actions。

3. 更新合同生成输出：
   - 运行项目现有 contract generation/check 命令。
   - 同步 `packages/app-web/src/generated/*`。

## Phase 2: Backend Steering Path

1. 在 connector capability 中加入 `supports_steering`。
2. 在 `AgentConnector` / `SessionControlService` 增加显式 `steer_session` 方法，参数保留 prompt blocks。
3. 为 in-process connector 实现 `steer_session`：
   - 将 prompt blocks 转换为 agent user message content。
   - 调用底层 agent steering queue。
4. 为 relay/codex connector 实现 `steer_session`：
   - 扩展 backend transport 端口与 relay 请求类型。
   - 将 prompt blocks 发送到 codex steering 控制协议。
   - 保持原 runtime session，不创建新 prompt/turn。
5. 新增 `LifecycleAgentSteeringService`：
   - 解析 RuntimeSessionExecutionAnchor。
   - 校验 run/agent/frame 一致。
   - 仅 running session 接受 steer。
   - 调用 `SessionControlService.steer_session`。
6. 新增 API route：
   - `POST /lifecycle-agents/by-runtime-session/{runtime_session_id}/steering-messages`
   - Edit 权限校验与普通 lifecycle message 一致。

## Phase 3: Frontend Service And State Model

1. 更新 `packages/app-web/src/services/lifecycle.ts`：
   - 新增 `sendLifecycleAgentSteeringMessageByRuntimeSession`。
   - 更新 runtime-control 类型消费。

2. 重构 `packages/app-web/src/pages/SessionPage.tsx`：
   - Draft 生成 `start_draft` action。
   - Runtime 由 `runtimeControl.actions` 生成 `send_next/steer/none` action。
   - cancel 独立传入 chat view。
   - 删除 `sessionSendReady` 作为 dispatcher 判断的职责。

3. 重构 `SessionChatViewTypes.ts` / `SessionChatView.tsx` / `SessionChatViewParts.tsx`：
   - 用 `SessionChatControlState` 替代 `customSend + sendUnavailableReason`。
   - 主动作按 action kind 分派。
   - Ctrl+Enter 触发当前 primary action。
   - 运行中 cancel 独立按钮渲染。
   - 输入 placeholder 与说明行来自 action/control reason。

## Phase 4: Tests

Backend tests:

- runtime-control:
  - unbound trace 返回 readonly actions。
  - anchored idle 返回 send_next enabled。
  - anchored running 返回 send_next disabled、steer/cancel action 正确。
  - terminal / frame missing 返回明确 disabled reason。
- steering service / route:
  - 缺 anchor 拒绝。
  - 非 running 拒绝。
  - connector 不支持 steering 拒绝并给出明确错误。
  - running + supported connector 成功入队，不创建新 turn。
- relay/codex steering:
  - relay connector 调用 transport steer，而不是 prompt。
  - codex backend 收到 steer command 后注入当前 session。
  - steering 失败返回明确错误给 API。

Frontend tests:

- composer helper / view:
  - Draft 显示开始动作。
  - idle runtime 显示发送动作。
  - running runtime 显示 steer 主动作和独立取消。
  - running runtime 不显示“未连接 dispatcher”。
  - readonly trace 显示只读原因。

## Phase 5: Validation

建议命令：

```powershell
pnpm run contracts:check
cargo test -p agentdash-application workflow::agent_message workflow::project_agent_session_start session::hub
cargo test -p agentdash-api
pnpm --filter app-web run typecheck
pnpm --filter app-web test -- src/features/session/ui/SessionChatView.test.tsx
```

根据实际改动补跑更精确的 Rust 包测试；如果 contract generation 修改输出，先跑生成命令再跑 `contracts:check`。

## Risk Points

- 不要把 steer 接到 `dispatch_user_message`，否则会重新进入 prompt claim 并制造并发 turn 冲突。
- 不要让前端通过 session 存在性或 stream 连接状态推导 lifecycle 控制面；runtime-control action set 是事实源。
- 不要复用 notification 文案作为用户 steer；用户输入需要独立审计语义和 prompt block 载荷。
- 如需数据库变更，只能新增 migration 文件，不能修改既有 migration。

## Review Gate

实现前不再等待 relay/codex scope 决策：relay/codex steering 属于本任务必做范围。

# 主线 4：Runtime Consumers

## Goal

让 Canvas、Agent、Workflow node、平台 UI 等消费端通过统一 Runtime Gateway 调用能力，避免每个消费端各自维护 MCP / relay / VFS / setup 的调用链。

## Scope

- Canvas Runtime Bridge SDK。
- Agent RuntimeActionToolAdapter。
- Workflow node action 调用。
- 平台 UI 调用 setup/session action 的 thin client。
- 用户触发、确认、结果展示和错误展示。

## Dependencies

- 依赖主线 1 的协议。
- Agent 消费依赖主线 2 的 Session Runtime Plane。
- 平台 UI 的 setup 消费依赖主线 3 的 Setup Action Plane。

## Execution Plan

1. Canvas Bridge SDK：
   - iframe 内只暴露 `window.agentdash.invoke(action_id, input)`。
   - 父页面校验 frame_id、nonce、用户手势和绑定 session_id。
   - Canvas 不直接知道 relay / MCP / HTTP 细节。
2. Canvas Runtime Snapshot 扩展：
   - 返回当前 session surface 中允许 Canvas 看到的 action manifest。
   - 不返回 secret、token、backend 内部细节。
3. Agent adapter：
   - 将选定 Runtime Action 包装成 AgentTool。
   - 由 Gateway 执行 policy/provider，不让 adapter 直接调底层。
4. Workflow node：
   - 定义 workflow action step 如何引用 Runtime Action。
   - WorkflowRun / LifecycleNode 必须关联或创建受控 Session。
5. Platform UI：
   - workspace 创建/绑定页继续使用原 API 路径，但后端 route 走 Gateway。
   - 后续可引入通用 action invoke client。
6. UX 与安全：
   - Canvas 用户点击触发，不允许自动执行敏感 action。
   - 对高风险 action 加确认。
   - 统一展示 invocation trace_id 和错误。

## Acceptance Criteria

- Canvas 是 Runtime Client，不是权限来源。
- Agent 和 Workflow 不维护自己的底层 provider 调用链。
- 消费端只拿到 action manifest，不拿到底层 relay/MCP secret。
- 用户触发类调用可被追踪到 actor、session、action、trace。
- 平台 UI 的 setup 操作能逐步迁移而不破坏现有体验。

## Risks

- Canvas iframe sandbox 与远程 import map 存在数据外传风险。
- Workflow node 若绕过 Session 创建独立调用链，会破坏 Session-bound 原则。
- Agent adapter 若暴露过多 setup action，会让 Agent 获得配置期能力。

## First PR Shape

- 先实现 Agent adapter 或 Canvas bridge 二选一。
- 推荐先 Agent adapter，因其更贴近现有 tool pipeline，验证 Gateway policy 更直接。
- Canvas bridge 在 action manifest 和安全策略稳定后再接。
